//! PUBACK packet for PUBLISH with [`QoS::AtLeastOnce`](crate::v3::packets::Qos::AtLeastOnce).

use core::fmt::Display;
use std::num::NonZero;

use crate::bytes::{Decode, Encode, Error, Input, Parsed};
use crate::v3::packets::common::fixed::builder::FixedHeaderBuilder;
use crate::v3::packets::common::fixed::{ControlPacketType, FixedHeaderArray};
use crate::v3::packets::common::packet_id::PacketId;

/// A PUBACK Packet is the response to a PUBLISH Packet with QoS level 1.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718043>
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PubAck<'a> {
    pkid: &'a PacketId,
}

impl<'a> PubAck<'a> {
    /// Returns the packet identifier.
    #[must_use]
    pub fn pkid(&self) -> &'a PacketId {
        self.pkid
    }
}

impl<'a> Display for PubAck<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} pkid({})", ControlPacketType::PubAck, self.pkid)
    }
}

impl<'p> Input<NonZero<u16>> for PubAck<'p> {
    type Validated = ();

    fn validate_value(_value: &NonZero<u16>) -> Result<Self::Validated, crate::bytes::Error> {
        Ok(())
    }
}

impl<'a> Parsed for PubAck<'a> {
    fn validate(&self) -> Result<(), crate::bytes::Error> {
        Ok(())
    }
}

impl<'a> Encode<NonZero<u16>> for PubAck<'a> {
    fn encode_len(value: &NonZero<u16>) -> Result<usize, Error> {
        PacketId::encode_len(value)
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &NonZero<u16>) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        let mut written = FixedHeaderBuilder::new(ControlPacketType::PubAck)
            .remaining_length(Self::encode_len(value)?)?
            .write_sync(writer)?;

        written += PacketId::write_sync(writer, value)?;

        Ok(written)
    }
}

impl<'a> Decode<'a> for PubAck<'a> {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), crate::bytes::Error> {
        let (fixed, rest) = FixedHeaderArray::<1>::parse(buf)?;
        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::PubAck);

        let (pkid, rest) = PacketId::parse(rest)?;

        let this = Self { pkid };

        this.validate()?;

        Ok((this, rest))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::tests::{Hexdump, insta_snapshots};

    use super::*;

    #[test]
    fn puback_roundtrip() {
        let pkid = NonZero::new(10u16).unwrap();

        let mut buf = Vec::new();

        let written = PubAck::write_sync(&mut buf, &pkid).unwrap();

        assert_eq!(written, buf.len());

        let res = PubAck::consume(&buf).unwrap();

        assert_eq!(res.pkid.read(), pkid);

        insta_snapshots!({
            insta::assert_snapshot!(Hexdump(buf));
        });
    }
}
