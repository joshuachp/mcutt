//! Handle subscribing to topics through the SUBSCRIBE packet.

use core::{
    fmt::Display,
    ops::Deref,
    slice::{self},
};

use iter::{Iter, SubAckCodeIter};

use crate::v3::{bytes::read_u8, packets::header::ControlPacketType};

use super::{
    header::{PacketId, Str, StrRef, TypeFlags},
    CursorIter, Decode, DecodeCursor, DecodeError, DecodePacket, Encode, EncodeError, EncodePacket,
    Qos,
};

#[cfg(feature = "alloc")]
pub mod alloc;
pub mod iter;

/// Reference to a [`Subscribe`] packet.
pub type SubscribeRef<'a> = Subscribe<SubscribeCursor<'a>>;

/// The SUBSCRIBE Packet is sent from the Client to the Server to create one or more Subscriptions.
///
/// Each Subscription registers a Client’s interest in one or more Topics. The Server sends PUBLISH
/// Packets to the Client in order to forward Application Messages that were published to Topics
/// that match these Subscriptions. The SUBSCRIBE Packet also specifies (for each Subscription) the
/// maximum QoS with which the Server can send Application Messages to the Client.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718063>
#[derive(Debug, Clone, Copy)]
pub struct Subscribe<I> {
    pkid: PacketId,
    filters: I,
}

impl<I> Subscribe<I> {
    /// Return the packet identifier of the subscribe.
    pub fn pkid(&self) -> PacketId {
        self.pkid
    }
}

impl<'a, S, I> IntoIterator for &'a Subscribe<I>
where
    &'a I: IntoIterator<Item = &'a SubscribeTopic<S>>,
    S: Deref<Target = str> + 'a,
{
    type Item = SubscribeTopic<&'a str>;

    type IntoIter = Iter<<&'a I as IntoIterator>::IntoIter>;

    fn into_iter(self) -> Self::IntoIter {
        Iter::new(&self.filters)
    }
}

impl<I> EncodePacket for Subscribe<I>
where
    for<'a> &'a Self: IntoIterator<Item = SubscribeTopic<&'a str>>,
{
    fn remaining_len(&self) -> usize {
        let filters = self
            .into_iter()
            .map(|i| i.encode_len())
            .fold(0usize, |acc, len| acc.saturating_add(len));

        self.pkid.encode_len().saturating_add(filters)
    }

    fn packet_type() -> ControlPacketType {
        ControlPacketType::Subscribe
    }

    fn packet_flags(&self) -> TypeFlags {
        TypeFlags::SUBSCRIBE
    }

    fn write_packet<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let pkid = self.pkid.write(writer)?;

        self.into_iter()
            .map(|filter| filter.write(writer))
            .try_fold(pkid, |len, written| {
                written.map(|written| written.saturating_add(len))
            })
    }
}

impl<'a> DecodePacket<'a> for SubscribeRef<'a> {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::Subscribe
    }

    fn parse_with_header(
        header: super::header::FixedHeader,
        bytes: &'a [u8],
    ) -> Result<Self, super::DecodeError> {
        if header.flags() != TypeFlags::SUBSCRIBE {
            return Err(DecodeError::Reserved);
        }

        let (pkid, mut bytes) = PacketId::parse(bytes)?;

        if bytes.is_empty() {
            return Err(DecodeError::EmptyTopics);
        }

        let filters = SubscribeCursor { bytes };

        // Check the remaining bytes are a valid packet filter
        while !bytes.is_empty() {
            let (_, rest) = SubscribeTopic::parse(bytes)?;

            bytes = rest;
        }

        Ok(Subscribe { pkid, filters })
    }
}

impl<I, T> Display for Subscribe<I>
where
    for<'a> &'a I: IntoIterator<Item = T>,
    T: Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} pkid({}) (", ControlPacketType::Subscribe, self.pkid)?;

        let mut iter = self.filters.into_iter();

        if let Some(i) = iter.next() {
            write!(f, "{i}")?;

            for i in iter {
                write!(f, ", {i}")?;
            }
        }

        write!(f, ")")
    }
}

