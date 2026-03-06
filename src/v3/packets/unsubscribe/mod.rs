//! Handle unsubscribing to topics to the UNSUBSCRIBE packet.

use core::fmt::Display;
use std::borrow::Borrow;

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};
use crate::v3::packets::common::fixed::{ControlPacketType, FixedHeaderSlice};

use self::builder::UnsubscribeBuilder;
use self::iter::{RawUnsubscribeCursor, UnsubscribeCursor};

use super::common::fixed::builder::FixedHeaderBuilder;
use super::common::packet_id::PacketId;
use super::common::string::MqttStr;
use super::common::topic::has_valid_wildcard;

pub mod ack;
pub mod builder;
pub mod iter;

/// An UNSUBSCRIBE Packet is sent by the Client to the Server, to unsubscribe from topics.
///
/// The payload for the UNSUBSCRIBE Packet contains the list of Topic Filters that the Client wishes
/// to unsubscribe from. The Topic Filters in an UNSUBSCRIBE packet MUST be UTF-8 encoded strings as
/// packed contiguously. The Payload of an UNSUBSCRIBE packet MUST contain at least one Topic
/// Filter. An UNSUBSCRIBE packet with no payload is a protocol violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsubscribe<'a> {
    pkid: &'a PacketId,
    filters: &'a [u8],
}

impl<'a> Unsubscribe<'a> {
    /// Returns the packet identifier of the UNSUBSCRIBE.
    pub fn pkid(&self) -> &PacketId {
        self.pkid
    }

    /// Return an iterator over the filters of the UNSUBSCRIBE.
    pub fn filters(&self) -> impl Iterator<Item = &MqttStr> {
        UnsubscribeCursor(RawUnsubscribeCursor {
            pos: 0,
            bytes: self.filters,
        })
    }
}

impl<'a> Display for Unsubscribe<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} pkid({}) [",
            ControlPacketType::Unsubscribe,
            self.pkid
        )?;

        let mut iter = self.filters();

        if let Some(i) = iter.next() {
            write!(f, "{i}")?;

            for i in iter {
                write!(f, ", {i}")?;
            }
        }

        write!(f, "]")
    }
}

impl<'a, T, S> Input<UnsubscribeBuilder<T>> for Unsubscribe<'a>
where
    for<'t> &'t T: IntoIterator<Item = &'t S>,
    S: Borrow<str>,
{
    type Validated = ();

    fn validate_value(
        value: &UnsubscribeBuilder<T>,
    ) -> Result<Self::Validated, crate::bytes::Error> {
        for filter in &value.filters {
            if !has_valid_wildcard(filter.borrow()) {
                return Err(Error::new(ErrorKind::Invalid, "UNSUBSCRIBE topic filter"));
            }
        }

        Ok(())
    }
}

impl<'a> Parsed for Unsubscribe<'a> {
    fn validate(&self) -> Result<(), Error> {
        let cursor = RawUnsubscribeCursor {
            bytes: self.filters,
            pos: 0,
        };

        for topic in cursor {
            let topic = topic?;
            if !has_valid_wildcard(topic.as_str()) {
                return Err(Error::new(ErrorKind::Invalid, "UNSUBSCRIBE topic filter"));
            }
        }

        Ok(())
    }
}

impl<'a, T, S> Encode<UnsubscribeBuilder<T>> for Unsubscribe<'a>
where
    for<'t> &'t T: IntoIterator<Item = &'t S>,
    S: Borrow<str>,
{
    fn encode_len(value: &UnsubscribeBuilder<T>) -> Result<usize, Error> {
        let len = PacketId::encode_len(&value.pkid)?;

        value.filters.into_iter().try_fold(len, |len, filter| {
            MqttStr::encode_len(filter.borrow()).and_then(|topic| {
                topic
                    .checked_add(len)
                    .ok_or(Error::new(ErrorKind::Overflow, "UNSUBSCRIBE packet len"))
            })
        })
    }

    fn write_sync<W>(writer: &mut W, value: &UnsubscribeBuilder<T>) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        Self::validate_value(value)?;

        let rem_len = Self::encode_len(value)?;

        let mut written = FixedHeaderBuilder::new(ControlPacketType::Unsubscribe)
            .remaining_length(rem_len)?
            .write_sync(writer)?;

        written += PacketId::write_sync(writer, &value.pkid)?;

        value.filters.into_iter().try_fold(written, |acc, filter| {
            MqttStr::write_sync(writer, filter.borrow()).map(|written| acc + written)
        })
    }
}

impl<'a> Decode<'a> for Unsubscribe<'a> {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), Error> {
        let (fixed, rest) = FixedHeaderSlice::parse(buf)?;
        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::Unsubscribe);

        let rem_len = fixed.remaining_length().read_len();

        let (packet, rest) = rest
            .split_at_checked(rem_len)
            .ok_or(Error::new(ErrorKind::NotEnoughSpace, "SUBSCRIBE packet"))?;

        let (pkid, packet) = PacketId::parse(packet)?;

        let this = Self {
            pkid,
            filters: packet,
        };

        this.validate()?;

        Ok((this, rest))
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use pretty_assertions::assert_eq;

    use crate::tests::{Hexdump, insta_snapshots};

    use super::*;

    #[test]
    fn unsubscribe_roundtrip() {
        let packet = UnsubscribeBuilder::new(NonZero::new(10u16).unwrap(), ["a/b", "c/d"]);

        let mut buf = Vec::new();

        let written = Unsubscribe::write_sync(&mut buf, &packet).unwrap();
        assert_eq!(written, buf.len());

        let res = Unsubscribe::consume(&buf).unwrap();

        assert_eq!(res.pkid.read(), packet.pkid);

        let eq_filters = res.filters().eq(packet.filters.iter().copied());
        assert!(eq_filters);

        insta_snapshots!({
            insta::assert_snapshot!(Hexdump(buf));
        });
    }
}
