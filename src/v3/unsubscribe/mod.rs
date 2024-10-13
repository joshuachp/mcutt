//! Handle unsubscribing to topics to the UNSUBSCRIBE packet.

use core::{fmt::Display, ops::Deref};

use iter::Iter;

use super::{
    header::{ControlPacketType, PacketId, Str, StrRef, TypeFlags},
    CursorIter, Decode, DecodeCursor, DecodeError, DecodePacket, Encode, EncodeError, EncodePacket,
};

#[cfg(feature = "alloc")]
pub mod alloc;
pub mod iter;

/// Reference to an [`Unsubscribe`]  packet.
pub type UnsubscribeRef<'a> = Unsubscribe<UnsubscribeCursor<'a>>;

/// An UNSUBSCRIBE Packet is sent by the Client to the Server, to unsubscribe from topics.
///
/// The payload for the UNSUBSCRIBE Packet contains the list of Topic Filters that the Client wishes
/// to unsubscribe from. The Topic Filters in an UNSUBSCRIBE packet MUST be UTF-8 encoded strings as
/// packed contiguously. The Payload of an UNSUBSCRIBE packet MUST contain at least one Topic
/// Filter. An UNSUBSCRIBE packet with no payload is a protocol violation.
#[derive(Debug, Clone, Copy)]
pub struct Unsubscribe<I> {
    pkid: PacketId,
    filters: I,
}

impl<I> Unsubscribe<I> {
    /// Returns the packet identifier of the unsubscribe.
    pub fn pkid(&self) -> PacketId {
        self.pkid
    }
}

impl<'a, I, S> IntoIterator for &'a Unsubscribe<I>
where
    &'a I: IntoIterator<Item = &'a UnsubscribeTopic<S>>,
    S: Deref<Target = str> + 'a,
{
    type Item = UnsubscribeTopic<&'a str>;

    type IntoIter = Iter<<&'a I as IntoIterator>::IntoIter>;

    fn into_iter(self) -> Self::IntoIter {
        Iter::new(&self.filters)
    }
}

impl<I> EncodePacket for Unsubscribe<I>
where
    for<'a> &'a Self: IntoIterator<Item = UnsubscribeTopic<&'a str>>,
{
    fn remaining_len(&self) -> usize {
        let filters = self
            .into_iter()
            .map(|i| i.encode_len())
            .fold(0usize, |acc, len| acc.saturating_add(len));

        self.pkid.encode_len().saturating_add(filters)
    }

    fn packet_type() -> ControlPacketType {
        ControlPacketType::Unsubscribe
    }

    fn packet_flags(&self) -> TypeFlags {
        TypeFlags::UNSUBSCRIBE
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

impl<'a> DecodePacket<'a> for Unsubscribe<UnsubscribeCursor<'a>> {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::Unsubscribe
    }

    fn parse_with_header(
        header: super::header::FixedHeader,
        bytes: &'a [u8],
    ) -> Result<Self, super::DecodeError> {
        if header.flags() != TypeFlags::UNSUBSCRIBE {
            return Err(DecodeError::Reserved);
        }

        let (pkid, mut bytes) = PacketId::parse(bytes)?;

        if bytes.is_empty() {
            return Err(DecodeError::EmptyTopics);
        }

        let filters = UnsubscribeCursor { bytes };

        // Check the remaining bytes are a valid packet filter
        while !bytes.is_empty() {
            let (_, rest) = UnsubscribeTopic::parse(bytes)?;

            bytes = rest;
        }

        Ok(Unsubscribe { pkid, filters })
    }
}

impl<I, T> Display for Unsubscribe<I>
where
    for<'a> &'a I: IntoIterator<Item = T>,
    T: Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} pkid({}) (",
            ControlPacketType::Unsubscribe,
            self.pkid
        )?;

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

impl<I1, I2> PartialEq<Unsubscribe<I2>> for Unsubscribe<I1>
where
    I1: PartialEq<I2>,
{
    fn eq(&self, other: &Unsubscribe<I2>) -> bool {
        self.pkid == other.pkid && self.filters == other.filters
    }
}

/// Topic to unsubscribe from.
// TODO: add a checked conversion from string
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsubscribeTopic<S> {
    // Topic or topic filter.
    topic: Str<S>,
}

impl<S> Encode for UnsubscribeTopic<S>
where
    S: Deref<Target = str>,
{
    fn encode_len(&self) -> usize {
        self.topic.encode_len()
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        self.topic.write(writer)
    }
}

impl<'a> Decode<'a> for UnsubscribeTopic<&'a str> {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), super::DecodeError> {
        let (topic, bytes) = Str::parse(bytes)?;

        Ok((Self { topic }, bytes))
    }
}

impl<S> Display for UnsubscribeTopic<S>
where
    S: Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "topic({})", self.topic)
    }
}

impl<'a, S> From<&'a UnsubscribeTopic<S>> for UnsubscribeTopic<&'a str>
where
    S: Deref<Target = str>,
{
    fn from(value: &'a UnsubscribeTopic<S>) -> Self {
        Self {
            topic: StrRef::from(&value.topic),
        }
    }
}

/// Cursor to iterator over the [`UnsubscribeTopic`] encoded in a buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsubscribeCursor<'a> {
    bytes: &'a [u8],
}

impl<'a> Deref for UnsubscribeCursor<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.bytes
    }
}

impl<'a> DecodeCursor<'a> for UnsubscribeCursor<'a> {
    type Item = UnsubscribeTopic<&'a str>;
}

impl<'a> IntoIterator for &'a UnsubscribeCursor<'_> {
    type Item = UnsubscribeTopic<&'a str>;

    type IntoIter = CursorIter<'a, UnsubscribeCursor<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        CursorIter::new(self)
    }
}

impl<'a, const N: usize, S> PartialEq<[UnsubscribeTopic<S>; N]> for UnsubscribeCursor<'a>
where
    S: Deref<Target = str>,
{
    fn eq(&self, other: &[UnsubscribeTopic<S>; N]) -> bool {
        self.into_iter().eq(other.iter().map(|s| s.into()))
    }
}

impl<'a, S> PartialEq<[UnsubscribeTopic<S>]> for UnsubscribeCursor<'a>
where
    S: Deref<Target = str>,
{
    fn eq(&self, other: &[UnsubscribeTopic<S>]) -> bool {
        self.into_iter().eq(other.iter().map(|s| s.into()))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::v3::tests::TestWriter;

    use super::*;

    #[test]
    fn should_encode_and_decode_unsubscribe() {
        let mut writer = TestWriter::new();

        let packet = Unsubscribe {
            pkid: PacketId::try_from(10u16).unwrap(),
            filters: [
                UnsubscribeTopic {
                    topic: Str::try_from("a/b").unwrap(),
                },
                UnsubscribeTopic {
                    topic: Str::try_from("c/d").unwrap(),
                },
            ],
        };

        packet.write(&mut writer).unwrap();

        let exp_bytes = [
            // fixed header
            0b1010_0010u8,
            12,
            // pkid
            0b00000000,
            0b00001010,
            // filters
            0b00000000,
            0b00000011,
            b'a',
            b'/',
            b'b',
            0b00000000,
            0b00000011,
            b'c',
            b'/',
            b'd',
        ];

        assert_eq!(writer.buf, exp_bytes);

        let (sub, bytes) = <UnsubscribeRef as DecodePacket>::parse(&writer.buf).unwrap();

        assert!(bytes.is_empty());

        assert_eq!(sub, packet);
    }
}