impl<I1, I2> PartialEq<Subscribe<I2>> for Subscribe<I1>
where
    I1: PartialEq<I2>,
{
    fn eq(&self, other: &Subscribe<I2>) -> bool {
        self.pkid == other.pkid && self.filters == other.filters
    }
}

/// Indicates a Topic the Clint wants to subscribe.
///
/// The server may or may not support wild card characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscribeTopic<S> {
    /// The Topic to subscribe.
    topic: Str<S>,
    /// Requested QoS for the topic.
    ///
    /// This gives the maximum QoS level at which the Server can send Application Messages to the Client.
    qos: Qos,
}

impl<S> Encode for SubscribeTopic<S>
where
    S: Deref<Target = str>,
{
    fn encode_len(&self) -> usize {
        // String len, plus a byte for the QOS with the upper 6 bits unused
        self.topic.encode_len().saturating_add(1)
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let filter = self.topic.write(writer)?;
        let qos = writer
            .write_u8(self.qos.into())
            .map_err(EncodeError::Write)?;

        Ok(filter.saturating_add(qos))
    }
}

impl<'a> Decode<'a> for SubscribeTopic<&'a str> {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), super::DecodeError> {
        let (topic, bytes) = Str::parse(bytes)?;

        let (qos, bytes) = read_u8(bytes)?;

        let qos = Qos::try_from(qos).map_err(|_| DecodeError::Reserved)?;

        Ok((SubscribeTopic { topic, qos }, bytes))
    }
}

impl<S> Display for SubscribeTopic<S>
where
    S: Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "topic({}) {}", self.topic, self.qos)
    }
}

impl<'a, S> From<&'a SubscribeTopic<S>> for SubscribeTopic<&'a str>
where
    S: Deref<Target = str>,
{
    fn from(value: &'a SubscribeTopic<S>) -> Self {
        Self {
            topic: StrRef::from(&value.topic),
            qos: value.qos,
        }
    }
}

/// Cursor to iterator over the [`SubscribeTopic`] encoded in a buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscribeCursor<'a> {
    bytes: &'a [u8],
}

impl Deref for SubscribeCursor<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.bytes
    }
}

impl<'a> DecodeCursor<'a> for SubscribeCursor<'a> {
    type Item = SubscribeTopic<&'a str>;
}

impl<'a> IntoIterator for &'a SubscribeCursor<'_> {
    type Item = SubscribeTopic<&'a str>;

    type IntoIter = CursorIter<'a, SubscribeCursor<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        CursorIter::new(self)
    }
}

impl<const N: usize, S> PartialEq<[SubscribeTopic<S>; N]> for SubscribeCursor<'_>
where
    S: Deref<Target = str>,
{
    fn eq(&self, other: &[SubscribeTopic<S>; N]) -> bool {
        self.into_iter().eq(other.iter().map(|s| s.into()))
    }
}

impl<S> PartialEq<[SubscribeTopic<S>]> for SubscribeCursor<'_>
where
    S: Deref<Target = str>,
{
    fn eq(&self, other: &[SubscribeTopic<S>]) -> bool {
        self.into_iter().eq(other.iter().map(|s| s.into()))
    }
}

/// Reference to a [`SubAck`] packet.
pub type SubAckRef<'a> = SubAck<SubAckCodeCursor<'a>>;

/// A SUBACK Packet is sent by the Server to the Client to confirm receipt and processing of a
/// SUBSCRIBE Packet.
///
/// A SUBACK Packet contains a list of return codes, that specify the maximum QoS level that was
/// granted in each Subscription that was requested by the SUBSCRIBE.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718068>
#[derive(Debug, Clone, Copy)]
pub struct SubAck<I> {
    pkid: PacketId,
    return_codes: I,
}

impl<I> SubAck<I> {
    /// Returns the [`PacketId`] of the packet.
    pub fn pkid(&self) -> PacketId {
        self.pkid
    }
}

impl<'a, const N: usize> IntoIterator for &'a SubAck<[SubAckCode; N]> {
    type Item = SubAckCode;

