//! Data representation of MQTT packets

use bitflags::bitflags;
use zerocopy::byteorder::network_endian::U16;
use zerocopy::{ByteSlice, Ref};

/// UTF-8 encoded string.
///
/// Text fields in the Control Packets described later are encoded as UTF-8 strings.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718016>
pub fn parse_str<'a, B: ByteSlice + 'a>(bytes: B) -> Option<(&'a str, B)> {
    let (length, bytes) = Ref::<B, U16>::new_from_prefix(bytes).unwrap();

    let length = usize::from(length.read());

    let (data, bytes) = Ref::new_slice_from_prefix(bytes, length)?;

    core::str::from_utf8(data.into_slice())
        .ok()
        .map(|s| (s, bytes))
}

pub struct RawFixedHeader<B> {
    type_flags: u8,
    remaining_length: B,
}

const CONTINUE_FLAG: u8 = 0b10000000;
const VALUE_MASK: u8 = !CONTINUE_FLAG;

impl<B: ByteSlice> RawFixedHeader<B> {
    pub fn parse(bytes: B) -> Option<(Self, B)> {
        let (type_flags, rest) = Ref::<B, u8>::new_from_prefix(bytes)?;

        // Count the bytes in the remaining length
        let count = rest
            .iter()
            .take_while(|&b| CONTINUE_FLAG & *b != 0)
            // Take 5 to check the that there are at most 4 bytes
            .take(5)
            .count();

        // Too many or none bytes in remaining length
        if !(1..=4).contains(&count) {
            return None;
        }

        let (remaining_length, rest) = rest.split_at(count);

        Some((
            Self {
                type_flags: *type_flags,
                remaining_length,
            },
            rest,
        ))
    }
}

#[derive(Debug)]
pub struct FixedHeader {
    packet_type: ControlPacketType,
    flags: TypeFlags,
    remaining_length: u32,
}

impl FixedHeader {
    pub fn packet_type(&self) -> &ControlPacketType {
        &self.packet_type
    }

    pub fn flags(&self) -> &TypeFlags {
        &self.flags
    }

    pub fn remaining_length(&self) -> u32 {
        self.remaining_length
    }
}

impl<B: ByteSlice> TryFrom<RawFixedHeader<B>> for FixedHeader {
    type Error = ();

    fn try_from(value: RawFixedHeader<B>) -> Result<Self, Self::Error> {
        let packet_type = value
            .type_flags
            .checked_shr(4)
            .ok_or(())
            .and_then(ControlPacketType::try_from)?;

        // We already unset all the bits with the mask
        let flags = TypeFlags::from_bits_retain(value.type_flags & TypeFlags::MASK.bits());

        match packet_type {
            // All flgs can be set
            ControlPacketType::Publish => {}
            ControlPacketType::PubRel => {
                if flags.0 != TypeFlags::PUBREL.0 {
                    return Err(());
                }
            }
            ControlPacketType::Subscribe => {
                if flags.0 != TypeFlags::SUBSCRIBE.0 {
                    return Err(());
                }
            }
            ControlPacketType::Unsubscribe => {
                if flags.0 != TypeFlags::UNSUBSCRIBE.0 {
                    return Err(());
                }
            }
            _ => {
                if !flags.is_empty() {
                    return Err(());
                }
            }
        }

        let remaining_length = remaining_length_to_u32(value.remaining_length);

        Ok(Self {
            packet_type,
            flags,
            remaining_length,
        })
    }
}

/// The bytes must be a valid remaining length.
fn remaining_length_to_u32<B: ByteSlice>(remaining_length: B) -> u32 {
    let mut multiplier = 1;

    remaining_length
        .iter()
        .map(|b| {
            let value = u32::from(b & VALUE_MASK);
            let value = value * multiplier;

            multiplier *= 128;

            value
        })
        .sum()
}

#[derive(Debug)]
#[repr(u8)]
pub enum ControlPacketType {
    Connect = 1,
    ConnAck = 2,
    Publish = 3,
    PubAck = 4,
    PubRec = 5,
    PubRel = 6,
    PubComp = 7,
    Subscribe = 8,
    SubAck = 9,
    Unsubscribe = 10,
    UnsubAck = 11,
    PingReq = 12,
    PingResp = 13,
    Disconnect = 14,
}

impl TryFrom<u8> for ControlPacketType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let value = match value {
            1 => ControlPacketType::Connect,
            2 => ControlPacketType::ConnAck,
            3 => ControlPacketType::Publish,
            4 => ControlPacketType::PubAck,
            5 => ControlPacketType::PubRec,
            6 => ControlPacketType::PubRel,
            7 => ControlPacketType::PubComp,
            8 => ControlPacketType::Subscribe,
            9 => ControlPacketType::SubAck,
            10 => ControlPacketType::Unsubscribe,
            11 => ControlPacketType::UnsubAck,
            12 => ControlPacketType::PingReq,
            13 => ControlPacketType::PingResp,
            14 => ControlPacketType::Disconnect,
            _ => return Err(()),
        };

        Ok(value)
    }
}

bitflags! {
    #[derive(Debug)]
    pub struct TypeFlags: u8 {
        const MASK = 0b00001111;

        const PUBLISH_DUP = 0b1000;
        const PUBLISH_QOS_1 = 0b0100;
        const PUBLISH_QOS_2 = 0b0010;
        const PUBLISH_RETAIN = 0b0001;

        const PUBREL = 0b0010;
        const SUBSCRIBE = 0b0010;
        const UNSUBSCRIBE = 0b0010;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_example_str() {
        const EXAMPLE_STRING: &[u8] = &[0x00, 0x05, 0x41, 0xF0, 0xAA, 0x9B, 0x94];
        const EXPECTED: &str = "A𪛔";

        let (str, rest) = parse_str(EXAMPLE_STRING).unwrap();

        assert!(rest.is_empty());

        assert_eq!(str, EXPECTED);
    }

    #[test]
    fn should_convert_packet_type() {
        for i in 1..14 {
            let t = ControlPacketType::try_from(i).unwrap();

            let b = t as u8;

            assert_eq!(i, b);
        }
    }

    #[test]
    fn should_convert_remaining_length() {
        let data: [(u32, &[u8]); 4] = [
            (0, &[0x00]),
            (128, &[0x80, 0x01]),
            (16384, &[0x80, 0x80, 0x01]),
            (2097152, &[0x80, 0x80, 0x80, 0x01]),
        ];

        for (exp, input) in data {
            let res = remaining_length_to_u32(input);

            assert_eq!(res, exp);
        }
    }
}
