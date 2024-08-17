use core::{fmt::Display, num::NonZeroUsize, panic};
use std::{
    collections::TryReserveError,
    io::{self, BufWriter, Read, Write},
    net::TcpStream,
};

use tracing::{debug, instrument};

use crate::v3::{
    connect::{ConnAck, Connect},
    DecodeError, DecodePacket, Encode, EncodeError, Writer, MAX_PACKET_SIZE,
};

#[derive(Debug)]
pub enum ConnectError {
    Encode(EncodeError<io::Error>),
    Read(ReadError),
}

impl Display for ConnectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConnectError::Encode(_) => write!(f, "couldn't encode the packet"),
            ConnectError::Read(_) => write!(f, "couldn't write to the connection"),
        }
    }
}

impl std::error::Error for ConnectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConnectError::Encode(err) => Some(err),
            ConnectError::Read(err) => Some(err),
        }
    }
}

impl From<ReadError> for ConnectError {
    fn from(v: ReadError) -> Self {
        Self::Read(v)
    }
}

impl From<EncodeError<io::Error>> for ConnectError {
    fn from(v: EncodeError<io::Error>) -> Self {
        Self::Encode(v)
    }
}

#[derive(Debug)]
pub enum ReadError {
    /// Disconnected gracefully
    Disconnected,
    /// Decode error
    Decode(DecodeError),
    /// Io error
    Read(io::Error),
    /// Non enough memory to read the packet.
    OutOfMemory { max: NonZeroUsize, required: usize },
    /// Couldn't allocate the memory for the buffer
    Reserve(TryReserveError),
}

impl Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReadError::Disconnected => write!(f, "the connection was closed gracefully"),
            ReadError::Decode(_) => write!(f, "couldn't decode the packet"),
            ReadError::Read(_) => write!(f, "couldn't read from the connection"),
            ReadError::OutOfMemory { max, required } => {
                write!(f, "couldn't read the packet of {required} bytes since the maximum size of the buffer is {max}")
            }
            ReadError::Reserve(_) => write!(f, "couldn't allocate the memory for the read buffer"),
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReadError::Disconnected | ReadError::OutOfMemory { .. } => None,
            ReadError::Decode(err) => Some(err),
            ReadError::Read(err) => Some(err),
            ReadError::Reserve(err) => Some(err),
        }
    }
}

impl From<TryReserveError> for ReadError {
    fn from(value: TryReserveError) -> Self {
        ReadError::Reserve(value)
    }
}

impl From<io::Error> for ReadError {
    fn from(value: io::Error) -> Self {
        ReadError::Read(value)
    }
}

impl From<DecodeError> for ReadError {
    fn from(value: DecodeError) -> Self {
        ReadError::Decode(value)
    }
}

#[derive(Debug)]
pub struct TcpConnection<'a> {
    reader: TcpReader<'a>,
    writer: BufWriter<&'a TcpStream>,
}

impl<'c> TcpConnection<'c> {
    pub fn new(connection: &'c TcpStream) -> Self {
        let read_buffer = ReadBuffer::default();

        Self::with_read_buffer(connection, read_buffer)
    }

    pub fn with_read_buffer(connection: &'c TcpStream, read_buffer: ReadBuffer) -> Self {
        Self {
            reader: TcpReader::new(read_buffer, connection),
            writer: BufWriter::new(connection),
        }
    }

    #[instrument(skip(self))]
    pub fn connect(&mut self, connect: Connect) -> Result<ConnAck, ConnectError> {
        debug!("sending the CONNECT packet");
        connect.write(&mut self.writer)?;

        self.writer.flush().map_err(EncodeError::Write)?;

        debug!("waiting for the CONNACK");
        self.reader.recv().map_err(ConnectError::Read)
    }
}

/// Buffer to store the data from the [`TcpStream`].
#[derive(Debug)]
pub struct ReadBuffer {
    buf: Vec<u8>,
    data_start: usize,
    data_end: usize,
    max_size: NonZeroUsize,
}

/// Const constructor for the [`NonZeroUsize`] without unsafe.
const fn const_non_zero(value: usize) -> NonZeroUsize {
    let Some(value) = NonZeroUsize::new(value) else {
        panic!("BUG: non zero value passed const_non_zero");
    };

    value
}

impl ReadBuffer {
    const DEFULAT_INITIAL: NonZeroUsize = const_non_zero(8 * 1024);
    const DEFULAT_MAX_SIZE: NonZeroUsize = const_non_zero(MAX_PACKET_SIZE);