    type IntoIter = core::iter::Copied<slice::Iter<'a, SubAckCode>>;

    fn into_iter(self) -> Self::IntoIter {
        self.return_codes.iter().copied()
    }
}

impl<'a> IntoIterator for &'a SubAck<&'a [SubAckCode]> {
    type Item = SubAckCode;

    type IntoIter = core::iter::Copied<slice::Iter<'a, SubAckCode>>;

    fn into_iter(self) -> Self::IntoIter {
        self.return_codes.iter().copied()
    }
}

impl<I> EncodePacket for SubAck<I>
where
    for<'a> &'a Self: IntoIterator<Item = SubAckCode>,
{
    fn remaining_len(&self) -> usize {
        let return_codes = self
            .into_iter()
            .map(|code| code.encode_len())
            .fold(0usize, |acc, size| acc.saturating_add(size));

        self.pkid.encode_len().saturating_add(return_codes)
    }

    fn packet_type() -> ControlPacketType {
        ControlPacketType::SubAck
    }

    fn packet_flags(&self) -> TypeFlags {
        TypeFlags::empty()
    }

    fn write_packet<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let pkid = self.pkid.write(writer)?;

        self.into_iter()
            .map(|filter| filter.write(writer))
            .try_fold(pkid, |len, written| {
                written.map(|written| written.saturating_add(len))
            })
    }
}

impl<'a> DecodePacket<'a> for SubAck<SubAckCodeCursor<'a>> {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::SubAck
    }

    fn parse_with_header(
        header: super::header::FixedHeader,
        bytes: &'a [u8],
    ) -> Result<Self, super::DecodeError> {
        if header.flags() != TypeFlags::empty() {
            return Err(DecodeError::Reserved);
        }

        let (pkid, mut bytes) = PacketId::parse(bytes)?;

        if bytes.is_empty() {
            return Err(DecodeError::EmptyTopics);
        }

        let return_codes = SubAckCodeCursor { bytes };

        // Check the remaining bytes are a valid packet filter
        while !bytes.is_empty() {
            let (_, rest) = SubAckCode::parse(bytes)?;

            bytes = rest;
        }

        Ok(SubAck { pkid, return_codes })
    }
}

impl<I, T> Display for SubAck<I>
where
    for<'a> &'a I: IntoIterator<Item = T>,
    T: Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} pkid({}) (", ControlPacketType::SubAck, self.pkid)?;

        let mut iter = self.return_codes.into_iter();

        if let Some(i) = iter.next() {
            write!(f, "{i}")?;

            for i in iter {
                write!(f, ", {i}")?;
            }
        }

        write!(f, ")")
    }
}

impl<I1, I2> PartialEq<SubAck<I2>> for SubAck<I1>
where
    I1: PartialEq<I2>,
{
    fn eq(&self, other: &SubAck<I2>) -> bool {
        self.pkid == other.pkid && self.return_codes == other.return_codes
    }
}

/// Return code for a SUBACK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SubAckCode {
    /// Success with a maximum QoS of 0.
    Qos0 = 0b0000_0000,
    /// Success with a maximum QoS of 1.
    Qos1 = 0b0000_0001,
    /// Success with a maximum QoS of 2.
    Qos2 = 0b0000_0010,
    /// Failure to subscribe.
    Failure = 0b1000_0000,
}

impl Display for SubAckCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            SubAckCode::Qos0 => "qos0",
            SubAckCode::Qos1 => "qos1",
            SubAckCode::Qos2 => "qos2",
            SubAckCode::Failure => "failure",
        };

        write!(f, "{s}")
    }
}

impl From<SubAckCode> for u8 {
    fn from(value: SubAckCode) -> Self {
        value as u8
    }
}

impl TryFrom<u8> for SubAckCode {
    type Error = DecodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b0000_0000 => Ok(SubAckCode::Qos0),
            0b0000_0001 => Ok(SubAckCode::Qos1),
            0b0000_0010 => Ok(SubAckCode::Qos2),
            0b1000_0000 => Ok(SubAckCode::Failure),
            _ => Err(DecodeError::Reserved),
        }
    }
}

