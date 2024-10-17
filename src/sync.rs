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
        header::{FixedHeader, PacketId},
        packet::Packet,
        publish::{
            ClientPublishOwned, ClientPublishRef, ClientQos, PubAck, PubComp, PubRec, Publish,
            PublishOwned,
        },
        Decode, DecodeError, EncodeError, EncodePacket, Writer, MAX_PACKET_SIZE,
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
    /// A CONNACK packet was expected
    Connack,
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
            ReadError::Connack => write!(f, "a CONNACK packet was expencted"),
            ReadError::Reserve(_) => write!(f, "couldn't allocate the memory for the read buffer"),
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReadError::Disconnected | ReadError::OutOfMemory { .. } | ReadError::Connack => None,
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
struct State {
    slab: Slab<Vec<Entry<Outgoing>>>,
}

impl State {
    fn new() -> Self {
        Self {
            slab: Slab::new(u16::MAX),
        }
    }

    fn handle_puback(&mut self, puback: &PubAck) {
        let idx = puback.pkid().get().saturating_sub(1);
        let Some(publish) = self.slab.get(idx).and_then(Outgoing::as_publish) else {
            error!("received {} for missing PUBLISH", puback);

            return;
        };

        if !publish.qos().is_qos1() {
            error!("received {} for publish with {}", puback, publish.qos());
            return;
        }

        trace!("acknowledged publish {}", puback.pkid());

        self.slab.remove(idx);
    }

    fn handle_pubrec(&mut self, pubrec: &PubRec) {
        let idx = pubrec.pkid().get().saturating_sub(1);
        let Some(outgoing) = self.slab.get_mut(idx) else {
            error!("received {} for missing PUBLISH", pubrec);

            return;
        };

        let Some(publish) = outgoing.as_publish() else {
            error!("received {} for missing PUBLISH", pubrec);

            return;
        };

        if !publish.qos().is_qos2() {
            error!("received {} for publish with {}", pubrec, publish.qos());
            return;
        }

        trace!("acknowledged with PUBREC publish {}", pubrec.pkid());

        *outgoing = Outgoing::PubRec;
    }
}

#[derive(Debug, Clone)]
enum Outgoing {
    Publish(PublishOwned),
    PubRec,
}

impl Outgoing {
    fn as_publish(&self) -> Option<&PublishOwned> {
        if let Self::Publish(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

/// The MQTT connection, for both reading  and writing.
#[derive(Debug)]
pub struct Connection<'a> {
    reader: TcpReader<'a>,
    writer: BufWriter<&'a TcpStream>,
    outgoing: State,
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
            outgoing: State::new(),
        }
    }

    /// Sends a [`Connect`] packet and waits for the Server's [`ConnAck`].
    #[instrument(skip(self))]
    pub fn connect(&mut self, connect: &Connect) -> Result<ConnAck, ConnectError> {
        trace!("sending the CONNECT packet");
        connect.write(&mut self.writer)?;

        self.writer.flush().map_err(EncodeError::Write)?;

        trace!("waiting for the CONNACK");
        self.reader
            .recv()
            .map_err(ConnectError::Read)
            .and_then(|p| {
                p.try_into_conn_ack()
                    .map_err(|_| ConnectError::Read(ReadError::Connack))
            })
    }

    /// Sends a [`ClientPublishRef`] to the connection with QoS at most once (0).
    #[instrument(skip(self))]
    pub fn publish(&mut self, publish: ClientPublishRef) -> Result<(), ConnectError> {
        let publish = Publish::from(publish);

        publish.write(&mut self.writer)?;

        self.writer.flush().map_err(EncodeError::Write)?;

        Ok(())
    }

