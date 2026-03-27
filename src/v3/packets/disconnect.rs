//! Handle the DISCONNECT Packet.

use crate::bytes::{Decode, Encode, Input, Parsed};
use crate::v3::packets::common::fixed::{ControlPacketType, FixedHeaderArray};

use super::common::fixed::builder::FixedHeaderBuilder;

/// The DISCONNECT Packet is the final Control Packet sent from the Client to the Server.
///
/// It indicates that the Client is disconnecting cleanly.
#[derive(Debug, Clone, Copy)]
pub struct Disconnect {}

impl Input<()> for Disconnect {
    type Validated = ();

    fn validate_value(_value: &()) -> Result<Self::Validated, crate::bytes::Error> {
        Ok(())
    }
}

impl Parsed for Disconnect {
    fn validate(&self) -> Result<(), crate::bytes::Error> {
        Ok(())
    }
}

impl Encode<()> for Disconnect {
    fn encode_len(_value: &()) -> Result<usize, crate::bytes::Error> {
        Ok(0)
    }

    fn write_sync<W>(writer: &mut W, _value: &()) -> Result<usize, crate::bytes::Error>
    where
        W: std::io::Write,
    {
        FixedHeaderBuilder::new(ControlPacketType::Disconnect).write_sync(writer)
    }
}

impl<'a> Decode<'a> for Disconnect {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), crate::bytes::Error> {
        let (fixed, rest) = FixedHeaderArray::<1>::parse(buf)?;
        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::Disconnect);

        Ok((Self {}, rest))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::tests::{Hexdump, insta_snapshots};

    use super::*;

    #[test]
    fn disconnect_roundtrip() {
        let mut buf = Vec::new();

        let written = Disconnect::write_sync(&mut buf, &()).unwrap();
        assert_eq!(buf.len(), written);

        Disconnect::parse(&buf).unwrap();

        insta_snapshots!({
            insta::assert_snapshot!(Hexdump(buf));
        });
    }
}
