//! Iterator over the topic filters in a SUBSCRIBE packet.

use crate::bytes::{Decode, Error};

use super::topic::TopicFilter;

/// Cursor to iterator over the [`SubscribeTopic`] encoded in a buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RawSubscribeCursor<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) pos: usize,
}

impl<'a> RawSubscribeCursor<'a> {
    fn read(&mut self) -> Option<Result<TopicFilter<'a>, Error>> {
        debug_assert!(self.pos <= self.bytes.len());

        let bytes = self
            .bytes
            .get(self.pos..)
            .filter(|bytes| !bytes.is_empty())?;

        let (topic, rest) = match TopicFilter::parse(bytes) {
            Ok(filter) => filter,
            Err(err) => return Some(Err(err)),
        };

        debug_assert!(rest.len() < bytes.len());
        self.pos += bytes.len().saturating_sub(rest.len());

        Some(Ok(topic))
    }
}

impl<'a> Iterator for RawSubscribeCursor<'a> {
    type Item = Result<TopicFilter<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.read()
    }
}

/// Cursor to iterator over the [`TopicFilter`] encoded in a buffer.
///
/// Panics if the SUBSCRIBE packet is invalid.
#[derive(Debug, Clone, Copy)]
pub(super) struct SubscriberCursor<'a>(pub(super) RawSubscribeCursor<'a>);

impl<'a> Iterator for SubscriberCursor<'a> {
    type Item = TopicFilter<'a>;

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

    use crate::v3::packets::subscribe::topic::RequestedQos;

    #[test]
    fn iter_subscribe_cursor() {
        let payload: [u8; _] = [
            // Topic len 3
            0b0000_0000,
            0b0000_0011,
            b'a',
            b'/',
            b'b',
            // Qos(1)
            0b0000_0001,
            // Topic len 3
            0b0000_0000,
            0b0000_0011,
            b'c',
            b'/',
            b'd',
            // Qos(2)
            0b0000_0010,
        ];

        let exp = [
            ("a/b", RequestedQos::AtLeastOnce),
            ("c/d", RequestedQos::ExactlyOnce),
        ];

        let mut cursor = RawSubscribeCursor {
            bytes: &payload,
            pos: 0,
        };

        for (exp_topic, exp_qos) in exp {
            let filter = cursor.read().unwrap().unwrap();

            assert_eq!(filter.topic().as_str(), exp_topic);
            assert_eq!(*filter.qos(), exp_qos);
        }

        assert!(cursor.read().is_none());
    }
}
