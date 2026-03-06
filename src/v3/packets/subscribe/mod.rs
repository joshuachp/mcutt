//! Handle subscribing to topics through the SUBSCRIBE packet.

use core::fmt::Display;

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};
use crate::v3::packets::common::fixed::ControlPacketType;

use self::builder::SubscribeBuilder;
use self::iter::{RawSubscribeCursor, SubscriberCursor};
use self::topic::TopicFilter;

use super::common::fixed::FixedHeaderSlice;
use super::common::fixed::builder::FixedHeaderBuilder;
use super::common::packet_id::PacketId;

pub mod ack;
pub mod builder;
pub mod iter;
pub mod topic;

/// The SUBSCRIBE Packet is sent from the Client to the Server to create one or more Subscriptions.
///
/// Each Subscription registers a Client’s interest in one or more Topics. The Server sends PUBLISH
/// Packets to the Client in order to forward Application Messages that were published to Topics
/// that match these Subscriptions. The SUBSCRIBE Packet also specifies (for each Subscription) the
/// maximum QoS with which the Server can send Application Messages to the Client.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718063>
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscribe<'a> {
    pkid: &'a PacketId,
    filters: &'a [u8],
}

impl<'a> Subscribe<'a> {
    /// Return the packet identifier of the subscribe.
    pub fn pkid(&self) -> &PacketId {
        self.pkid
    }

    /// Return an iterator over the filters of the subscribe.
    pub fn filters(&self) -> impl Iterator<Item = TopicFilter<'_>> {
        SubscriberCursor(RawSubscribeCursor {
            bytes: self.filters,
            pos: 0,
        })
    }
}

impl<'a> Display for Subscribe<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} pkid({}) [", ControlPacketType::Subscribe, self.pkid)?;

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

impl<'a> Input<SubscribeBuilder<'a>> for Subscribe<'a> {
    type Validated = ();

    fn validate_value(value: &SubscribeBuilder<'a>) -> Result<Self::Validated, Error> {
        value
            .filters
            .iter()
            .try_for_each(TopicFilter::validate_value)?;

        Ok(())
    }
}
impl<'a> Parsed for Subscribe<'a> {
    fn validate(&self) -> Result<(), Error> {
        RawSubscribeCursor {
            bytes: self.filters,
            pos: 0,
        }
        .try_for_each(|v| v.map(drop))
    }
}

impl<'a> Encode<SubscribeBuilder<'a>> for Subscribe<'a> {
    fn encode_len(value: &SubscribeBuilder<'a>) -> Result<usize, Error> {
        let len = PacketId::encode_len(&value.pkid)?;

        value.filters.iter().try_fold(len, |len, filter| {
            TopicFilter::encode_len(filter).and_then(|topic| {
                topic
                    .checked_add(len)
                    .ok_or(Error::new(ErrorKind::Overflow, "SUBSCRIBE packet len"))
            })
        })
    }

    fn write_sync<W>(writer: &mut W, value: &SubscribeBuilder<'a>) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        Self::validate_value(value)?;

        let rem_len = Self::encode_len(value)?;

        let mut written = FixedHeaderBuilder::new(ControlPacketType::Subscribe)
            .remaining_length(rem_len)?
            .write_sync(writer)?;

        written += PacketId::write_sync(writer, &value.pkid)?;

        value.filters.iter().try_fold(written, |acc, filter| {
            TopicFilter::write_sync(writer, filter).map(|written| acc + written)
        })
    }
}

impl<'a> Decode<'a> for Subscribe<'a> {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), Error> {
        let (fixed, rest) = FixedHeaderSlice::parse(buf)?;
        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::Subscribe);

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

    use crate::bytes::{Decode, Encode};
    use crate::tests::{Hexdump, insta_snapshots};
    use crate::v3::packets::subscribe::Subscribe;
    use crate::v3::packets::subscribe::builder::{SubscribeBuilder, SubscribeFilter};

    use super::topic::RequestedQos;

    #[test]
    fn subscribe_roundtrip() {
        let packet = SubscribeBuilder::new(
            NonZero::try_from(10u16).unwrap(),
            &[
                SubscribeFilter {
                    topic: "a/b",
                    qos: RequestedQos::AtLeastOnce,
                },
                SubscribeFilter {
                    topic: "c/d",
                    qos: RequestedQos::ExactlyOnce,
                },
            ],
        );

        let mut buf = Vec::new();

        let written = Subscribe::write_sync(&mut buf, &packet).unwrap();
        assert_eq!(written, buf.len());

        let subscribe = Subscribe::consume(&buf).unwrap();

        assert_eq!(subscribe.pkid.read(), packet.pkid);

        let eq_filters = subscribe.filters().eq(packet.filters.iter().copied());
        assert!(eq_filters);

        insta_snapshots!({
            insta::assert_snapshot!(Hexdump(buf));
        });
    }
}
