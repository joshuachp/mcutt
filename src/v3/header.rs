//! Data representation of MQTT packets

use core::{fmt::Display, num::NonZeroU16, ops::Deref};

use bitflags::bitflags;

use crate::bytes::{read_chunk, read_exact, read_u16, read_u8};

use super::{Decode, DecodeError};

/// UTF-8 encoded string.
///
/// Text fields in the Control Packets described later are encoded as UTF-8 strings.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718016>
#[derive(Debug, Clone, Copy)]
pub struct Str<'a>(&'a str);

impl<'a> TryFrom<&'a str> for Str<'a> {
    type Error = DecodeError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        value
            .chars()
            .all(is_valid_char)
            .then_some(Str(value))
            .ok_or(DecodeError::Utf8)
    }
}

impl<'a> Deref for Str<'a> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

/// Check if the UTF-8 character is valid for the MQTT string.
///
/// This will check for NULL, control and non characters.
///
/// See <https://www.unicode.org/faq/private_use.html#nonchar1> of a list of non characters. This
/// set is formally immutable.
fn is_valid_char(c: char) -> bool {
    match c {
        '\u{0000}'
        | '\u{0001}'..='\u{001F}'
        | '\u{FDD0}'..='\u{FDEF}'
        | '\u{FFFE}'..='\u{FFFF}' => false,
        _ => {
            // Last two of the supplementary planes
            if is_last_two_supplementary(c) {
                return false;
            }

            true
        }
    }
}

/// The last two code points of each of the 16 supplementary planes.
///
// ```
// U+1FFFE, U+1FFFF, U+2FFFE, U+2FFFF, ... U+10FFFE, U+10FFFF
// ```
#[inline]
fn is_last_two_supplementary(c: char) -> bool {
    let val = u32::from(c);

    let last_bits = val & 0xFF;

    (last_bits == 0xFE || last_bits == 0xFF) && (0x01..=0x10).contains(&(val >> 16))
}

pub const MAX_STR_BYTES: usize = 65_535;

impl<'a> Decode<'a> for Str<'a> {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (length, bytes) = read_u16(bytes)?;

        let length = usize::from(length);

        if length > MAX_STR_BYTES {
            return Err(DecodeError::FrameTooBig { max: MAX_STR_BYTES });
        }

        let (data, bytes) = read_exact(bytes, length)?;

        let data = core::str::from_utf8(data)
            .map_err(|_| DecodeError::Utf8)
            .and_then(Str::try_from)?;

        Ok((data, bytes))
    }
}

/// Raw fixed header.
///
/// This doesn't check the validity of the packet it self,.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718020>
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct RawFixedHeader<'a, const N: usize> {
    type_flags: u8,
    remaining_length: &'a [u8; N],
}

const CONTINUE_FLAG: u8 = 0b10000000;
const VALUE_MASK: u8 = !CONTINUE_FLAG;

impl<'a, const N: usize> RawFixedHeader<'a, N> {
    const _CHECK: () = assert!(N <= 4);

    fn read(bytes: &'a [u8]) -> Result<(Self, &[u8]), DecodeError> {
        let (type_flags, rest) = read_u8(bytes)?;

        let (remaining_length, rest) = read_chunk(rest)?;

        Ok((
            Self {
                type_flags,
                remaining_length,
            },
            rest,
        ))
    }

    /// The bytes must be a valid remaining length.
    fn read_packet_type(&self) -> u8 {
        self.type_flags >> 4
    }

    /// The bytes must be a valid remaining length.
    fn read_remaining_length(&self) -> u32 {
        let mut multiplier = 1;

        self.remaining_length
            .iter()
            .map(|b| {
                let value = u32::from(b & VALUE_MASK);
                let value = value * multiplier;

                multiplier *= 128;

                value
            })
            .sum()
    }

    fn read_packet_flags(&self) -> u8 {
        self.type_flags & TypeFlags::MASK.bits()
    }
}

/// Fixed header containing the packet type, flags and remaining length of the payload.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718020>
#[derive(Debug, Clone, Copy)]
pub struct FixedHeader {
    packet_type: ControlPacketType,
    flags: TypeFlags,
    remaining_length: RemainingLength,
}

impl FixedHeader {
    pub fn packet_type(&self) -> &ControlPacketType {
        &self.packet_type
    }

