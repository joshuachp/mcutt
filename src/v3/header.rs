//! Data representation of MQTT packets

use core::{
    fmt::Display,
    mem,
    num::{NonZeroU16, TryFromIntError},
    ops::Deref,
};

use bitflags::bitflags;

use crate::{
    bytes::{read_chunk, read_exact, read_u16, read_u8},
    v3::EncodeError,
};

use super::{Decode, DecodeError, Encode};

/// Error returned by the MQTT [`Str`].
#[derive(Debug)]
#[non_exhaustive]
pub enum StrError {
    /// The length of the string exceeds the maximum size
    MaxBytes {
        /// The length of the string.
        len: usize,
    },
    /// The string is not UTF-8 encoded or contains invalid characters.
    Utf8,
}

impl Display for StrError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StrError::MaxBytes { len } => {
                write!(
                    f,
                    "the string length {len} exceeds the maximum of {}",
                    Str::MAX_BYTES
                )
            }
            StrError::Utf8 => write!(f, "the string was not UTF-8 encoded"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StrError {}

/// UTF-8 encoded string.
///
/// Text fields in the Control Packets described later are encoded as UTF-8 strings.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718016>
#[derive(Debug, Clone, Copy)]
pub struct Str<'a>(&'a str);

impl<'a> Str<'a> {
    /// Max number of bytes in a string
    pub const MAX_BYTES: usize = u16::MAX as usize;

    /// Returns and empty string.
    pub const fn new() -> Self {
        Self("")
    }

    /// The length of the string in bytes.
    pub const fn len_as_bytes(&self) -> usize {
        self.0.as_bytes().len()
    }
}

impl<'a> Default for Str<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> TryFrom<&'a str> for Str<'a> {
    type Error = StrError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let len = value.as_bytes().len();
        if len > Str::MAX_BYTES {
            return Err(StrError::MaxBytes { len });
        }

        value
            .chars()
            .all(is_valid_char)
            .then_some(Str(value))
            .ok_or(StrError::Utf8)
    }
}

impl<'a> Deref for Str<'a> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a> Encode for Str<'a> {
    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let len = u16::try_from(self.len_as_bytes()).map_err(|_| EncodeError::FrameTooBig {
            max: Str::MAX_BYTES,
        })?;

        writer
            .write_u16(len)
            .and_then(|()| writer.write_all(self.as_bytes()))
            .map_err(EncodeError::Write)?;

        Ok(mem::size_of::<u16>() + self.len_as_bytes())
    }
}

impl<'a> Decode<'a> for Str<'a> {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (length, bytes) = read_u16(bytes)?;

        let length = usize::from(length);

        // We don't check the size since we know it was a u16
        let (data, bytes) = read_exact(bytes, length)?;

        let data = core::str::from_utf8(data)
            .map_err(|_| StrError::Utf8)
            .and_then(Str::try_from)?;

        Ok((data, bytes))
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

/// Error returned by the [`BytesBuf`].
#[derive(Debug)]
pub struct BytesBufError {
    pub(crate) len: usize,
}

impl Display for BytesBufError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "the string length {} exceeds the maximum of {}",
            self.len,
            BytesBuf::MAX
        )
    }
}

/// A buffer of bytes for the MQTT packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BytesBuf<'a>(&'a [u8]);

impl<'a> BytesBuf<'a> {
    /// Maximum number of bytes in the buffer.
    pub const MAX: usize = u16::MAX as usize;
}

impl<'a> Deref for BytesBuf<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a> TryFrom<&'a [u8]> for BytesBuf<'a> {
    type Error = BytesBufError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() > BytesBuf::MAX {
            return Err(BytesBufError { len: value.len() });
        }

        Ok(Self(value))
    }
}

impl<'a> Decode<'a> for BytesBuf<'a> {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (len, bytes) = read_u16(bytes)?;

        let len = usize::from(len);

        // We don't check the size since we know it was a u16
        let (buf, bytes) = read_exact(bytes, len)?;

        Ok((Self(buf), bytes))
    }
}

