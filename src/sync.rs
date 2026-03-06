//! Sync clientimplementation using the standard library [`TcpStream`].

use std::borrow::Borrow;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::{debug, info, instrument, trace};

use crate::bytes::{Decode, Encode, Error, ErrorKind};
use crate::v3::packets::common::fixed::FixedHeaderSlice;
use crate::v3::packets::common::fixed::remaining_length::RemainingLengthSlice;
use crate::v3::packets::connect::Connect;
use crate::v3::packets::connect::ack::ConnAck;
use crate::v3::packets::connect::builder::ConnectBuilder;
use crate::v3::packets::disconnect::Disconnect;
use crate::v3::packets::packet::Packet;
use crate::v3::packets::publish::Publish;
use crate::v3::packets::publish::builder::PublishBuilder;
use crate::v3::packets::subscribe::Subscribe;
use crate::v3::packets::subscribe::builder::SubscribeBuilder;
use crate::v3::packets::unsubscribe::Unsubscribe;
use crate::v3::packets::unsubscribe::builder::UnsubscribeBuilder;

/// Sends packet to the broker.
pub trait Sender {
    /// Sends a PUBLISH packet.
    fn publish(&self, publish: &PublishBuilder) -> Result<(), Error>;

    /// Sends a SUBSCRIBE packet.
    fn subscribe(&self, subscribe: &SubscribeBuilder) -> Result<(), Error>;

    /// Sends an UNSUBSCRIBE packet.
    fn unsubscribe<T, S>(&self, unsubscribe: &UnsubscribeBuilder<T>) -> Result<(), Error>
    where
        for<'t> &'t T: IntoIterator<Item = &'t S>,
        S: Borrow<str>;

    /// Sends a DISCONNECT packet.
    fn disconnect(&self) -> Result<(), Error>;
}

/// Receives packet from the broker.
pub trait Receiver {
    /// Receives and parses a packet.
    fn recv(&mut self) -> Result<Packet<'_>, Error>;
}

/// The MQTT connection, for both reading  and writing.
#[derive(Debug)]
pub struct Connection<W, R> {
    writer: Arc<WriterHalf<W>>,
    reader: ReaderHalf<R>,
}

impl<W, R> Connection<W, R> {
    /// Creates a new connection from a socket.
    #[must_use]
    pub fn new(writer: W, reader: R) -> Self {
        Self {
            writer: Arc::new(WriterHalf {
                sent_flag: AtomicBool::new(false),
                inner: writer,
            }),
            reader: ReaderHalf {
                inner: Reader::with_capacity(reader, 8 * 1024).expect("size is greater than 8K"),
            },
        }
    }

    /// Sends a [`Connect`] packet and waits for the Server's [`ConnAck`].
    #[instrument(skip(self, connect))]
    pub fn connect(&mut self, connect: ConnectBuilder) -> Result<ConnAck, Error>
    where
        for<'a> &'a W: Write,
        R: Read,
    {
        trace!("sending CONNECT packet");

        let written = Connect::write_sync(&mut &self.writer.inner, &connect)?;

        self.writer.flush()?;

        self.writer.sent_flag.store(true, Ordering::Release);

        trace!(written);

        trace!("waiting for CONNACK packet");

        let read = self.reader.inner.read()?;

        ConnAck::consume(read).map(|connack| {
            trace!("CONNACK received");

            *connack
        })
    }

    /// Sends a PUBLISH packet with QoS0
    pub fn publish(&self, publish: &PublishBuilder) -> Result<(), Error>
    where
        for<'a> &'a W: Write,
    {
        self.writer.publish(publish)
    }
}

impl<W, R> Sender for Connection<W, R>
where
    for<'a> &'a W: Write,
{
    fn publish(&self, publish: &PublishBuilder) -> Result<(), Error> {
        self.writer.publish(publish)
    }

    fn subscribe(&self, subscribe: &SubscribeBuilder) -> Result<(), Error> {
        self.writer.subscribe(subscribe)
    }

    fn unsubscribe<T, S>(&self, unsubscribe: &UnsubscribeBuilder<T>) -> Result<(), Error>
    where
        for<'t> &'t T: IntoIterator<Item = &'t S>,
        S: Borrow<str>,
    {
        self.writer.unsubscribe(unsubscribe)
    }

    fn disconnect(&self) -> Result<(), Error> {
        self.writer.disconnect()
    }
}

impl<W, R> Receiver for Connection<W, R>
where
    R: Read,
{
    fn recv(&mut self) -> Result<Packet<'_>, Error> {
        self.reader.recv()
    }
}

/// Writer half of an MQTT connection
#[derive(Debug)]
pub struct WriterHalf<W> {
    sent_flag: AtomicBool,
    inner: W,
}