    pub fn flags(&self) -> &TypeFlags {
        &self.flags
    }

    pub fn remaining_length(&self) -> RemainingLength {
        self.remaining_length
    }

    fn parse_with_length<const N: usize>(bytes: &[u8]) -> Result<(Self, &[u8]), DecodeError> {
        let (raw, rest) = RawFixedHeader::<N>::read(bytes)?;

        let fixed = FixedHeader::from_raw(raw)?;

        Ok((fixed, rest))
    }

    /// Internal conversion since we already checked the length of remaining packets
    fn from_raw<const N: usize>(raw: RawFixedHeader<N>) -> Result<Self, DecodeError> {
        let packet_type = ControlPacketType::try_from(raw.read_packet_type())?;
        let flags = packet_type.read_flags(raw.read_packet_flags())?;
        // This was validated while reading the raw
        let remaining_length = RemainingLength(raw.read_remaining_length());

        Ok(Self {
            packet_type,
            flags,
            remaining_length,
        })
    }
}

impl<'a> Decode<'a> for FixedHeader {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        // count the continue flags
        let n_continue = bytes
            .iter()
            .skip(1)
            .take_while(|&b| CONTINUE_FLAG & *b != 0)
            .take(4)
            .count();

        let actual_size = 1 + n_continue + 1;
        if bytes.len() < actual_size {
            return Err(DecodeError::NotEnoughBytes {
                needed: actual_size,
                actual: bytes.len(),
            });
        }

        match n_continue {
            0 => Self::parse_with_length::<1>(bytes),
            1 => Self::parse_with_length::<2>(bytes),
            2 => Self::parse_with_length::<3>(bytes),
            3 => Self::parse_with_length::<4>(bytes),
            _ => Err(DecodeError::RemainingLengthBytes),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl ControlPacketType {
    pub fn read_flags(&self, flags: u8) -> Result<TypeFlags, DecodeError> {
        let flags = TypeFlags::from_bits_retain(flags & TypeFlags::MASK.bits());

        match self {
            ControlPacketType::Publish => {}
            ControlPacketType::PubRel => {
                if flags != TypeFlags::PUBREL {
                    return Err(DecodeError::ControlFlags {
                        packet_type: *self,
                        flags,
                    });
                }
            }
            ControlPacketType::Subscribe => {
                if flags != TypeFlags::SUBSCRIBE {
                    return Err(DecodeError::ControlFlags {
                        packet_type: *self,
                        flags,
                    });
                }
            }
            ControlPacketType::Unsubscribe => {
                if flags != TypeFlags::UNSUBSCRIBE {
                    return Err(DecodeError::ControlFlags {
                        packet_type: *self,
                        flags,
                    });
                }
            }
            _ => {
                if !flags.is_empty() {
                    return Err(DecodeError::ControlFlags {
                        packet_type: *self,
                        flags,
                    });
                }
            }
        }

        Ok(flags)
    }
}

impl Display for ControlPacketType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ControlPacketType::Connect => write!(f, "CONNECT({})", *self as u8),
            ControlPacketType::ConnAck => write!(f, "CONNACK({})", *self as u8),
            ControlPacketType::Publish => write!(f, "PUBLISH({})", *self as u8),
            ControlPacketType::PubAck => write!(f, "PUBACK({})", *self as u8),
            ControlPacketType::PubRec => write!(f, "PUBREC({})", *self as u8),
            ControlPacketType::PubRel => write!(f, "PUBREL({})", *self as u8),
            ControlPacketType::PubComp => write!(f, "PUBCOMP({})", *self as u8),
            ControlPacketType::Subscribe => write!(f, "SUBSCRIBE({})", *self as u8),
            ControlPacketType::SubAck => write!(f, "SUBACK({})", *self as u8),
            ControlPacketType::Unsubscribe => write!(f, "UNSUBSCRIBE({})", *self as u8),
            ControlPacketType::UnsubAck => write!(f, "UNSUBACK({})", *self as u8),
            ControlPacketType::PingReq => write!(f, "PINGREQ({})", *self as u8),
            ControlPacketType::PingResp => write!(f, "PINGRESP({})", *self as u8),
            ControlPacketType::Disconnect => write!(f, "DISCONNECT({})", *self as u8),
        }
    }
}

