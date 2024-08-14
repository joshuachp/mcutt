use std::{
    io::{self, BufReader, BufWriter, Write},
    net::TcpStream,
};

use crate::v3::{connect::Connect, Encode, EncodeError, Writer};

type Error = EncodeError<io::Error>;

pub struct TcpClient<'a> {
    connection: BufWriter<&'a TcpStream>,
}

impl<'a> TcpClient<'a> {
    pub fn new(connection: BufWriter<&'a TcpStream>) -> Self {
        Self { connection }
    }

    pub fn connect(&mut self, connect: Connect) -> Result<(), Error> {
        connect.write(self)?;

        self.flush().map_err(EncodeError::Write)?;

        Ok(())
    }
}

impl<'a> Writer for TcpClient<'a> {
    type Err = io::Error;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Err> {
        self.connection.write_all(buf)
    }

    fn write_u8(&mut self, value: u8) -> Result<(), Self::Err> {
        self.connection.write_all(&[value])
    }

    fn flush(&mut self) -> Result<(), Self::Err> {
        self.connection.flush()
    }
}

pub struct TcpConnection {
    pub connection: BufReader<TcpStream>,
}

impl TcpConnection {
    pub fn new(connection: BufReader<TcpStream>) -> Self {
        Self { connection }
    }
}
