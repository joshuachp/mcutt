use std::io::{self, BufReader, Read};

pub(crate) struct Reader<S> {
    reader: BufReader<S>,
}

impl<S> Reader<S> {
    pub(crate) fn new(reader: BufReader<S>) -> Self {
        Self { reader }
    }
}

impl<S> Reader<S>
where
    S: Read,
{
    pub(crate) fn try_recv(&mut self) -> io::Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::v3::{connect::{Connect, KeepAlive}, header::StrRef};

    use super::*;

    #[test]
    fn should_recv_packet() {
        let client_id = StrRef::try_from("client-id").unwrap();
        let keep_alive = KeepAlive::new()
        let payload = Connect::new(client_id, keep_alive);

        let reader = Reader::new();
    }
}
