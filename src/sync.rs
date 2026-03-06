//! Sync client implementation using the standard library [`TcpStream`].

use std::borrow::Borrow;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tracing::{debug, info, instrument, trace, warn};
use zerocopy::IntoBytes;

use crate::bytes::{Decode, Encode, Error, ErrorKind};
use crate::v3::packets::common::fixed::FixedHeaderSlice;
use crate::v3::packets::common::fixed::remaining_length::RemainingLengthSlice;
use crate::v3::packets::connect::Connect;
use crate::v3::packets::connect::ack::ConnAck;
use crate::v3::packets::connect::builder::ConnectBuilder;
use crate::v3::packets::disconnect::Disconnect;
use crate::v3::packets::packet::Packet;
use crate::v3::packets::ping::PingReq;
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
pub struct Connection<S> {
    writer: Arc<WriterHalf<S>>,
    reader: ReaderHalf<S>,
}

impl Connection<mio::net::TcpStream> {
    // TODO: use non blocking connect
    fn connect_socket(
        addr: impl ToSocketAddrs,
        timeout: Duration,
    ) -> std::io::Result<Option<(mio::net::TcpStream, mio::net::TcpStream)>> {
        addr.to_socket_addrs().and_then(|addrs| {
            addrs
                .map(|addr| {
                    let writer = TcpStream::connect_timeout(&addr, timeout)?;

                    writer.set_nodelay(true)?;
                    writer.set_nonblocking(true)?;
                    writer.set_read_timeout(Some(timeout))?;
                    writer.set_write_timeout(Some(timeout))?;

                    let reader = writer.try_clone().map(mio::net::TcpStream::from_std)?;

                    Ok((mio::net::TcpStream::from_std(writer), reader))
                })
                .find(Result::is_ok)
                .transpose()
        })
    }

    /// Creates a new connection from a socket.
    #[must_use]
    pub fn create(addr: impl ToSocketAddrs, timeout: Duration) -> Result<Self, Error> {
        let (writer, reader) = Self::connect_socket(addr, timeout)
            .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "while opening socket"))
            .and_then(|opt| opt.ok_or(Error::new(ErrorKind::Invalid, "socket addrs")))?;

        let writer = Arc::new(WriterHalf {
            sent_flag: AtomicBool::new(false),
            inner: writer,
        });

        let mut reader = ReaderHalf::create(reader, Arc::clone(&writer), timeout)?;

        reader.register()?;

        Ok(Self { writer, reader })
    }
}

impl<S> Connection<S> {
    /// Sends a [`Connect`] packet and waits for the Server's [`ConnAck`].
    #[instrument(skip(self, connect))]
    pub fn connect(&mut self, connect: ConnectBuilder) -> Result<ConnAck, Error>
    where
        for<'a> &'a S: Write,
        S: Read,
    {
        trace!("sending CONNECT packet");

        let written = Connect::write_sync(&mut &self.writer.inner, &connect)?;

        self.writer.flush()?;

        self.writer.sent_flag.store(true, Ordering::Release);

        trace!(written);

        trace!("waiting for CONNACK packet");

        let read = self.reader.poll_event()?;

        ConnAck::consume(read).map(|connack| {
            trace!("CONNACK received");

            *connack
        })
    }

    /// Sends a PUBLISH packet with QoS0
    pub fn publish(&self, publish: &PublishBuilder) -> Result<(), Error>
    where
        for<'a> &'a S: Write,
    {
        self.writer.publish(publish)
    }
}

impl<S> Sender for Connection<S>
where
    for<'a> &'a S: Write,
{
    fn publish(&self, publish: &PublishBuilder) -> Result<(), Error> {
        self.writer.publish(publish)
    }

    fn subscribe(&self, subscribe: &SubscribeBuilder) -> Result<(), Error> {
        self.writer.subscribe(subscribe)
    }

    fn unsubscribe<T, U>(&self, unsubscribe: &UnsubscribeBuilder<T>) -> Result<(), Error>
    where
        for<'t> &'t T: IntoIterator<Item = &'t U>,
        U: Borrow<str>,
    {
        self.writer.unsubscribe(unsubscribe)
    }

    fn disconnect(&self) -> Result<(), Error> {
        self.writer.disconnect()
    }
}

