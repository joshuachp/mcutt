use core::ops::Deref;
use std::{
    io::BufReader,
    net::TcpStream,
    sync::{Arc, Mutex},
};

struct Client<S> {
    inner: Arc<SharecClient<S>>,
}

impl<S> Client<S> {
    fn connect() {}

    fn send() {}
}

impl Client<TcpStream> {
    fn new(socket: TcpStream) -> Self {
        Self {
            inner: Arc::new(SharecClient {
                reader: Mutex::new(socket),
                writer: Mutex::new(socket),
            }),
        }
    }
}

impl<S> Clone for Client<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<S> Deref for Client<S> {
    type Target = SharecClient<S>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

struct SharecClient<S> {
    reader: Mutex<BufReader<S>>,
    writer: Mutex<BufReader<S>>,
}
