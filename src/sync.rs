use core::fmt::Display;
use std::{
    io::{self, BufWriter, Read, Write},
    net::TcpStream,
};

use tracing::{debug, instrument};

use crate::v3::{
    connect::{ConnAck, Connect},
    Decode, DecodeError, Encode, EncodeError, Writer,
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
}

impl Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReadError::Disconnected => write!(f, "the connection was closed gracefully"),
            ReadError::Decode(_) => write!(f, "couldn't decode the packet"),
            ReadError::Read(_) => write!(f, "couldn't read from the connection"),
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReadError::Disconnected => None,
            ReadError::Decode(err) => Some(err),
            ReadError::Read(err) => Some(err),
        }
    }
}

#[derive(Debug)]
pub struct TcpConnection<'a> {
    reader: TcpReader<'a>,
    writer: BufWriter<&'a TcpStream>,
}

impl<'c> TcpConnection<'c> {
    pub fn new(connection: &'c TcpStream) -> Self {
        // Start with default capacity
        let buf = vec![0; 1024];

        Self {
            reader: TcpReader {
                buf: ReadBuffer::new(buf),
                stream: connection,
            },
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

#[derive(Debug)]
struct ReadBuffer {
    buf: Vec<u8>,
    data_start: usize,
    data_end: usize,
}

impl ReadBuffer {
    fn new(buf: Vec<u8>) -> Self {
        Self {
            buf,
            data_start: 0,
            data_end: 0,
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
        T: Decode<'a>,
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

    fn reserve(&mut self, needed: usize) {
        if self.writable_len() >= needed {
            return;
        }

        self.compact();

        let writable_len = self.writable_len();
        if writable_len >= needed {
            return;
        }

        // |--            len            --|
        // |-- filled --|-- writable_len --|
        //              |--      needed      --|
        let new_len = self
            .buf
            .len()
            .saturating_sub(needed.saturating_sub(writable_len));

        self.buf.resize(new_len, 0);
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
struct TcpReader<'a> {
    buf: ReadBuffer,
    stream: &'a TcpStream,
}

impl<'c> TcpReader<'c> {
    fn read(&mut self) -> Result<(), ReadError> {
        // We need to make sure there is free space in the buffer
        let read = self.stream.read(self.buf.writable())?;

        if read == 0 {
            return Err(ReadError::Disconnected);
        }

        self.buf.filled(read);

        Ok(())
    }

    fn recv<T>(&mut self) -> Result<T, ReadError>
    where
        for<'a> T: Decode<'a>,
    {
        if self.buf.is_empty() {
            self.buf.reset();
        }

        loop {
            self.read()?;

            let needed = match self.buf.parse::<T>() {
                Ok(val) => return Ok(val),
                Err(DecodeError::NotEnoughBytes { needed, .. }) => needed,
                Err(err) => return Err(ReadError::Decode(err)),
            };

            self.buf.reserve(needed);
        }
    }
}

impl<'a> Writer for BufWriter<&'a TcpStream> {
    type Err = io::Error;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Err> {
        Write::write_all(self, buf)
    }
}