    pub fn new(initial: NonZeroUsize) -> Self {
        Self::with_max(initial, Self::DEFULAT_MAX_SIZE)
    }

    pub fn with_max(initial: NonZeroUsize, max_size: NonZeroUsize) -> Self {
        Self {
            buf: vec![0; initial.get()],
            data_start: 0,
            data_end: 0,
            max_size,
        }
    }

    fn is_empty(&self) -> bool {
        self.data_start == self.data_end
    }

    /// Reset the data portion
    fn reset(&mut self) {
        self.data_start = 0;
        self.data_end = 0;
    }

    /// Return the free portion of the buffer.
    fn writable(&mut self) -> &mut [u8] {
        &mut self.buf[self.data_end..]
    }

    /// Update the data portion size.
    fn filled(&mut self, read: usize) {
        self.data_end = self.data_end.saturating_add(read);
    }

    fn consumed(&self, remaining: &[u8]) -> usize {
        self.data_end
            .saturating_sub(self.data_start)
            .saturating_sub(remaining.len())
    }

    /// Shifts the data to the start of the buffer, so there is more free space to fill at the end.
    fn compact(&mut self) {
        self.buf.rotate_left(self.data_start)
    }

    fn parse<'a, T>(&'a mut self) -> Result<T, DecodeError>
    where
        T: DecodePacket<'a>,
    {
        match T::parse(&self.buf[self.data_start..self.data_end]) {
            Ok((val, bytes)) => {
                let consumed = self.consumed(bytes);

                self.data_start = self.data_start.saturating_add(consumed);

                Ok(val)
            }
            Err(err) => Err(err),
        }
    }

    fn writable_len(&self) -> usize {
        self.buf.len().saturating_sub(self.data_end)
    }

    fn reserve(&mut self, needed: usize) -> Result<(), ReadError> {
        if self.writable_len() >= needed {
            return Ok(());
        }

        // No-op if empty
        self.compact();

        let writable_len = self.writable_len();
        if writable_len >= needed {
            return Ok(());
        }

        // |--            len            --|
        // |-- filled --|-- writable_len --|
        //              |--      needed      --|
        let new_len = self
            .buf
            .len()
            .saturating_sub(needed.saturating_sub(writable_len));

        if new_len > self.max_size.get() {
            return Err(ReadError::OutOfMemory {
                max: self.max_size,
                required: new_len,
            });
        }

        // Over allocate to prevent frequent resizing, but cap it at max. We already checked that
        // the new length.
        let additional = self
            .buf
            .capacity()
            .saturating_mul(2)
            .max(self.max_size.get())
            .saturating_sub(self.buf.len());

        self.buf.try_reserve_exact(additional)?;

        self.buf.resize(new_len, 0);
        self.buf.resize(new_len, 0);

        Ok(())
    }
}

impl Default for ReadBuffer {
    fn default() -> Self {
        ReadBuffer::new(Self::DEFULAT_INITIAL)
    }
}

#[derive(Debug)]
struct TcpReader<'a> {
    buf: ReadBuffer,
    stream: &'a TcpStream,
}

impl<'c> TcpReader<'c> {
    fn new(buf: ReadBuffer, stream: &'c TcpStream) -> Self {
        Self { buf, stream }
    }

    fn read(&mut self) -> Result<(), ReadError> {
        // We need to make sure there is free space in the buffer
        let read = self.stream.read(self.buf.writable())?;

        if read == 0 {
            return Err(ReadError::Disconnected);
        }

        self.buf.filled(read);

        Ok(())
    }

    /// Parses the next packet.
    ///
    /// If the buffer is empty, reads from the connection. Otherwise tries to parse the data in the
    /// buffer. If more bytes are needed, makes sure the capacity is reserved in the array and reads
    /// more from the connection.
    fn recv<T>(&mut self) -> Result<T, ReadError>
    where
        for<'a> T: DecodePacket<'a>,
    {
        if self.buf.is_empty() {
            self.buf.reset();

            self.read()?;
        }

        loop {
            let needed = match self.buf.parse::<T>() {
                Ok(val) => return Ok(val),
                Err(DecodeError::NotEnoughBytes { needed, .. }) => needed,
                Err(err) => return Err(ReadError::Decode(err)),
            };

            self.buf.reserve(needed)?;

            self.read()?;
        }
    }
}

impl<'a> Writer for BufWriter<&'a TcpStream> {
    type Err = io::Error;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Err> {
        Write::write_all(self, buf)
    }
}
