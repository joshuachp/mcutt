//! Sync client implementation using the standard library [`TcpStream`].

use std::io::{BufWriter, Read, Write};
use std::num::NonZero;

use tracing::{debug, info, instrument, trace};

use crate::bytes::{Decode, Encode, Error, ErrorKind};
use crate::v3::packets::common::fixed::FixedHeaderSlice;
use crate::v3::packets::common::fixed::remaining_length::RemainingLengthSlice;
use crate::v3::packets::connect::Connect;
use crate::v3::packets::connect::ack::ConnAck;
use crate::v3::packets::connect::builder::ConnectBuilder;
use crate::v3::packets::packet::Packet;
use crate::v3::packets::publish::Publish;
use crate::v3::packets::publish::builder::PublishBuilder;
use crate::v3::packets::subscribe::Subscribe;
use crate::v3::packets::subscribe::builder::{SubscribeBuilder, SubscribeFilter};
use crate::v3::packets::subscribe::topic::RequestedQos;

/// The MQTT connection, for both reading  and writing.
#[derive(Debug)]
pub struct Connection<W, R>
where
    W: Write,
{
    writer: BufWriter<W>,
    reader: Reader<R>,
}

impl<W, R> Connection<W, R>
where
    W: Write,
    R: Read,
{
    /// Creates a new connection from a socket.
    #[must_use]
    pub fn new(writer: W, reader: R) -> Self {
        Self {
            writer: BufWriter::new(writer),
            reader: Reader::with_capacity(reader, 8 * 1024).expect("size is greater than 8K"),
        }
    }

    /// Sends a [`Connect`] packet and waits for the Server's [`ConnAck`].
    #[instrument(skip(self, connect))]
    pub fn connect(&mut self, connect: ConnectBuilder) -> Result<ConnAck, Error>
    where
        W: Write,
        R: Read,
    {
        trace!("sending CONNECT packet");

        let written = Connect::write_sync(&mut self.writer, &connect)?;

        self.writer.flush().map_err(|error| {
            Error::new(ErrorKind::StdIo(error.kind()), "while flusing the writer")
        })?;

        trace!(written);

        trace!("waiting for CONNACK packet");

        let read = self.reader.read()?;

        ConnAck::consume(read).map(|connack| {
            trace!("CONNACK received");

            *connack
        })
    }

    /// Sends a PUBLISH packet with QoS0
    #[instrument(skip(self, payload), fields(payload_bytes = payload.len()))]
    pub fn publish(&mut self, topic: &str, payload: &[u8]) -> Result<(), Error> {
        Publish::write_sync(&mut self.writer, &PublishBuilder::new(topic, payload))?;

        debug!("PUBLISH packet sent");

        self.writer.flush().map_err(|error| {
            Error::new(ErrorKind::StdIo(error.kind()), "while flusing the writer")
        })?;

        Ok(())
    }

    /// Subscribes to a topic
    #[instrument(skip(self))]
    pub fn subscribe(&mut self, topic: &str) -> Result<(), Error> {
        Subscribe::write_sync(
            &mut self.writer,
            &SubscribeBuilder::with_topic(
                NonZero::new(1).unwrap(),
                &SubscribeFilter {
                    topic,
                    qos: RequestedQos::AtMostOnce,
                },
            ),
        )?;

        self.writer.flush().map_err(|error| {
            Error::new(ErrorKind::StdIo(error.kind()), "while flusing the writer")
        })?;

        Ok(())
    }

    /// Receives from the socket.
    // TODO: This does nothing
    #[instrument(skip(self))]
    pub fn recv(&mut self) -> Result<Packet<'_>, Error> {
        let buf = self.reader.read()?;

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