impl<'a> Encode for BytesBuf<'a> {
    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let len = u16::try_from(self.len()).map_err(|_| EncodeError::FrameTooBig {
            max: Str::MAX_BYTES,
        })?;

        writer
            .write_u16(len)
            .and_then(|()| writer.write_all(self))
            .map_err(EncodeError::Write)?;

        Ok(mem::size_of::<u16>() + self.len())
    }
}

/// Raw fixed header.
///
/// This doesn't check the validity of the packet it self,.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718020>
#[derive(Debug, Clone, Copy)]
pub(super) struct RawFixedHeader<'a, const N: usize> {
    type_flags: u8,
    remaining_length: &'a [u8; N],
}

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
                let value = u32::from(b & RemainingLength::VALUE_MASK);
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

impl<'a, const N: usize> Encode for RawFixedHeader<'a, N> {
    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        writer
            .write_u8(self.type_flags)
            .and_then(|()| writer.write_all(self.remaining_length))
            .map_err(EncodeError::Write)?;

        Ok(1 + N)
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
    pub(crate) fn new(
        packet_type: ControlPacketType,
        flags: TypeFlags,
        remaining_length: RemainingLength,
    ) -> Self {
        Self {
            packet_type,
            flags,
            remaining_length,
        }
    }

    pub(crate) fn packet_type(&self) -> ControlPacketType {
        self.packet_type
    }

    #[allow(unused)]
    pub(crate) fn flags(&self) -> TypeFlags {
        self.flags
    }

    pub(crate) fn remaining_length(&self) -> RemainingLength {
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

    /// Internal conversion since we already checked the length of remaining packets
    fn as_raw<'a, const N: usize>(
        &'a self,
        remaining_length: &'a mut [u8; N],
    ) -> RawFixedHeader<N> {
        let type_flags = u8::from(self.packet_type) << 4 | self.flags.bits();

        self.remaining_length.write(remaining_length);

        RawFixedHeader {
            type_flags,
            remaining_length,
        }
    }
}

impl<'a> Decode<'a> for FixedHeader {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        // count the continue flags
        let n_continue = bytes
            .iter()
            .skip(1)
            .take_while(|&b| RemainingLength::CONTINUE_FLAG & *b != 0)
            .take(4)
            .count();

        match n_continue {
            0 => Self::parse_with_length::<1>(bytes),
            1 => Self::parse_with_length::<2>(bytes),
            2 => Self::parse_with_length::<3>(bytes),
            3 => Self::parse_with_length::<4>(bytes),
            _ => Err(DecodeError::RemainingLengthBytes),
        }
    }
}

impl Encode for FixedHeader {
    fn write<W>(&self, writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        match self.remaining_length.0 {
            0..=127 => {
                let mut buf = [0u8; 1];
                let raw = self.as_raw(&mut buf);

                raw.write(writer)
            }
            128..=16_383 => {
                let mut buf = [0u8; 2];
                let raw = self.as_raw(&mut buf);

                raw.write(writer)
            }
            16_384..=2_097_151 => {
                let mut buf = [0u8; 3];
                let raw = self.as_raw(&mut buf);

                raw.write(writer)
            }
            2_097_152..=268_435_455 => {
                let mut buf = [0u8; 4];
                let raw = self.as_raw(&mut buf);

                raw.write(writer)
            }
            _ => unreachable!("remaining length cannot exceed the max"),
        }
    }
}