impl<S> Receiver for Connection<S>
where
    S: Read,
    for<'a> &'a S: Write,
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
    #[instrument(skip_all)]
    fn flush(&self) -> Result<(), Error> {
        Write::flush(&mut &self.inner)
            .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "while flusing the writer"))
    }

    /// Sends a PING packet
    #[instrument(skip_all)]
    fn handle_ping(&self) -> Result<(), Error> {
        if self
            .sent_flag
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            trace!("control packet already sent");

            return Ok(());
        }

        trace!("sending ping");

        PingReq::write_sync(&mut &self.inner, &())?;

        self.flush()?;

        self.sent_flag.store(false, Ordering::Release);

        debug!("PING packet sent");

        Ok(())
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
    poll: mio::Poll,
    events: mio::Events,
    timer_fd: rustix::fd::OwnedFd,
    inner: Reader<R>,
    writer: Arc<WriterHalf<R>>,
}

impl<R> ReaderHalf<R> {
    const READER: mio::Token = mio::Token(0);
    const TIMER: mio::Token = mio::Token(1);

    fn create(
        reader: R,
        writer: Arc<WriterHalf<R>>,
        timeout: Duration,
    ) -> Result<ReaderHalf<R>, Error> {
        let inner = Reader::with_capacity(reader, 8 * 1024).expect("size is greater than 8K");
        let timer_fd = Self::create_timer_fd(timeout)?;
        let poll = mio::Poll::new()
            .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "while creating polll"))?;

        Ok(ReaderHalf {
            inner,
            timer_fd,
            poll,
            events: mio::Events::with_capacity(128),
            writer,
        })
    }

    #[instrument(skip_all)]
    fn create_timer_fd(interval: Duration) -> Result<OwnedFd, Error> {
        let timer_fd = rustix::time::timerfd_create(
            rustix::time::TimerfdClockId::Monotonic,
            rustix::time::TimerfdFlags::empty(),
        )
        .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "while creating timer_fd"))?;

        let sec = i64::try_from(interval.as_secs())
            .map_err(|_| Error::new(ErrorKind::OutOfRange, "interval seconds"))?;

        rustix::time::timerfd_settime(
            &timer_fd,
            rustix::time::TimerfdTimerFlags::ABSTIME,
            &rustix::time::Itimerspec {
                it_interval: rustix::time::Timespec {
                    tv_sec: sec,
                    tv_nsec: 0,
                },
                it_value: rustix::time::Timespec {
                    tv_sec: sec,
                    tv_nsec: 0,
                },
            },
        )
        .map_err(|error| {
            Error::new(
                ErrorKind::StdIo(error.kind()),
                "while setting interval time",
            )
        })?;

        Ok(timer_fd)
    }

    #[instrument(skip_all)]
    fn register(&mut self) -> Result<(), Error>
    where
        R: mio::event::Source,
    {
        self.poll
            .registry()
            .register(&mut self.inner.inner, Self::READER, mio::Interest::READABLE)
            .map_err(|error| {
                Error::new(ErrorKind::StdIo(error.kind()), "while registering socket")
            })?;

        self.poll
            .registry()
            .register(
                &mut mio::unix::SourceFd(&self.timer_fd.as_raw_fd()),
                Self::TIMER,
                mio::Interest::READABLE,
            )
            .map_err(|error| {
                Error::new(ErrorKind::StdIo(error.kind()), "while registering timer")
            })?;

        Ok(())
    }

    fn poll_event(&mut self) -> Result<&[u8], Error>
    where
        R: Read,
        for<'a> &'a R: Write,
    {
        'blk: loop {
            self.poll
                .poll(&mut self.events, Some(Duration::from_millis(100)))
                .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "while polling"))?;

            for event in self.events.iter() {
                match event.token() {
                    Self::READER => match self.inner.read() {
                        Ok(_buf) => break 'blk,
                        Err(err) if would_block(&err) => {}
                        Err(err) => return Err(err),
                    },
                    Self::TIMER => {
                        trace!("reading timer");
                        let mut buf = 0i64;
                        rustix::io::read(&self.timer_fd, buf.as_mut_bytes())
                            .ok()
                            .filter(|s| *s == size_of::<i64>())
                            // TODO: real error
                            .ok_or(Error::new(ErrorKind::Invalid, "read"))?;

                        debug!("timer checking for ping");

                        self.writer.handle_ping()?;
                    }
                    _ => unreachable!(),
                }
            }
        }

        Ok(self.inner.buff())
    }
}

