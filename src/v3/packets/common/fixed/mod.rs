//! Fixed header for the packets.

use core::fmt::Display;

use bitflags::bitflags;
use zerocopy::*;

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};

use self::builder::FixedHeaderBuilder;
use self::remaining_length::{RemainingLength, RemainingLengthArray, RemainingLengthSlice};

pub(crate) mod builder;
pub mod remaining_length;

pub(crate) type FixedHeaderSlice = FixedHeader<[u8]>;
pub(crate) type FixedHeaderArray<const N: usize> = FixedHeader<RemainingLength<[u8; N]>>;

/// Fixed header containing the packet type, flags and remaining length of the payload.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718020>
#[derive(Debug, Clone, Copy, KnownLayout, Immutable, FromBytes, IntoBytes, ByteEq, SplitAt)]
#[repr(C)]
pub struct FixedHeader<R: ?Sized> {
    type_and_flags: u8,
    remaining_length: R,
}

impl<B: ?Sized> FixedHeader<B> {
    /// Returns the control packet type
    pub fn packet_type(&self) -> ControlPacketType {
        ControlPacketType::try_from(self.type_and_flags >> 4)
            .expect("the fixed header must must be valid when constructed or parsed")
    }

    /// Returns the control packet flags
    pub fn flags(&self) -> TypeFlags {
        TypeFlags::from_bits_retain(self.type_and_flags & 0x0f)
    }

    fn validate_type_flags(pk_type: ControlPacketType, flags: TypeFlags) -> Result<(), Error> {
        match pk_type {
            ControlPacketType::Publish => {}
            ControlPacketType::PubRel => {
                if flags != TypeFlags::PUBREL {
                    return Err(Error::new(ErrorKind::Invalid, "pubrel flags"));
                }
            }
            ControlPacketType::Subscribe => {
                if flags != TypeFlags::SUBSCRIBE {
                    return Err(Error::new(ErrorKind::Invalid, "subscribe flags"));
                }
            }
            ControlPacketType::Unsubscribe => {
                if flags != TypeFlags::UNSUBSCRIBE {
                    return Err(Error::new(ErrorKind::Invalid, "unsubscribe flags"));
                }
            }
            _ => {
                if !flags.is_empty() {
                    return Err(Error::new(ErrorKind::Invalid, "non empty flags"));
                }
            }
        }

        Ok(())
    }

    fn encode_type_and_flags(pk_type: ControlPacketType, flags: TypeFlags) -> u8 {
        (u8::from(pk_type) << 4) | flags.bits()
    }

    /// Reads the remaining length for the packet
    pub fn next_frame(buff: &[u8]) -> Result<Option<usize>, Error> {
        let Some(buff) = buff.get(1..) else {
            return Ok(None);
        };

        let rem_bytes = 1 + RemainingLengthSlice::count_continue_flags(buff)?;

        let Some(buff) = buff.get(..rem_bytes) else {
            return Ok(None);
        };

        let rem_len = usize::try_from(RemainingLengthSlice::read_from_buf(buff))
            .map_err(|_err| Error::new(ErrorKind::OutOfRange, "remaining length as usize"))?;

        // Next frame len
        // 1            | fixed_header                  |
        // rem_bytes    | remaining_length              |
        // rem_len      | variable header and payload   |
        (1 + rem_bytes)
            .checked_add(rem_len)
            .map(Some)
            .ok_or(Error::new(ErrorKind::Overflow, "next frame"))
    }
}

impl FixedHeader<[u8]> {
    /// Returns the control packet remaining length
    pub fn remaining_length(&self) -> &RemainingLength<[u8]> {
        transmute_ref!(&self.remaining_length)
    }
}

impl<const N: usize> FixedHeaderArray<N> {
    /// Returns the control packet remaining length
    pub fn remaining_length(&self) -> &RemainingLength<[u8; N]> {
        &self.remaining_length
    }
}

impl Display for FixedHeaderSlice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "FixedHeader[{}, {}, {}]",
            self.packet_type(),
            self.flags(),
            self.remaining_length()
        )
    }
}

impl<const N: usize> Display for FixedHeaderArray<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "FixedHeader[{}, {}, {}]",
            self.packet_type(),
            self.flags(),
            self.remaining_length()
        )
    }
}

impl Input<FixedHeaderBuilder> for FixedHeaderSlice {
    type Validated = ();

