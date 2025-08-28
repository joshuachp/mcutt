//! UNSUBACK packet in response to a UNSUBSCRIBE.

use std::fmt::Display;
use std::num::NonZero;

use crate::bytes::{Decode, Error};
use crate::bytes::{Encode, Input, Parsed};
use crate::v3::packets::common::fixed::builder::FixedHeaderBuilder;
use crate::v3::packets::common::fixed::{ControlPacketType, FixedHeaderArray};
use crate::v3::packets::common::packet_id::PacketId;

/// The UNSUBACK Packet is sent by the Server to the Client to confirm receipt of an UNSUBSCRIBE
/// Packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsubAck<'a> {
    pkid: &'a PacketId,
}

impl<'a> UnsubAck<'a> {
    /// Returns the packet identifier of the packet.
    pub fn pkid(&self) -> &PacketId {
        self.pkid
    }
}

impl<'a> Display for UnsubAck<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} pkid({})", ControlPacketType::SubAck, self.pkid)
    }
}

impl<'a> Input<NonZero<u16>> for UnsubAck<'a> {
    type Validated = ();

    fn validate_value(_value: &NonZero<u16>) -> Result<Self::Validated, crate::bytes::Error> {
        Ok(())
    }
}

impl<'a> Parsed for UnsubAck<'a> {
    fn validate(&self) -> Result<(), crate::bytes::Error> {
        Ok(())
    }
}

impl<'a> Encode<NonZero<u16>> for UnsubAck<'a> {
    fn encode_len(value: &NonZero<u16>) -> Result<usize, Error> {
        PacketId::encode_len(value)
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &NonZero<u16>) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        let mut written = FixedHeaderBuilder::new(ControlPacketType::UnsubAck)
            .remaining_length(Self::encode_len(value)?)?
            .write_sync(writer)?;

        written += PacketId::write_sync(writer, value)?;

        Ok(written)
    }
}

impl<'a> Decode<'a> for UnsubAck<'a> {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), crate::bytes::Error> {
        let (fixed, rest) = FixedHeaderArray::<1>::parse(buf)?;
        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::UnsubAck);

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
    fn unsuback_roundtrip() {
        let pkid = NonZero::new(10u16).unwrap();

        let mut buf = Vec::new();

        let written = UnsubAck::write_sync(&mut buf, &pkid).unwrap();
        assert_eq!(written, buf.len());

        let res = UnsubAck::consume(&buf).unwrap();

        assert_eq!(res.pkid.read(), pkid);

        insta_snapshots!({
            insta::assert_snapshot!(Hexdump(buf));
        });
    }
}
