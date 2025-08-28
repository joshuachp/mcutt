//! Packets related publish operations.

use core::fmt::Display;

use memchr::memchr2;

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};
use crate::v3::packets::common::fixed::ControlPacketType;

use self::builder::PublishBuilder;

use super::common::fixed::builder::FixedHeaderBuilder;
use super::common::fixed::{FixedHeaderSlice, TypeFlags};
use super::common::packet_id::PacketId;
use super::common::string::MqttStr;

pub mod ack;
pub mod builder;
pub mod comp;
pub mod rec;
pub mod rel;

/// A PUBLISH Control Packet is sent from a Client to a Server or from Server to a Client to transport an Application Message.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718037>
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Publish<'a> {
    fixed: &'a FixedHeaderSlice,
    topic: &'a MqttStr,
    packet_id: Option<&'a PacketId>,
    payload: &'a [u8],
}

impl<'a> Publish<'a> {
    /// Returns the topic of the PUBLISH
    pub fn topic(&self) -> &str {
        self.topic.as_str()
    }

    /// Returns the packet identifier if the QoS 1 or 2.
    pub fn pkid(&self) -> Option<&PacketId> {
        self.packet_id
    }

    /// Returns the publish QoS.
    pub fn payload(&self) -> &[u8] {
        self.payload
    }
}

impl<'a> Display for Publish<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} topic({})", ControlPacketType::Publish, self.topic,)?;

        if let Some(pkid) = self.packet_id {
            write!(f, " {pkid}")?;
        }

        write!(f, " payload({} bytes)", self.payload.len())
    }
}

impl<'a> Input<PublishBuilder<'a>> for Publish<'a> {
    type Validated = ();

    fn validate_value(value: &PublishBuilder<'a>) -> Result<Self::Validated, crate::bytes::Error> {
        if contains_wildcard_char(value.topic) {
            return Err(Error::new(
                ErrorKind::Invalid,
                "PUBLISH topic contains wildcard character",
            ));
        }

        Ok(())
    }
}

impl<'a> Parsed for Publish<'a> {
    fn validate(&self) -> Result<(), crate::bytes::Error> {
        if contains_wildcard_char(self.topic.as_str()) {
            return Err(Error::new(
                ErrorKind::Invalid,
                "PUBLISH topic contains wildcard character",
            ));
        }

        Ok(())
    }
}

impl<'a> Encode<PublishBuilder<'a>> for Publish<'a> {
    fn encode_len(value: &PublishBuilder<'a>) -> Result<usize, Error> {
        let mut len = MqttStr::encode_len(value.topic)?;

        if let Some(pkid) = &value.pkid {
            let packet_id = PacketId::encode_len(pkid)?;

            len = len
                .checked_add(packet_id)
                .ok_or(Error::new(ErrorKind::Overflow, "PUBLISH encode len"))?;
        }

        len.checked_add(value.payload.len())
            .ok_or(Error::new(ErrorKind::Overflow, "PUBLISH encode len"))
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &PublishBuilder<'a>) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        Self::validate_value(value)?;

        let rem_len = Self::encode_len(value)?;

        // TODO: Set appropriate flags from builder
        let mut written = FixedHeaderBuilder::with_flags(ControlPacketType::Publish, value.flags)
            .remaining_length(rem_len)?
            .write_sync(writer)?;

        written += MqttStr::write_sync(writer, value.topic)?;

        if let Some(pkid) = &value.pkid {
            written += PacketId::write_sync(writer, pkid)?;
        }

        writer.write_all(value.payload).map_err(|error| {
            Error::new(
                ErrorKind::StdIo(error.kind()),
                "while writing PUBLISH payload",
            )
        })?;

        Ok(written + value.payload.len())
    }
}

impl<'a> Decode<'a> for Publish<'a> {
    type Out = Self;

    fn parse(buff: &'a [u8]) -> Result<(Self::Out, &'a [u8]), Error> {
        let (fixed, rest) = FixedHeaderSlice::parse(buff)?;
        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::Publish);

        let rem_len = fixed.remaining_length().read_len();

        let (rest, rem) = rest
            .split_at_checked(rem_len)
            .ok_or(Error::new(ErrorKind::NotEnoughSpace, "PUBLISH packet"))?;

        let (topic, rest) = MqttStr::parse(rest)?;

        let has_pkid = fixed
            .flags()
            .intersects(TypeFlags::PUBLISH_QOS_1 | TypeFlags::PUBLISH_QOS_2);

        let (packet_id, rest) = if has_pkid {
            PacketId::parse(rest).map(|(v, r)| (Some(v), r))?
        } else {
            (None, rest)
        };

        let this = Self {
            fixed,
            topic,
            packet_id,
            payload: rest,
        };

        this.validate()?;

        Ok((this, rem))
    }
}

fn contains_wildcard_char(topic: &str) -> bool {
    memchr2(b'+', b'#', topic.as_bytes()).is_some()
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use crate::tests::{Hexdump, insta_snapshots};
    use crate::v3::packets::publish::builder::PublishQos;

    use super::*;

    #[rstest]
    #[case("/foo/+")]
    #[case("/foo/#")]
    fn should_match_whildcard(#[case] topic: &str) {
        assert!(contains_wildcard_char(topic));
    }

    #[test]
    fn should_encode_and_decode_publish() {
        let publish = PublishBuilder::new("a/b", &[42]).qos(PublishQos::AtLeastOnce {
            dup: true,
            pkid: NonZero::new(10).unwrap(),
        });

        let mut buf = Vec::new();

        let len = Publish::write_sync(&mut buf, &publish).unwrap();

        assert_eq!(buf.len(), len);

        let res = Publish::consume(&buf).unwrap();

        assert_eq!(
            res.fixed.flags(),
            TypeFlags::PUBLISH_QOS_1 | TypeFlags::PUBLISH_DUP
        );
        assert_eq!(res.packet_id.map(|p| p.read()), publish.pkid);
        assert_eq!(res.topic.as_str(), publish.topic);
        assert_eq!(res.payload, publish.payload);

        insta_snapshots!({
            insta::assert_snapshot!(Hexdump(buf));
        });
    }
}
