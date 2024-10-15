//! Handle PINGREQ and PINGRESP packets.

use core::fmt::Display;

use crate::v3::{header::ControlPacketType, DecodeError};

use super::{
    header::{FixedHeader, RemainingLength, TypeFlags},
    DecodePacket, EncodePacket,
};

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

impl PingReq {
    const REMAINING_LENGTH: RemainingLength = RemainingLength::new_const(0);
}

impl Display for PingReq {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", ControlPacketType::PingReq)
    }
}

impl EncodePacket for PingReq {
    fn remaining_len(&self) -> usize {
        0
    }

    fn packet_type() -> ControlPacketType {
        ControlPacketType::PingReq
    }

    fn packet_flags(&self) -> super::header::TypeFlags {
        TypeFlags::empty()
    }

    fn write_packet<W>(&self, _writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        // The PINGREQ has no variable header nor payload
        Ok(0)
    }
}

impl<'a> DecodePacket<'a> for PingReq {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::PingReq
    }

    fn fixed_remaining_length() -> Option<super::header::RemainingLength> {
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

        Ok(PingReq {})
    }
}

/// The PINGRESP Packet is sent by the Server to the Client in response to a PINGREQ Packet.
///
/// It indicates that the server is alive.
#[derive(Debug, Clone, Copy)]
pub struct PingResp {}

impl PingResp {
    const REMAINING_LENGTH: RemainingLength = RemainingLength::new_const(0);
}

impl Display for PingResp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", ControlPacketType::PingResp)
    }
}

impl EncodePacket for PingResp {
    fn remaining_len(&self) -> usize {
        0
    }

    fn packet_type() -> ControlPacketType {
        ControlPacketType::PingResp
    }

    fn packet_flags(&self) -> super::header::TypeFlags {
        TypeFlags::empty()
    }

    fn write_packet<W>(&self, _writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        // The PINGRESP has no variable header nor payload
        Ok(0)
    }
}

impl<'a> DecodePacket<'a> for PingResp {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::PingResp
    }

    fn fixed_remaining_length() -> Option<super::header::RemainingLength> {
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

        Ok(PingResp {})
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::v3::tests::TestWriter;

    use super::*;

    #[test]
    fn should_encode_and_decode_pingreq() {
        let exp_bytes = [0b1100_0000u8, 0b0000_0000u8];

        let mut writer = TestWriter::new();

        PingReq {}.write(&mut writer).unwrap();

        assert_eq!(writer.buf, exp_bytes);

        let (_ping_req, bytes) = PingReq::parse(&exp_bytes).unwrap();

        assert!(bytes.is_empty());
    }

    #[test]
    fn should_encode_and_decode_pingresp() {
        let exp_bytes = [0b1101_0000u8, 0b0000_0000u8];

        let mut writer = TestWriter::new();

        PingResp {}.write(&mut writer).unwrap();

        assert_eq!(writer.buf, exp_bytes);

        let (_ping_resp, bytes) = PingResp::parse(&exp_bytes).unwrap();

        assert!(bytes.is_empty());
    }
}
