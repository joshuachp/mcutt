//! Handle PINGREQ and PINGRESP packets.

use core::fmt::Display;

use crate::bytes::{Decode, Encode, Input, Parsed};
use crate::v3::packets::common::fixed::ControlPacketType;

use super::common::fixed::FixedHeaderArray;
use super::common::fixed::builder::FixedHeaderBuilder;

pub mod resp;

/// The PINGREQ Packet is sent from a Client to the Server.
///
/// It can be used to:
///
/// 1. Indicate to the Server that the Client is alive in the absence of any other Control Packets
///    being sent from the Client to the Server.
/// 2. Request that the Server responds to confirm that it is alive.
/// 3. Exercise the network to indicate that the Network Connection is active.
#[derive(Debug, Clone, Copy)]
pub struct PingReq {}

impl Display for PingReq {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", ControlPacketType::PingReq)
    }
}

impl Input<()> for PingReq {
    type Validated = ();

    fn validate_value(_value: &()) -> Result<Self::Validated, crate::bytes::Error> {
        Ok(())
    }
}

impl Parsed for PingReq {
    fn validate(&self) -> Result<(), crate::bytes::Error> {
        Ok(())
    }
}

impl Encode<()> for PingReq {
    fn encode_len(_value: &()) -> Result<usize, crate::bytes::Error> {
        Ok(0)
    }

    fn write_sync<W>(writer: &mut W, _value: &()) -> Result<usize, crate::bytes::Error>
    where
        W: std::io::Write,
    {
        FixedHeaderBuilder::new(ControlPacketType::PingReq).write_sync(writer)
    }
}

impl<'a> Decode<'a> for PingReq {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), crate::bytes::Error> {
        let (fixed, rest) = FixedHeaderArray::<0>::parse(buf)?;
        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::PingReq);

        Ok((Self {}, rest))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::tests::{Hexdump, insta_snapshots};

    use super::*;

    #[test]
    fn should_encode_and_decode_pingreq() {
        let mut buf = Vec::new();

        let written = PingReq::write_sync(&mut buf, &()).unwrap();
        assert_eq!(buf.len(), written);

        PingReq::parse(&buf).unwrap();

        insta_snapshots!({
            insta::assert_snapshot!(Hexdump(buf));
        });
    }
}
