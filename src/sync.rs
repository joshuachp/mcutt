//! Sync client implementation using the standard library [`TcpStream`].

use core::{fmt::Display, num::NonZeroUsize, panic};
use std::{
    collections::TryReserveError,
    io::{self, BufWriter, Read, Write},
    net::TcpStream,
};

use tracing::{error, instrument, trace};

use crate::{
    slab::{Entry, Slab},
    v3::{
        connect::{ConnAck, Connect, ReturnCode},
        header::PacketId,
        publish::{ClientPublishOwned, ClientPublishRef, Publish, PublishOwned, Qos},
        DecodeError, DecodePacket, EncodeError, EncodePacket, Writer, MAX_PACKET_SIZE,
    },
};

/// Error returned by the MQTT connection
#[derive(Debug)]
#[non_exhaustive]
pub enum ConnectError {
    /// Error returned by the [`ConnAck`].
    Connect(ReturnCode),
    /// Couldn't encode or write an outgoing packet.
    Encode(EncodeError<io::Error>),
    /// Couldn't decode the packet.
    Decode(DecodeError),
    /// Couldn't read an incoming packet.
    Read(ReadError),
    /// Couldn't find a packet id.
    MissingPkid(PacketId),
    /// Couldn't send packet, too many other outgoing
    ToManyOutgoing,
}

impl Display for ConnectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConnectError::Connect(code) => {
                write!(f, "connect failed with return code {code}")
            }
            ConnectError::Encode(_) => write!(f, "couldn't encode the packet"),
            ConnectError::Decode(_) => write!(f, "couldn't decode the packet"),
            ConnectError::Read(_) => write!(f, "couldn't write to the connection"),
            ConnectError::MissingPkid(id) => write!(f, "couldn't find packet identifier {id}"),
            ConnectError::ToManyOutgoing => write!(f, "too many outgoing packets"),
        }
    }
}

impl std::error::Error for ConnectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConnectError::Connect(_)
            | ConnectError::ToManyOutgoing
            | ConnectError::MissingPkid(_) => None,
            ConnectError::Encode(err) => Some(err),
            ConnectError::Decode(err) => Some(err),
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

impl From<DecodeError> for ConnectError {
    fn from(v: DecodeError) -> Self {
        Self::Decode(v)
    }
}

/// Couldn't read or decode an incoming packet.
#[derive(Debug)]
pub enum ReadError {
    /// The connection was disconnected gracefully.
    ///
    /// This means we read 0 bytes from the [`TcpStream`].
    Disconnected,
    /// Couldn't decode the packet.
    Decode(DecodeError),
    /// The underling [`Read`] operation failed.
    Read(io::Error),
    /// Non enough memory to read the packet.
    OutOfMemory {
        /// The configured maximum for the [`ReadBuffer`]
        max: NonZeroUsize,
        /// The required length of the packet.
        required: usize,
    },
    /// Couldn't allocate the memory for the buffer.
    ///
    /// This error is returned by the [`Vec::try_reserve_exact`] call.
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

/// The MQTT connection, for both reading  and writing.
#[derive(Debug)]
pub struct Connection<'a> {
    reader: TcpReader<'a>,
    writer: BufWriter<&'a TcpStream>,
    outgoing: Slab<Vec<Entry<PublishOwned>>>,
}

impl<'c> Connection<'c> {
    /// Creates a new connection from a socket.
    #[must_use]
    pub fn new(connection: &'c TcpStream) -> Self {
        let read_buffer = ReadBuffer::default();

        Self::with_read_buffer(connection, read_buffer)
    }

    /// Specify a configurable [`ReadBuffer`] for the connection.
    #[must_use]
    pub fn with_read_buffer(connection: &'c TcpStream, read_buffer: ReadBuffer) -> Self {
        Self {
            reader: TcpReader::new(read_buffer, connection),
            writer: BufWriter::new(connection),
            outgoing: Slab::new(u16::MAX.into()),
        }
    }

    /// Sends a [`Connect`] packet and waits for the Server's [`ConnAck`].
    #[instrument(skip(self))]
    pub fn connect(&mut self, connect: &Connect) -> Result<ConnAck, ConnectError> {
        trace!("sending the CONNECT packet");
        connect.write(&mut self.writer)?;

        self.writer.flush().map_err(EncodeError::Write)?;

        trace!("waiting for the CONNACK");
        self.reader.recv().map_err(ConnectError::Read)
    }

    /// Sends a [`ClientPublishRef`] to the connection with QoS at most once (0).
    #[instrument(skip(self))]
    pub fn publish(&mut self, publish: ClientPublishRef) -> Result<(), ConnectError> {
        let publish = Publish::from(publish);

        publish.write(&mut self.writer)?;

        Ok(())
    }

    /// Sends a PUBLISH to the connection with QoS > 0.
    ///
    /// It will return the [`PacketId`] of the publish.
    #[instrument(skip(self))]
    pub fn publish_with_qos(
        &mut self,
        publish: ClientPublishOwned,
        qos: Qos,
    ) -> Result<PacketId, ConnectError> {
        let pkid = self
            .outgoing
            .try_insert(|idx| {
                PacketId::try_from(idx.saturating_add(1))
                    .map_err(DecodeError::PacketIdentifier)
                    .map(|pkid| (PublishOwned::with_qos(pkid, publish, qos), pkid))
            })?
            .ok_or(ConnectError::ToManyOutgoing)?;

        let publish = self
            .outgoing
            .get(usize::from(pkid).saturating_sub(1))
            .ok_or(ConnectError::MissingPkid(pkid))?;

        publish.write(&mut self.writer)?;

        Ok(pkid)
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
    /// The default initial capacity of the buffer.
    pub const DEFULAT_INITIAL: NonZeroUsize = const_non_zero(8 * 1024);

    /// The default maximum size that the buffer will grow to.
    pub const DEFULAT_MAX_SIZE: NonZeroUsize = const_non_zero(MAX_PACKET_SIZE);

    /// Create a new buffer with the specified initial capacity.
    #[must_use]
    pub fn new(initial: NonZeroUsize) -> Self {
        Self::with_max_size(initial, Self::DEFULAT_MAX_SIZE)
    }

    /// Create a new buffer with the specified initial capacity and maximum size.
    #[must_use]
    pub fn with_max_size(initial: NonZeroUsize, max_size: NonZeroUsize) -> Self {
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
        self.buf.rotate_left(self.data_start);
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
                Err(err) => {
                    if err.must_close() {
                        if let Err(err) = self.stream.shutdown(std::net::Shutdown::Both) {
                            error!(error = %err, "couldnt shutdown socket");
                        }
                    }

                    return Err(ReadError::Decode(err));
                }
            };

            self.buf.reserve(needed)?;

            self.read()?;
        }
    }
}

impl<'a> Writer for BufWriter<&'a TcpStream> {
    type Err = io::Error;

    fn write_slice(&mut self, buf: &[u8]) -> Result<usize, Self::Err> {
        Write::write_all(self, buf)?;

        Ok(buf.len())
    }
}