    fn validate_value(value: &FixedHeaderBuilder) -> Result<(), Error> {
        Self::validate_type_flags(value.packet_type, value.flags)?;
        RemainingLengthSlice::validate_value(&value.remaining_length)?;

        Ok(())
    }
}

impl<const N: usize> Input<FixedHeaderBuilder> for FixedHeaderArray<N> {
    type Validated = Self;

    fn validate_value(value: &FixedHeaderBuilder) -> Result<Self, Error> {
        Self::validate_type_flags(value.packet_type, value.flags)?;
        RemainingLengthArray::<N>::validate_value(&value.remaining_length)?;

        Ok(Self {
            type_and_flags: Self::encode_type_and_flags(value.packet_type, value.flags),
            remaining_length: RemainingLengthArray {
                len: RemainingLengthArray::<N>::encode_to_array(value.remaining_length),
            },
        })
    }
}

impl Parsed for FixedHeaderSlice {
    fn validate(&self) -> Result<(), Error> {
        ControlPacketType::parse(self.type_and_flags)?;
        self.remaining_length().validate()?;

        Ok(())
    }
}

impl<const N: usize> Parsed for FixedHeaderArray<N> {
    fn validate(&self) -> Result<(), Error> {
        ControlPacketType::parse(self.type_and_flags)?;
        self.remaining_length().validate()?;

        Ok(())
    }
}

impl Encode<FixedHeaderBuilder> for FixedHeaderSlice {
    fn encode_len(value: &FixedHeaderBuilder) -> Result<usize, Error> {
        RemainingLengthSlice::encode_len(&value.remaining_length).and_then(|size| {
            size.checked_add(1)
                .ok_or(Error::new(ErrorKind::Overflow, "fixed header encode len"))
        })
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &FixedHeaderBuilder) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        Self::validate_value(value)?;

        let to_write = Self::encode_len(value)?;

        let type_and_flags = Self::encode_type_and_flags(value.packet_type, value.flags);

        writer
            .write_all(core::slice::from_ref(&type_and_flags))
            .map_err(|error| {
                Error::new(
                    ErrorKind::StdIo(error.kind()),
                    "writing fixed header type and flags",
                )
            })?;
        RemainingLengthSlice::write_sync(writer, &value.remaining_length)?;

        Ok(to_write)
    }
}

impl<const N: usize> Encode<FixedHeaderBuilder> for FixedHeaderArray<N> {
    fn encode_len(_value: &FixedHeaderBuilder) -> Result<usize, Error> {
        Ok(size_of::<Self>())
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &FixedHeaderBuilder) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        let this = Self::validate_value(value)?;

        writer
            .write_all(this.as_bytes())
            .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "writing fixed header"))?;

        Ok(size_of::<Self>())
    }
}

impl<'a> Decode<'a> for FixedHeaderSlice {
    type Out = &'a Self;

    fn parse(buf: &[u8]) -> Result<(&Self, &[u8]), Error> {
        // Ignore the rest since the rest will be split off the remaining length
        let (this, _) = Self::ref_from_prefix(buf)
            .map_err(|_| Error::new(ErrorKind::NotEnoughSpace, "fixed header"))?;

        let (_pk_type, _flags) = ControlPacketType::parse(this.type_and_flags)?;

        let (remaining_length, rest) = RemainingLengthSlice::parse(&this.remaining_length)?;

        let bytes = remaining_length.as_bytes().len();
        // Should never error
        let (this, _) = this
            .split_at(bytes)
            .ok_or(Error::new(
                ErrorKind::NotEnoughSpace,
                "fixed header split after remaining length",
            ))?
            .via_immutable();

        Ok((this, rest))
    }
}

impl<'a, const N: usize> Decode<'a> for FixedHeaderArray<N> {
    type Out = &'a Self;

