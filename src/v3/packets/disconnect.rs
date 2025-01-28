//! Handle the DISCONNECT Packet.

use crate::v3::packets::DecodeError;

use super::{
    header::{ControlPacketType, FixedHeader, RemainingLength, TypeFlags},
    DecodePacket, EncodePacket,
};

/// The DISCONNECT Packet is the final Control Packet sent from the Client to the Server.
///
/// It indicates that the Client is disconnecting cleanly.
#[derive(Debug, Clone, Copy)]
pub struct Disconnect {}

impl Disconnect {
    const REMAINING_LENGTH: RemainingLength = RemainingLength::new_const(0);
}

impl EncodePacket for Disconnect {
    fn remaining_len(&self) -> usize {
        0
    }

    fn packet_type() -> super::header::ControlPacketType {
        ControlPacketType::Disconnect
    }

    fn packet_flags(&self) -> TypeFlags {
        TypeFlags::empty()
    }

    fn write_packet<W>(&self, _writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        // The DISCONNECT has no variable header nor payload
        Ok(0)
    }
}

impl<'a> DecodePacket<'a> for Disconnect {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::Disconnect
    }

    fn fixed_remaining_length() -> Option<RemainingLength> {
        Some(Self::REMAINING_LENGTH)
    }

    fn parse_with_header(header: FixedHeader, bytes: &'a [u8]) -> Result<Self, super::DecodeError> {
        if !header.flags().is_empty() {
            return Err(DecodeError::Reserved);
        }

        debug_assert!(
            bytes.is_empty(),
            "BUG: remaining length was correct, but bytes are still present after parsing"
        );

        Ok(Disconnect {})
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::v3::packets::tests::TestWriter;

    use super::*;

    #[test]
    fn should_encode_and_decode_disconnect() {
        let exp_bytes = [0b1110_0000u8, 0b0000_0000];

        let mut writer = TestWriter::new();

        Disconnect {}.write(&mut writer).unwrap();

        assert_eq!(writer.buf, exp_bytes);

        let (_disconnect, bytes) = Disconnect::parse(&exp_bytes).unwrap();

        assert!(bytes.is_empty());
    }
}
