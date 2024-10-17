//! Shareable socket

use std::{
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    sync::Mutex,
};

use crate::v3::{Decode, DecodeError, Writer};

mod reader;

/// The underling connection for the struct
#[derive(Debug)]
pub struct SyncSocket<S>
where
    S: Read + Write,
{
    writer: Mutex<BufWriter<S>>,
}

impl<S> SyncSocket<S>
where
    S: Read + Write,
{
    fn try_recv<T>(&mut self) -> Result<T, DecodeError> {
        todo!()
    }
}

impl<T> Write for &'_ SyncSocket<T>
where
    T: Read + Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "write socket mutex was poisoned"))?
            .write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "write socket mutex was poisoned"))?
            .flush()
    }
}

impl<T> Writer for &'_ SyncSocket<T>
where
    T: Read + Write,
{
    type Err = io::Error;

    fn write_slice(&mut self, buf: &[u8]) -> Result<usize, Self::Err> {
        self.write_all(buf)?;

        Ok(buf.len())
    }
}