    fn parse(buf: &[u8]) -> Result<(&Self, &[u8]), Error> {
        let (this, rest) = Self::ref_from_prefix(buf)
            .map_err(|_| Error::new(ErrorKind::NotEnoughSpace, "fixed header"))?;

        this.validate()?;

        Ok((this, rest))
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
    /// Validates the ControlPacketType and TypeFlags.
    pub(crate) fn parse(type_and_flags: u8) -> Result<(Self, TypeFlags), Error> {
        let pk_type = Self::try_from(type_and_flags >> 4)?;

        let flags = TypeFlags::from_bits(type_and_flags & 0b0000_1111)
            .ok_or(Error::new(ErrorKind::Invalid, "control packet flags"))?;

        FixedHeaderSlice::validate_type_flags(pk_type, flags)?;

        Ok((pk_type, flags))
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
    type Error = Error;

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
            _ => return Err(Error::new(ErrorKind::Invalid, "control packet type")),
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
        /// Publish is duplicate.
        const PUBLISH_DUP = 0b0000_1000;
        /// Publish QoS first bit.
        const PUBLISH_QOS_1 = 0b0000_0010;
        /// Publish QoS second bit.
        const PUBLISH_QOS_2 = 0b0000_0100;
        /// Publish retain flag.
        const PUBLISH_RETAIN = 0b0000_0001;

        /// Reserved non zero flags for PUBREL
        const PUBREL = 0b0000_0010;
        /// Reserved non zero flags for SUBSCRIBE
        const SUBSCRIBE = 0b0000_0010;
        /// Reserved non zero flags for UNSUBSCRIBE
        const UNSUBSCRIBE = 0b0000_0010;
    }
}

impl Display for TypeFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:04b}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(0, &[0b0001_0000, 0x00])]
    #[case(127, &[0b0001_0000, 0x7F])]
    #[case(128, &[0b0001_0000, 0x80, 0x01])]
    #[case(16_383, &[0b0001_0000, 0xFF, 0x7F])]
    #[case(16_384, &[0b0001_0000, 0x80, 0x80, 0x01])]
    #[case(2_097_151, &[0b0001_0000, 0xFF, 0xFF, 0x7F])]
    #[case(2_097_152, &[0b0001_0000, 0x80, 0x80, 0x80, 0x01])]
    #[case(268_435_455, &[0b0001_0000, 0xFF, 0xFF, 0xFF, 0x7F])]
    fn should_parse_fixed_headers_and_remaining_length(#[case] exp: u32, #[case] bytes: &[u8]) {
        let (fixed, rest) =
            FixedHeaderSlice::parse(bytes).unwrap_or_else(|err| panic!("failed {exp}: {err}"));

        assert!(rest.is_empty());
        assert_eq!(fixed.remaining_length().read(), exp);
    }

    #[rstest]
    #[case(ControlPacketType::Connect, TypeFlags::empty(), 0b0001_0000u8)]
    #[case(ControlPacketType::ConnAck, TypeFlags::empty(), 0b0010_0000)]
    #[case(ControlPacketType::Publish, TypeFlags::PUBLISH_DUP | TypeFlags::PUBLISH_QOS_2 | TypeFlags::PUBLISH_RETAIN, 0b0011_1101)]
    #[case(ControlPacketType::PubAck, TypeFlags::empty(), 0b0100_0000)]
    #[case(ControlPacketType::PubRec, TypeFlags::empty(), 0b0101_0000)]
    #[case(ControlPacketType::PubRel, TypeFlags::PUBREL, 0b0110_0010)]
    #[case(ControlPacketType::PubComp, TypeFlags::empty(), 0b0111_0000)]
    #[case(ControlPacketType::Subscribe, TypeFlags::SUBSCRIBE, 0b1000_0010)]
    #[case(ControlPacketType::SubAck, TypeFlags::empty(), 0b1001_0000)]
    #[case(ControlPacketType::Unsubscribe, TypeFlags::UNSUBSCRIBE, 0b1010_0010)]
    #[case(ControlPacketType::UnsubAck, TypeFlags::empty(), 0b1011_0000)]
    #[case(ControlPacketType::PingReq, TypeFlags::empty(), 0b1100_0000)]
    #[case(ControlPacketType::PingResp, TypeFlags::empty(), 0b1101_0000)]
    #[case(ControlPacketType::Disconnect, TypeFlags::empty(), 0b1110_0000)]
    fn should_parse_packet_type_and_flag(
        #[case] exp_packet: ControlPacketType,
        #[case] exp_type: TypeFlags,
        #[case] case: u8,
    ) {
        let exp = (exp_packet, exp_type);

        let res = ControlPacketType::parse(case).unwrap_or_else(|_| {
            panic!(
                "couldn't parse {} with flags {} from {case:08b}",
                exp.0, exp.1
            )
        });

        assert_eq!(res, exp);
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
    fn should_require_more_bytes_for_fixed_header() {
        let bytes = &[0b0001_0000, 0x80];

        let err = FixedHeaderSlice::parse(bytes).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::NotEnoughSpace);
    }
}