impl<R> Receiver for ReaderHalf<R>
where
    R: Read,
    for<'a> &'a R: Write,
{
    /// Receives from the socket.
    #[instrument(skip(self))]
    fn recv(&mut self) -> Result<Packet<'_>, Error> {
        self.inner.consume();

        let pkt = self.poll_event().and_then(Packet::consume)?;

        info!(%pkt, "packet received");

        Ok(pkt)
    }
}

fn would_block(err: &Error) -> bool {
    err.kind() == ErrorKind::StdIo(std::io::ErrorKind::WouldBlock)
}

#[derive(Debug)]
struct Reader<R> {
    buf: Box<[u8]>,
    pos: usize,
    frame: Option<usize>,
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
            frame: None,
            filled: 0,
            inner: reader,
        })
    }

    /// Reorders the internal buffer
    ///
    /// Should be amortized since we have at max 5 bytes left after a read.
    #[instrument(skip(self))]
    fn backshift(&mut self) {
        self.buf.copy_within(self.pos..self.filled, 0);
        self.filled -= self.pos;
        self.pos = 0;
    }

    #[instrument(skip(self))]
    pub fn read(&mut self) -> Result<&[u8], Error>
    where
        R: Read,
    {
        let frame = match self.frame {
            None => self.next_frame()?,
            Some(frame) => frame,
        };

        trace!(frame);

        self.read_frame(frame)
    }

    fn next_frame(&mut self) -> Result<usize, Error>
    where
        R: Read,
    {
        debug_assert!((self.filled - self.pos) < Self::MIN);
        // We assume the buffer is fairly small since we read at most less than Self::MIN bytes
        self.backshift();

        loop {
            let buf = self.buff();

            if let Some(frame) = FixedHeaderSlice::next_frame(buf)? {
                self.frame = Some(frame);

                return Ok(frame);
            }

            self.fill_head()?;
        }
    }

    /// Fills the buffer for the next fixed header.
    #[instrument(skip(self))]
    fn fill_head(&mut self) -> Result<(), Error>
    where
        R: Read,
    {
        let avail = self.filled - self.pos;
        let to_read = Self::MIN - avail;
        let end = self.filled + to_read;

        let buf = self
            .buf
            .get_mut(self.filled..end)
            .ok_or(Error::new(ErrorKind::NotEnoughSpace, "for header"))?;

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

    #[instrument(skip(self))]
    fn read_frame(&mut self, bytes: usize) -> Result<&[u8], Error>
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

        let buf = self.buff();

        Ok(buf)
    }

    #[instrument(skip(self))]
    fn consume(&mut self) {
        if let Some(frame) = self.frame.take() {
            self.pos = std::cmp::min(self.pos + frame, self.filled);
        }
    }

    #[instrument(skip(self))]
    fn buff(&self) -> &[u8] {
        &self.buf[self.pos..self.filled]
    }
}
