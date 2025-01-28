//! Sync MQTT v3 client

use std::{
    io::{self, BufReader, BufWriter, IoSlice, IoSliceMut, Read, Write},
    rc::Rc,
    sync::{Arc, Mutex},
};

/// Client that wraps a transport to send and receive MQTT packets.
#[derive(Debug)]
pub struct Client<R, W>
where
    R: std::io::Read,
    W: std::io::Write,
{
    inner: Arc<SharedClient<R, W>>,
}

impl<R, W> Client<R, W>
where
    R: std::io::Read,
    W: std::io::Write,
{
    fn with_both(reader: R, writer: W) -> Self {
        let reader = Mutex::new(BufReader::new(reader));
        let writer = Mutex::new(BufWriter::new(writer));

        Self {
            inner: Arc::new(SharedClient { reader, writer }),
        }
    }
}

impl<S> Client<SharedSocket<S>, SharedSocket<S>>
where
    for<'a> &'a S: std::io::Read,
    for<'a> &'a S: std::io::Write,
{
    fn with_shared(socket: S) -> Self {
        let reader = Rc::new(socket);
        let writer = Rc::clone(&reader);

        Self::with_both(SharedSocket(reader), SharedSocket(writer))
    }
}

#[derive(Debug)]
struct SharedClient<R, W>
where
    W: std::io::Write,
{
    reader: Mutex<BufReader<R>>,
    writer: Mutex<BufWriter<W>>,
}

#[derive(Debug)]
struct SharedSocket<S>(Rc<S>);

impl<S> Read for SharedSocket<S>
where
    for<'a> &'a S: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.as_ref().read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        self.0.as_ref().read_vectored(bufs)
    }
}

impl<S> Write for SharedSocket<S>
where
    for<'a> &'a S: Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.as_ref().write(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.0.as_ref().write_vectored(bufs)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