/// Control Packet Type for the [`FixedHeader`].
///
/// Represented as a 4-bit unsigned value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ControlPacketType {
    /// Client request to connect to Server.
    ///
    /// Sent by Client to Server.
    Connect = 1,
    /// Connect acknowledgment.
    ///
    /// Server to Client.
    ConnAck = 2,
    /// Publish message.
    ///
    /// Sent by both Client and Server.
    Publish = 3,
    //// Publish acknowledgment.
    ///
    /// Sent by both Client and Server.
    PubAck = 4,
    /// Publish received (assured delivery part 1).
    ///
    /// Sent by both Client and Server.
    PubRec = 5,
    /// Publish release (assured delivery part 2)
    ///
    /// Sent by both Client and Server.
    PubRel = 6,
    /// Publish complete (assured delivery part 3).
    ///
    /// Sent by both Client and Server.
    PubComp = 7,
    /// Client subscribe request.
    ///
    /// Sent by both Client and Server.
    Subscribe = 8,
    /// Subscribe acknowledgment.
    ///
    /// Sent by both Client and Server.
    SubAck = 9,
    /// Unsubscribe request.
    ///
    /// Sent by both Client and Server.
    Unsubscribe = 10,
    /// Unsubscribe acknowledgment.
    ///
    /// Sent by both Client and Server.
    UnsubAck = 11,
    /// PING request.
    ///
    /// Sent by Client to Server.
    PingReq = 12,
    /// PING response.
    ///
    /// Sent by Server to Client.
    PingResp = 13,
    /// Client is disconnecting.
    ///
    /// Sent by Client to Server.
    Disconnect = 14,
}

impl ControlPacketType {
    pub(crate) fn read_flags(&self, flags: u8) -> Result<TypeFlags, DecodeError> {
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

impl From<ControlPacketType> for u8 {
    fn from(value: ControlPacketType) -> Self {
        value as u8
    }
}

bitflags! {
    /// Control Packet type flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TypeFlags: u8 {
        /// Mask for the flag in the [`FixedHeader`].
        const MASK = 0b00001111;

        /// Publish is duplicate.
        const PUBLISH_DUP = 0b1000;
        /// Publish QoS first bit.
        const PUBLISH_QOS_1 = 0b0100;
        /// Publish QoS second bit.
        const PUBLISH_QOS_2 = 0b0010;
        /// Publish retain flag.
        const PUBLISH_RETAIN = 0b0001;

        /// Reserved non zero flags for PUBREL
        const PUBREL = 0b0010;
        /// Reserved non zero flags for SUBSCRIBE
        const SUBSCRIBE = 0b0010;
        /// Reserved non zero flags for UNSUBSCRIBE
        const UNSUBSCRIBE = 0b0010;
    }
}

impl Display for TypeFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:04}", self.0)
    }
}

/// Error for an invalid [`RemainingLength`].
#[derive(Debug)]
#[non_exhaustive]
pub enum RemainingLengthError {
    /// The value is greater than the [`max`](RemainingLength::MAX)
    Max {
        /// The invalid value.
        value: u32,
    },
    /// Checked integer conversion failed
    TryFromInt(TryFromIntError),
    /// Invalid remaining length for the packet
    InvalidLength {
        /// The expected length.
        expected: u32,
        /// The actual length.
        actual: u32,
    },
}

impl Display for RemainingLengthError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RemainingLengthError::Max { value } => write!(
                f,
                "remaining length {value} is greater than the maximum {}",
                RemainingLength::MAX
            ),
            RemainingLengthError::TryFromInt(..) => write!(f, "couldn't convert the value to u32"),
            RemainingLengthError::InvalidLength { expected, actual } => {
                write!(f, "packet has must have remaining length of {expected}, but received a length of {actual}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RemainingLengthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RemainingLengthError::Max { .. } | RemainingLengthError::InvalidLength { .. } => None,
            RemainingLengthError::TryFromInt(err) => Some(err),
        }
    }
}

impl From<TryFromIntError> for RemainingLengthError {
    fn from(value: TryFromIntError) -> Self {
        Self::TryFromInt(value)
    }
}

/// The Remaining Length is the number of bytes remaining within the current packet.
///
/// It includes the data in the variable header and the payload. The Remaining Length does not
/// include the bytes used to encode the Remaining Length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemainingLength(u32);

