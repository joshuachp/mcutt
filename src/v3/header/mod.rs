//! Data representation of MQTT packets

use core::{fmt::Display, mem, num::NonZeroU16, ops::Deref};

use bitflags::bitflags;

use crate::{
    bytes::{read_exact, read_u16, read_u8},
    v3::EncodeError,
};

use super::{Decode, DecodeError, Encode};

#[cfg(feature = "alloc")]
pub use self::alloc::StrOwned;
pub use remaining_length::{RemainingLength, RemainingLengthError};

#[cfg(feature = "alloc")]
mod alloc;
mod remaining_length;

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

/// [`Str`] with borrowed data.
pub type StrRef<'a> = Str<&'a str>;

/// UTF-8 encoded string.
///
/// Text fields in the Control Packets described later are encoded as UTF-8 strings.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718016>
#[derive(Debug, Clone, Copy, Eq)]
pub struct Str<S>(S);

impl<S> Str<S> {
    /// The length of the string in bytes.
    #[must_use]
    pub fn len_as_bytes(&self) -> usize
    where
        S: Deref<Target = str>,
    {
        self.0.as_bytes().len()
    }
}

impl<'a> Str<&'a str> {
    /// Max number of bytes in a string
    pub const MAX_BYTES: usize = u16::MAX as usize;

    /// Returns and empty string.
    #[must_use]
    pub const fn new() -> Self {
        Self("")
    }

    /// Returns the inner string.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

impl<S> Display for Str<S>
where
    S: Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<'a> Default for StrRef<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S1, S2> PartialEq<Str<S2>> for Str<S1>
where
    S1: PartialEq<S2>,
{
    fn eq(&self, other: &Str<S2>) -> bool {
        self.0.eq(&other.0)
    }
}

impl<'a, S> From<&'a Str<S>> for StrRef<'a>
where
    S: Deref<Target = str>,
{
    fn from(value: &'a Str<S>) -> Self {
        Self(value)
    }
}

impl<'a> TryFrom<&'a str> for StrRef<'a> {
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

impl<S> Deref for Str<S>
where
    S: Deref<Target = str>,
{
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> Encode for Str<S>
where
    S: Deref<Target = str>,
{
    fn encode_len(&self) -> usize {
        mem::size_of::<u16>().saturating_add(self.len_as_bytes())
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let len = u16::try_from(self.len_as_bytes()).map_err(|_| EncodeError::FrameTooBig {
            max: Str::MAX_BYTES,
        })?;

        let len = writer.write_u16(len).map_err(EncodeError::Write)?;
        let bytes = writer
            .write_slice(self.as_bytes())
            .map_err(EncodeError::Write)?;

        Ok(len.saturating_add(bytes))
    }
}

impl<'a> Decode<'a> for StrRef<'a> {
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
    fn encode_len(&self) -> usize {
        mem::size_of::<u16>().saturating_add(self.len())
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let len = u16::try_from(self.len()).map_err(|_| EncodeError::FrameTooBig {
            max: Str::MAX_BYTES,
        })?;

        let len = writer.write_u16(len).map_err(EncodeError::Write)?;
        let bytes = writer.write_slice(self).map_err(EncodeError::Write)?;

        Ok(len.saturating_add(bytes))
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
    /// Max number of bytes the header will require
    pub const MAX_BYTES: usize = 1 + 4;

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

    pub(crate) fn packet_type(self) -> ControlPacketType {
        self.packet_type
    }

    #[allow(unused)]
    pub(crate) fn flags(self) -> TypeFlags {
        self.flags
    }

    pub(crate) fn remaining_length(self) -> RemainingLength {
        self.remaining_length
    }
}

impl<'a> Decode<'a> for FixedHeader {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (type_and_flags, bytes) = read_u8(bytes)?;

        let packet_type = ControlPacketType::try_from(type_and_flags >> 4)?;
        let flags = packet_type.read_flags(type_and_flags & TypeFlags::MASK.bits())?;

        let (remaining_length, bytes) = RemainingLength::parse(bytes)?;

        Ok((
            Self {
                packet_type,
                flags,
                remaining_length,
            },
            bytes,
        ))
    }
}

impl Encode for FixedHeader {
    fn encode_len(&self) -> usize {
        mem::size_of::<u8>().saturating_add(self.remaining_length.encode_len())
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        writer
            .write_u8(u8::from(self.packet_type) << 4 | self.flags().bits())
            .map_err(EncodeError::Write)?;

        self.remaining_length.write(writer)?;

        Ok(self.encode_len())
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
    pub(crate) fn read_flags(self, flags: u8) -> Result<TypeFlags, DecodeError> {
        let flags = TypeFlags::from_bits_retain(flags & TypeFlags::MASK.bits());

        match self {
            ControlPacketType::Publish => {}
            ControlPacketType::PubRel => {
                if flags != TypeFlags::PUBREL {
                    return Err(DecodeError::ControlFlags {
                        packet_type: self,
                        flags,
                    });
                }
            }
            ControlPacketType::Subscribe => {
                if flags != TypeFlags::SUBSCRIBE {
                    return Err(DecodeError::ControlFlags {
                        packet_type: self,
                        flags,
                    });
                }
            }
            ControlPacketType::Unsubscribe => {
                if flags != TypeFlags::UNSUBSCRIBE {
                    return Err(DecodeError::ControlFlags {
                        packet_type: self,
                        flags,
                    });
                }
            }
            _ => {
                if !flags.is_empty() {
                    return Err(DecodeError::ControlFlags {
                        packet_type: self,
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
        const MASK = 0b0000_1111;

        /// Publish is duplicate.
        const PUBLISH_DUP = 0b1000;
        /// Publish QoS first bit.
        const PUBLISH_QOS_1 = 0b0010;
        /// Publish QoS second bit.
        const PUBLISH_QOS_2 = 0b0100;
        /// Publish QoS both bytes
        const PUBLISH_QOS_MASK = 0b0110;
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

/// Error from an invalid [`PacketId`].
///
/// The packet id must be a valid non zero [`u16`] value.
#[derive(Debug)]
pub struct PacketIdError;

impl Display for PacketIdError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "the packet id must be a valid non zero u16 value")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PacketIdError {}

/// Identifier for a Packet with QoS > 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PacketId(NonZeroU16);

impl Display for PacketId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'a> Decode<'a> for PacketId {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (pkid, rest) = read_u16(bytes)?;

        let pkid = PacketId::try_from(pkid)?;

        Ok((pkid, rest))
    }
}

impl Encode for PacketId {
    fn encode_len(&self) -> usize {
        mem::size_of::<u16>()
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        writer.write_u16(self.0.get()).map_err(EncodeError::Write)
    }
}

impl Deref for PacketId {
    type Target = NonZeroU16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<usize> for PacketId {
    type Error = PacketIdError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        u16::try_from(value)
            .map_err(|_| PacketIdError)
            .and_then(PacketId::try_from)
    }
}

impl TryFrom<u16> for PacketId {
    type Error = PacketIdError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        NonZeroU16::new(value).ok_or(PacketIdError).map(PacketId)
    }
}

impl From<PacketId> for usize {
    fn from(value: PacketId) -> Self {
        value.get().into()
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
        let bytes = &[0b0001_0000, 0x80];

        let err = FixedHeader::parse(bytes).unwrap_err();

        let DecodeError::NotEnoughBytes { needed } = err else {
            panic!("wrong error {err:?}");
        };

        assert_eq!(needed, 1);
    }
}