impl TryFrom<u8> for ControlPacketType {
    type Error = DecodeError;

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
            _ => return Err(DecodeError::PacketType(value)),
        };

        Ok(value)
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl Display for TypeFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:04}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemainingLength(u32);

impl RemainingLength {
    /// Maximum value of the remaining length
    pub const MAX: u32 = 268_435_455;
}

impl TryFrom<u32> for RemainingLength {
    type Error = DecodeError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value > Self::MAX {
            return Err(DecodeError::MaxRemainingLength(value));
        }

        Ok(Self(value))
    }
}

impl Deref for RemainingLength {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PacketIdentifier(NonZeroU16);

impl PacketIdentifier {
    pub fn parse(bytes: &[u8]) -> Result<(Self, &[u8]), DecodeError> {
        let (pkid, rest) = read_u16(bytes)?;

        let pkid = NonZeroU16::new(pkid).ok_or(DecodeError::PacketIdentifier)?;

        Ok((Self(pkid), rest))
    }
}

impl Deref for PacketIdentifier {
    type Target = NonZeroU16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_example_str() {
        const EXAMPLE_STRING: &[u8] = &[0x00, 0x05, 0x41, 0xF0, 0xAA, 0x9B, 0x94];
        const EXPECTED: &str = "A𪛔";

        let (str, rest) = Str::parse(EXAMPLE_STRING).unwrap();

        assert!(rest.is_empty());

        assert_eq!(&*str, EXPECTED);
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
    fn should_parse_fixed_headers_remaining_length() {
        let data: &[(u32, &[u8])] = &[
            (0, &[0b00010000, 0x00]),
            (127, &[0b00010000, 0x7F]),
            (128, &[0b00010000, 0x80, 0x01]),
            (16_383, &[0b00010000, 0xFF, 0x7F]),
            (16_384, &[0b00010000, 0x80, 0x80, 0x01]),
            (2_097_151, &[0b00010000, 0xFF, 0xFF, 0x7F]),
            (2_097_152, &[0b00010000, 0x80, 0x80, 0x80, 0x01]),
            (268_435_455, &[0b00010000, 0xFF, 0xFF, 0xFF, 0x7F]),
        ];

        for (exp, bytes) in data {
            let (fixed, rest) = FixedHeader::parse(bytes).unwrap();

            assert!(rest.is_empty());

            assert_eq!(*fixed.remaining_length, *exp);
        }
    }

    #[test]
    fn should_be_a_valid_char() {
        let null = '\u{0000}';

        let ranges = [
            '\u{0001}'..='\u{001F}',
            '\u{FDD0}'..='\u{FDEF}',
            '\u{FFFE}'..='\u{FFFF}',
        ];

        let supplementary = [
            '\u{1FFFE}',
            '\u{1FFFF}',
            '\u{2FFFE}',
            '\u{2FFFF}',
            '\u{3FFFE}',
            '\u{3FFFF}',
            '\u{4FFFE}',
            '\u{4FFFF}',
            '\u{5FFFE}',
            '\u{5FFFF}',
            '\u{6FFFE}',
            '\u{6FFFF}',
            '\u{7FFFE}',
            '\u{7FFFF}',
            '\u{8FFFE}',
            '\u{8FFFF}',
            '\u{9FFFE}',
            '\u{9FFFF}',
            '\u{10FFFE}',
            '\u{10FFFF}',
        ];

        let iter = core::iter::once(null)
            .chain(ranges.into_iter().flatten())
            .chain(supplementary);

        for c in iter {
            assert!(
                !is_valid_char(c),
                "should not be valid {:08X}",
                u32::from(c)
            );
        }
    }

    #[test]
    fn alpha_numeric_should_be_valid() {
        let iter = ['a'..='z', 'A'..='Z', '0'..='9'];
        for c in iter.into_iter().flatten() {
            assert!(is_valid_char(c));
        }
    }

    #[test]
    fn should_require_more_bytes_for_fixed_header() {
        let bytes = &[0b00010000, 0x80];

        let res = FixedHeader::parse(bytes).unwrap_err();
        assert!(matches!(
            res,
            DecodeError::NotEnoughBytes {
                needed: 3,
                actual: 2
            }
        ))
    }
}
