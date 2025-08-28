//! Iterator over the topic filters in a SUBSCRIBE packet.

use crate::bytes::{Decode, Error};
use crate::v3::packets::common::string::MqttStr;

/// Cursor to iterator over the [`UnsubscribeTopic`] encoded in a buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RawUnsubscribeCursor<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) pos: usize,
}

impl<'a> RawUnsubscribeCursor<'a> {
    fn read(&mut self) -> Option<Result<&'a MqttStr, Error>> {
        debug_assert!(self.pos <= self.bytes.len());

        let bytes = self
            .bytes
            .get(self.pos..)
            .filter(|bytes| !bytes.is_empty())?;

        let (topic, rest) = match MqttStr::parse(bytes) {
            Ok(filter) => filter,
            Err(err) => return Some(Err(err)),
        };

        debug_assert!(rest.len() < bytes.len());
        self.pos += bytes.len().saturating_sub(rest.len());

        Some(Ok(topic))
    }
}

impl<'a> Iterator for RawUnsubscribeCursor<'a> {
    type Item = Result<&'a MqttStr, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.read()
    }
}

/// Cursor to iterator over the [`TopicFilter`] encoded in a buffer.
///
/// Panics if the SUBSCRIBE packet is invalid.
#[derive(Debug, Clone, Copy)]
pub(super) struct UnsubscribeCursor<'a>(pub(super) RawUnsubscribeCursor<'a>);

impl<'a> Iterator for UnsubscribeCursor<'a> {
    type Item = &'a MqttStr;

    fn next(&mut self) -> Option<Self::Item> {
        self.0
            .next()
            .map(|filter| filter.expect("must be a valid SUBSCRIBE packet"))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn iter_subscribe_cursor() {
        let payload: [u8; _] = [
            // Topic len 3
            0b0000_0000,
            0b0000_0011,
            b'a',
            b'/',
            b'b',
            // Topic len 3
            0b0000_0000,
            0b0000_0011,
            b'c',
            b'/',
            b'd',
        ];

        let exp_topics = ["a/b", "c/d"];

        let mut cursor = RawUnsubscribeCursor {
            bytes: &payload,
            pos: 0,
        };

        for exp in exp_topics {
            let filter = cursor.read().unwrap().unwrap();

            assert_eq!(filter.as_str(), exp);
        }

        assert!(cursor.read().is_none());
    }
}
