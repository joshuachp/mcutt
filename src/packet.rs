//! Data representation of MQTT packets

use core::{fmt::Display, mem, num::NonZeroU16, ops::Deref};

#[cfg(feature = "std")]
use std::error::Error;

use bitflags::bitflags;

use crate::bytes::{read_chunk, read_exact, read_u16, read_u8};

#[derive(Debug)]
pub enum DecodeError {
    NotEnoughBytes {
        needed: usize,
        actual: usize,
    },
    Utf8,
    RemainingLengthBytes,
    ControlFlags {
        packet_type: ControlPacketType,
        flags: TypeFlags,
    },
    PacketType(u8),
    MaxRemainingLength(u32),
    PacketIdentifier,
}

impl Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::NotEnoughBytes { needed, actual } => {
                write!(
                    f,
                    "not enough bytes, buffer contains {actual} but {needed} were needed"
                )
            }
            DecodeError::Utf8 => write!(f, "the string was not UTF-8 encoded"),
            DecodeError::RemainingLengthBytes => {
                write!(f, "remaining length exceeded the maximum of 4 bytes")
            }
            DecodeError::ControlFlags { packet_type, flags } => {
                write!(f, "invalid control packet flags {flags} for {packet_type}")
            }
            DecodeError::PacketType(packet_type) => {
                write!(f, "invalid control packet type {packet_type}")
            }
            DecodeError::MaxRemainingLength(value) => {
                write!(
                    f,
                    "remaining length {value} is bigger than the maximum {}",
                    RemainingLength::MAX
                )
            }
            DecodeError::PacketIdentifier => {
                write!(f, "the packet identifier needs to be non-zero")
            }
        }
    }
}

#[cfg(feature = "std")]
impl Error for DecodeError {}

/// Parses UTF-8 encoded string.
///
/// Text fields in the Control Packets described later are encoded as UTF-8 strings.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718016>
pub fn parse_str(bytes: &[u8]) -> Result<(&str, &[u8]), DecodeError> {
    let (length, rest) = read_u16(bytes)?;

    let (data, rest) = read_exact(rest, length.into())?;

    core::str::from_utf8(data)
        .map(|s| (s, rest))
        .map_err(|_| DecodeError::Utf8)
}

/// Raw fixed header.
///
/// This doesn't check the validity of the packet it self.
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

#[derive(Debug)]
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

    pub fn parse(bytes: &[u8]) -> Result<(Self, &[u8]), DecodeError> {
        let mut c_flag = true;

        // Count the bytes in the remaining length
        let count = bytes
            .iter()
            .skip(1)
            .take_while(|&b| {
                // This stores the continue flag check for next iteration
                mem::replace(&mut c_flag, CONTINUE_FLAG & *b != 0)
            })
            // Take 5 to check the that there are at most 4 bytes
            .take(5)
            .count();

        match count {
            0 => Err(DecodeError::NotEnoughBytes {
                needed: 1,
                actual: 0,
            }),
            1 => Self::parse_with_length::<1>(bytes),
            2 => Self::parse_with_length::<2>(bytes),
            3 => Self::parse_with_length::<3>(bytes),
            4 => Self::parse_with_length::<4>(bytes),
            _ => Err(DecodeError::RemainingLengthBytes),
        }
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
}