impl Encode for SubAckCode {
    fn encode_len(&self) -> usize {
        1
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        writer.write_u8((*self).into()).map_err(EncodeError::Write)
    }
}

impl<'a> Decode<'a> for SubAckCode {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (code, bytes) = read_u8(bytes)?;

        let code = SubAckCode::try_from(code)?;

        Ok((code, bytes))
    }
}

/// Cursor to iterator over the [`SubAckCode`] encoded in a buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubAckCodeCursor<'a> {
    bytes: &'a [u8],
}

impl<'a> IntoIterator for &'a SubAckCodeCursor<'_> {
    type Item = SubAckCode;

    type IntoIter = SubAckCodeIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        SubAckCodeIter::new(self)
    }
}

impl<const N: usize> PartialEq<[SubAckCode; N]> for SubAckCodeCursor<'_> {
    fn eq(&self, other: &[SubAckCode; N]) -> bool {
        self.into_iter().eq(other.iter().copied())
    }
}

impl PartialEq<[SubAckCode]> for SubAckCodeCursor<'_> {
    fn eq(&self, other: &[SubAckCode]) -> bool {
        self.into_iter().eq(other.iter().copied())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::v3::packets::tests::TestWriter;

    use super::*;

    #[test]
    fn should_encode_and_decode_subscribe() {
        let mut writer = TestWriter::new();

        let packet = Subscribe {
            pkid: PacketId::try_from(10u16).unwrap(),
            filters: [
                SubscribeTopic {
                    topic: Str::try_from("a/b").unwrap(),
                    qos: Qos::AtLeastOnce,
                },
                SubscribeTopic {
                    topic: Str::try_from("c/d").unwrap(),
                    qos: Qos::ExactlyOnce,
                },
            ],
        };

        packet.write(&mut writer).unwrap();

        let exp_bytes = [
            // fixed header
            0b10000010u8,
            14,
            // pkid
            0b00000000,
            0b00001010,
            // filters
            0b00000000,
            0b00000011,
            b'a',
            b'/',
            b'b',
            1,
            0b00000000,
            0b00000011,
            b'c',
            b'/',
            b'd',
            2,
        ];

        assert_eq!(writer.buf, exp_bytes);

        let (sub, bytes) = <SubscribeRef as DecodePacket>::parse(&writer.buf).unwrap();

        assert!(bytes.is_empty());

        assert_eq!(sub, packet);
    }

    #[test]
    fn should_iter_return_code_cursor() {
        let cursor = SubAckCodeCursor {
            bytes: &[0b0000_0000, 0b0000_0001, 0b0000_0010, 0b1000_0000],
        };

        let expected = [
            SubAckCode::Qos0,
            SubAckCode::Qos1,
            SubAckCode::Qos2,
            SubAckCode::Failure,
        ];

        let mut iter = cursor.into_iter();

        for case in expected {
            assert_eq!(iter.next().unwrap(), case);
        }

        assert!(iter.next().is_none());
    }

    #[test]
    fn should_encode_and_decode_suback() {
        let mut writer = TestWriter::new();

        let packet = SubAck {
            pkid: PacketId::try_from(10u16).unwrap(),
            return_codes: [
                SubAckCode::Qos0,
                SubAckCode::Qos1,
                SubAckCode::Qos2,
                SubAckCode::Failure,
            ],
        };

        packet.write(&mut writer).unwrap();

        let exp_bytes = [
            // fixed header
            0b1001_0000u8,
            6,
            // pkid
            0b0000_0000,
            0b0000_1010,
            // codes
            0b0000_0000,
            0b0000_0001,
            0b0000_0010,
            0b1000_0000,
        ];

        assert_eq!(writer.buf, exp_bytes);

        let (sub, bytes): (SubAck<SubAckCodeCursor>, _) = DecodePacket::parse(&writer.buf).unwrap();

        assert!(bytes.is_empty());

        assert_eq!(sub, packet);
    }
}