    /// Sends a PUBLISH to the connection with QoS 1 or 2.
    ///
    /// It will return the [`PacketId`] of the publish.
    #[instrument(skip(self))]
    pub fn publish_with_qos(
        &mut self,
        publish: ClientPublishOwned,
        qos: ClientQos,
    ) -> Result<PacketId, ConnectError> {
        let pkid = self
            .outgoing
            .slab
            .try_insert(|idx| {
                PacketId::try_from(idx.saturating_add(1))
                    .map_err(DecodeError::PacketIdentifier)
                    .map(|pkid| {
                        (
                            Outgoing::Publish(PublishOwned::with_qos(pkid, publish, qos)),
                            pkid,
                        )
                    })
            })?
            .ok_or(ConnectError::ToManyOutgoing)?;

        let publish = self
            .outgoing
            .slab
            .get(pkid.get().saturating_sub(1))
            .and_then(Outgoing::as_publish)
            .ok_or(ConnectError::MissingPkid(pkid))?;

        publish.write(&mut self.writer)?;

        self.writer.flush().map_err(EncodeError::Write)?;

        Ok(pkid)
    }

    /// Returns the next packet received from the server.
    ///
    /// It will also handle keeping the connection alive and the control messages for acknowledge
    /// packets for QoS 2.
    #[instrument(skip(self))]
    #[must_use]
    pub fn revc<'a>(&'a mut self) -> Result<Packet<'a>, ConnectError> {
        let packet = self.reader.recv().map_err(ConnectError::Read)?;

        match &packet {
            Packet::ConnAck(_) => {}
            Packet::Publish(_publish) => todo!(),
            Packet::PubAck(puback) => {
                self.outgoing.handle_puback(puback);
            }
            Packet::PubRec(pubrec) => {
                self.outgoing.handle_pubrec(pubrec);
            }
            Packet::PubRel(pubrel) => {
                trace!("sending PUBCOMP for {}", pubrel);

                PubComp::new(pubrel.pkid()).write(&mut self.writer)?;
            }
            Packet::PubComp(_) => todo!(),
            Packet::SubAck(_) => todo!(),
            Packet::UnsubAck(_) => todo!(),
            Packet::PingResp(_) => todo!(),
        }

        Ok(packet)
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

    fn parse_packet(&mut self, header: FixedHeader) -> Result<Packet, DecodeError> {
        let remaining_length =
            usize::try_from(header.remaining_length()).map_err(DecodeError::RemainingLength)?;

        let new_start = self.data_start.saturating_add(remaining_length);
        let start = std::mem::replace(&mut self.data_start, new_start);

        debug_assert!(self.data_start <= self.data_end);

        let bytes = &self.buf[start..self.data_start];

        match Packet::parse_with_header(header, bytes) {
            Ok(val) => {
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

    fn readable_len(&self) -> usize {
        self.data_end.saturating_sub(self.data_start)
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
    fn recv(&mut self) -> Result<Packet, ReadError> {
        if self.buf.is_empty() {
            self.buf.reset();

            self.read()?;
        }

        let fixed_header: FixedHeader = loop {
            match self.buf.parse() {
                Ok(header) => break header,
                Err(DecodeError::NotEnoughBytes { needed }) => {
                    self.buf.reserve(needed)?;

                    self.read()?;
                }
                Err(err) => {
                    return Err(ReadError::Decode(err));
                }
            }
        };

        let needed = fixed_header
            .remaining_length()
            .try_into()
            .map_err(DecodeError::RemainingLength)?;

        self.buf.reserve(needed)?;

        while self.buf.readable_len() < needed {
            self.read()?;
        }

        let packet = self
            .buf
            .parse_packet(fixed_header)
            .map_err(ReadError::Decode)?;

        Ok(packet)
    }
}

impl<'a> Writer for BufWriter<&'a TcpStream> {
    type Err = io::Error;

    fn write_slice(&mut self, buf: &[u8]) -> Result<usize, Self::Err> {
        Write::write_all(self, buf)?;

        Ok(buf.len())
    }
}

/// Const constructor for the [`NonZeroUsize`] without unsafe.
const fn const_non_zero(value: usize) -> NonZeroUsize {
    let Some(value) = NonZeroUsize::new(value) else {
        panic!("BUG: non zero value passed const_non_zero");
    };

    value
}