impl<W> WriterHalf<W>
where
    for<'a> &'a W: Write,
{
    fn flush(&self) -> Result<(), Error> {
        Write::flush(&mut &self.inner)
            .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "while flusing the writer"))
    }
}
impl<W> Sender for WriterHalf<W>
where
    for<'a> &'a W: Write,
{
    /// Sends a PUBLISH packet with QoS0
    #[instrument(skip_all, fields(payload_bytes = publish.payload.len()))]
    fn publish(&self, publish: &PublishBuilder) -> Result<(), Error> {
        Publish::write_sync(&mut &self.inner, publish)?;

        self.flush()?;

        self.sent_flag.store(true, Ordering::Release);

        debug!("PUBLISH packet sent");

        Ok(())
    }

    /// Subscribes to one or more topic
    #[instrument(skip(self))]
    fn subscribe(&self, subscribe: &SubscribeBuilder) -> Result<(), Error> {
        Subscribe::write_sync(&mut &self.inner, subscribe)?;

        self.flush()?;

        self.sent_flag.store(true, Ordering::Release);

        debug!("SUBSCRIBE packet sent");

        Ok(())
    }

    /// Unsubscribe to one or more topics
    #[instrument(skip_all)]
    fn unsubscribe<T, S>(&self, unsubscribe: &UnsubscribeBuilder<T>) -> Result<(), Error>
    where
        for<'t> &'t T: IntoIterator<Item = &'t S>,
        S: Borrow<str>,
    {
        Unsubscribe::write_sync(&mut &self.inner, unsubscribe)?;

        self.flush()?;

        self.sent_flag.store(true, Ordering::Release);

        debug!("UNSUBSCRIBE packet sent");

        Ok(())
    }

    #[instrument(skip_all)]
    fn disconnect(&self) -> Result<(), Error> {
        Disconnect::write_sync(&mut &self.inner, &())?;

        self.flush()?;

        Ok(())
    }
}

/// Reader half of an MQTT connection.
#[derive(Debug)]
pub struct ReaderHalf<R> {
    inner: Reader<R>,
}

impl<R> Receiver for ReaderHalf<R>
where
    R: Read,
{
    /// Receives from the socket.
    #[instrument(skip(self))]
    fn recv(&mut self) -> Result<Packet<'_>, Error> {
        let buf = self.inner.read()?;

        // TODO actually return something
        let pkt = Packet::consume(buf)?;

        info!(%pkt, "packet received");

        Ok(pkt)
    }
}

#[derive(Debug)]
struct Reader<R> {
    buf: Box<[u8]>,
    pos: usize,
    filled: usize,
    inner: R,
}

impl<R> Reader<R> {
    const MIN: usize = 1 + RemainingLengthSlice::MAX_BYTES;

    pub fn with_capacity(reader: R, capacity: usize) -> Option<Self> {
        if capacity < Self::MIN {
            return None;
        }

        Some(Self {
            // TODO: Box::new_uninit_slice
            buf: vec![0; capacity].into_boxed_slice(),
            pos: 0,
            filled: 0,
            inner: reader,
        })
    }

    /// Reorders the internal buffer
    ///
    /// Should be amortized since we have at max 5 bytes left after a read.
    fn backshift(&mut self) {
        self.buf.copy_within(self.pos..self.filled, 0);
        self.filled -= self.pos;
        self.pos = 0;
    }

    pub fn read(&mut self) -> Result<&[u8], Error>
    where
        R: Read,
    {
        self.backshift();

        let frame = loop {
            let buf = &self.buf[self.pos..self.filled];

            if let Some(len) = FixedHeaderSlice::next_frame(buf)? {
                break len;
            }

            self.fill_head()?;
        };

        self.read_exact(frame)
    }

    /// Fills the buffer for the next fixed header.
    fn fill_head(&mut self) -> Result<(), Error>
    where
        R: Read,
    {
        debug_assert_eq!(self.pos, 0);
        assert!(self.filled < Self::MIN);

        let buf = &mut self.buf[self.filled..Self::MIN];

        let read = self
            .inner
            .read(buf)
            .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "while reading"))?;

        if read == 0 {
            return Err(Error::new(
                ErrorKind::StdIo(std::io::ErrorKind::UnexpectedEof),
                "while reading",
            ));
        }

        self.filled += read;

        Ok(())
    }

    fn read_exact(&mut self, bytes: usize) -> Result<&[u8], Error>
    where
        R: Read,
    {
        if bytes > self.buf.len() {
            return Err(Error::new(ErrorKind::OutOfRange, "payload is too big"));
        }

        if self.filled < bytes {
            let buf = &mut self.buf[self.filled..bytes];

            self.inner
                .read_exact(buf)
                .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "while reading"))?;

            self.filled = bytes;
        }

        let buf = &self.buf[self.pos..bytes];

        self.pos += buf.len();

        Ok(buf)
    }
}
