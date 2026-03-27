//! Handle PINGRESP packet for KeepAlive processing.

use std::fmt::Display;

use crate::bytes::{Decode, Encode, Input, Parsed};
use crate::v3::packets::common::fixed::builder::FixedHeaderBuilder;
use crate::v3::packets::common::fixed::{ControlPacketType, FixedHeaderArray};

/// The PINGRESP Packet is sent by the Server to the Client in response to a PINGREQ Packet.
///
/// It indicates that the server is alive.
#[derive(Debug, Clone, Copy)]
pub struct PingResp {}

impl Display for PingResp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", ControlPacketType::PingResp)
    }
}

impl Input<()> for PingResp {
    type Validated = ();

    fn validate_value(_value: &()) -> Result<Self::Validated, crate::bytes::Error> {
        Ok(())
    }
}

impl Parsed for PingResp {
    fn validate(&self) -> Result<(), crate::bytes::Error> {
        Ok(())
    }
}

impl Encode<()> for PingResp {
    fn encode_len(_value: &()) -> Result<usize, crate::bytes::Error> {
        Ok(0)
    }

    fn write_sync<W>(writer: &mut W, _value: &()) -> Result<usize, crate::bytes::Error>
    where
        W: std::io::Write,
    {
        FixedHeaderBuilder::new(ControlPacketType::PingResp).write_sync(writer)
    }
}

impl<'a> Decode<'a> for PingResp {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), crate::bytes::Error> {
        let (fixed, rest) = FixedHeaderArray::<1>::parse(buf)?;
        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::PingResp);

        Ok((Self {}, rest))
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::{Hexdump, insta_snapshots};

    use super::*;

    #[test]
    fn should_encode_and_decode_pingresp() {
        let mut buf = Vec::new();

        let written = PingResp::write_sync(&mut buf, &()).unwrap();
        assert_eq!(buf.len(), written);

        PingResp::parse(&buf).unwrap();

        insta_snapshots!({
            insta::assert_snapshot!(Hexdump(buf));
        });
    }
}