impl RemainingLength {
    const CONTINUE_FLAG: u8 = 0b10000000;
    const VALUE_MASK: u8 = !Self::CONTINUE_FLAG;
    /// Maximum value of the remaining length
    pub const MAX: u32 = 268_435_455;

    fn write<const N: usize>(&self, buf: &mut [u8; N]) {
        let mut value = self.0;

        let iter = core::iter::from_fn(|| {
            // If the value is 0, no bytes should be written
            if value == 0 {
                return None;
            }

            // This will never fail
            let mut encoded = u8::try_from(value.rem_euclid(128)).ok()?;

            value = value.div_euclid(128);

            if value > 0 {
                encoded |= RemainingLength::CONTINUE_FLAG;
            }

            Some(encoded)
        });

        buf.iter_mut().zip(iter).for_each(|(b, v)| {
            *b = v;
        });
    }
}

impl TryFrom<u32> for RemainingLength {
    type Error = RemainingLengthError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value > Self::MAX {
            return Err(RemainingLengthError::Max { value });
        }

        Ok(Self(value))
    }
}

impl TryFrom<usize> for RemainingLength {
    type Error = RemainingLengthError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        let value = u32::try_from(value)?;

        RemainingLength::try_from(value)
    }
}

impl TryFrom<RemainingLength> for usize {
    type Error = RemainingLengthError;

    fn try_from(value: RemainingLength) -> Result<Self, Self::Error> {
        usize::try_from(value.0).map_err(RemainingLengthError::TryFromInt)
    }
}

impl Deref for RemainingLength {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Identifier for a Packet with QoS > 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PacketIdentifier(NonZeroU16);

impl<'a> Decode<'a> for PacketIdentifier {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
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
    use core::panic;

    use pretty_assertions::assert_eq;

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
    fn fixed_headers_as_raw() {
        let type_flags = 0b00010000;
        let data: &[(u32, &[u8])] = &[
            (0, &[0x00]),
            (127, &[0x7F]),
            (128, &[0x80, 0x01]),
            (16_383, &[0xFF, 0x7F]),
            (16_384, &[0x80, 0x80, 0x01]),
            (2_097_151, &[0xFF, 0xFF, 0x7F]),
            (2_097_152, &[0x80, 0x80, 0x80, 0x01]),
            (268_435_455, &[0xFF, 0xFF, 0xFF, 0x7F]),
        ];

        for (v, exp) in data {
            let remaining_length = RemainingLength::try_from(*v).unwrap();
            let fix = FixedHeader::new(
                ControlPacketType::Connect,
                TypeFlags::empty(),
                remaining_length,
            );

            let mut buf = [0u8; 5];

            let raw = fix.as_raw(&mut buf);

            assert_eq!(raw.type_flags, type_flags);
            assert_eq!(raw.remaining_length[exp.len()], 0);
            assert_eq!(&raw.remaining_length[..exp.len()], *exp);
        }
    }

    #[test]
    fn should_encode_remaining_length() {
        let data: &[(u32, &[u8])] = &[
            (0, &[0x00]),
            (127, &[0x7F]),
            (128, &[0x80, 0x01]),
            (16_383, &[0xFF, 0x7F]),
            (16_384, &[0x80, 0x80, 0x01]),
            (2_097_151, &[0xFF, 0xFF, 0x7F]),
            (2_097_152, &[0x80, 0x80, 0x80, 0x01]),
            (268_435_455, &[0xFF, 0xFF, 0xFF, 0x7F]),
        ];

        for (v, exp) in data {
            let mut buf = [0u8; 5];

            let r = RemainingLength::try_from(*v).unwrap();

            r.write(&mut buf);

            assert_eq!(buf[exp.len()], 0);
            assert_eq!(&buf[..exp.len()], *exp);
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

        let err = FixedHeader::parse(bytes).unwrap_err();

        let DecodeError::NotEnoughBytes { needed } = err else {
            panic!("wrong error {err:?}");
        };

        assert_eq!(needed, 1);
    }
}
