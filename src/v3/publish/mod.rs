//! Packets related publish operations.

use core::{fmt::Display, ops::Deref};

use memchr::memchr2;

use super::{
    header::{ControlPacketType, FixedHeader, PacketId, RemainingLength, Str, StrError, TypeFlags},
    Decode, DecodeError, DecodePacket, Encode, EncodeError, EncodePacket,
};

#[cfg(feature = "alloc")]
pub use self::alloc::{ClientPublishOwned, PublishOwned, PublishTopicOwned};

#[cfg(feature = "alloc")]
mod alloc;

/// [`Publish`] with borrowed data
pub type PublishRef<'a> = Publish<&'a str, &'a [u8]>;

/// A PUBLISH Control Packet is sent from a Client to a Server or from Server to a Client to transport an Application Message.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718037>
#[derive(Debug, Clone, Copy)]
pub struct Publish<S, B> {
    qos: PublishQos,
    retain: bool,
    topic: PublishTopic<S>,
    payload: B,
}

impl<S, B> Publish<S, B> {
    /// Create a publish from the client one with the missing data.
    pub fn with_qos(pkid: PacketId, publish: ClientPublish<S, B>, qos: ClientQos) -> Self {
        let mut publish = Publish::from(publish);

        publish.qos = PublishQos::from_qos(pkid, qos);

        publish
    }

    /// Returns the packet identifier if the QoS 1 or 2.
    pub fn pkid(&self) -> Option<PacketId> {
        self.qos.pkid()
    }

    /// Returns the publish QoS.
    pub fn qos(&self) -> &PublishQos {
        &self.qos
    }
}

impl<S, B> Display for Publish<S, B>
where
    S: Display,
    B: Deref<Target = [u8]>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} topic({}) {} retain({}) payload({} bytes)",
            ControlPacketType::Publish,
            self.topic,
            self.qos,
            self.retain,
            self.payload.len()
        )
    }
}

impl<S1, S2, B1, B2> PartialEq<Publish<S2, B2>> for Publish<S1, B1>
where
    S1: PartialEq<S2>,
    B1: PartialEq<B2>,
{
    fn eq(&self, other: &Publish<S2, B2>) -> bool {
        self.qos.eq(&other.qos)
            && self.retain.eq(&other.retain)
            && self.topic.eq(&other.topic)
            && self.payload.eq(&other.payload)
    }
}

impl<S, B> From<ClientPublish<S, B>> for Publish<S, B> {
    fn from(value: ClientPublish<S, B>) -> Self {
        Self {
            qos: PublishQos::AtMostOnce,
            retain: value.retain,
            topic: value.topic,
            payload: value.payload,
        }
    }
}

impl<S, B> EncodePacket for Publish<S, B>
where
    S: Deref<Target = str>,
    B: AsRef<[u8]>,
{
    fn remaining_len(&self) -> usize {
        let mut len = self.topic.encode_len();

        if let Some(pkid) = self.qos.pkid() {
            len = len.saturating_add(pkid.encode_len());
        }

        len.saturating_add(self.payload.as_ref().encode_len())
    }

    fn packet_type() -> ControlPacketType {
        ControlPacketType::Publish
    }

    fn packet_flags(&self) -> TypeFlags {
        let mut flags = if self.retain {
            TypeFlags::PUBLISH_RETAIN
        } else {
            TypeFlags::empty()
        };

        let dup = match self.qos {
            PublishQos::AtMostOnce => false,
            PublishQos::AtLeastOnce { dup, .. } => {
                flags |= TypeFlags::PUBLISH_QOS_1;

                dup
            }
            PublishQos::ExactlyOnce { dup, .. } => {
                flags |= TypeFlags::PUBLISH_QOS_2;

                dup
            }
        };

        if dup {
            flags |= TypeFlags::PUBLISH_DUP;
        }

        flags
    }

    fn write_packet<W>(&self, writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        let mut variable = self.topic.write(writer)?;

        if let Some(pkid) = self.qos.pkid() {
            variable = variable.saturating_add(pkid.write(writer)?);
        }

        Ok(variable.saturating_add(self.payload.as_ref().write(writer)?))
    }
}

impl<'a> DecodePacket<'a> for PublishRef<'a> {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::Publish
    }

    fn parse_with_header(header: FixedHeader, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let (topic, bytes) = PublishTopic::parse(bytes)?;

        let (qos, payload) = PublishQos::parse_with_header(header, bytes)?;

        let retain = header.flags().contains(TypeFlags::PUBLISH_RETAIN);

        Ok(Self {
            qos,
            retain,
            topic,
            payload,
        })
    }
}

/// Level of assurance for delivery of an Application Message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum PublishQos {
    /// At most once delivery.
    AtMostOnce,
    /// At least once delivery.
    AtLeastOnce {
        /// Flag to indicate if the packet is a duplicate.
        dup: bool,
        /// Packet identifier.
        pkid: PacketId,
    },
    /// Exactly once delivery.
    ExactlyOnce {
        /// Flag to indicate if the packet is a duplicate.
        dup: bool,
        /// Packet identifier.
        pkid: PacketId,
    },
}

impl PublishQos {
    fn from_qos(pkid: PacketId, qos: ClientQos) -> Self {
        match qos {
            ClientQos::AtLeastOnce => PublishQos::AtLeastOnce { dup: false, pkid },
            ClientQos::ExactlyOnce => PublishQos::ExactlyOnce { dup: false, pkid },
        }
    }

    fn pkid(self) -> Option<PacketId> {
        match self {
            PublishQos::AtMostOnce => None,
            PublishQos::AtLeastOnce { pkid, .. } | PublishQos::ExactlyOnce { pkid, .. } => {
                Some(pkid)
            }
        }
    }

    fn parse_with_header(header: FixedHeader, bytes: &[u8]) -> Result<(Self, &[u8]), DecodeError> {
        const QOS_0: TypeFlags = TypeFlags::empty();

        let dup = header.flags().contains(TypeFlags::PUBLISH_DUP);

        let qos = header.flags() & TypeFlags::PUBLISH_QOS_MASK;

        match qos {
            QOS_0 => Ok((PublishQos::AtMostOnce, bytes)),
            TypeFlags::PUBLISH_QOS_1 => {
                let (pkid, bytes) = PacketId::parse(bytes)?;

                Ok((PublishQos::AtLeastOnce { dup, pkid }, bytes))
            }
            TypeFlags::PUBLISH_QOS_2 => {
                let (pkid, bytes) = PacketId::parse(bytes)?;

                Ok((PublishQos::ExactlyOnce { dup, pkid }, bytes))
            }
            _ => Err(DecodeError::Reserved),
        }
    }

    /// Returns `true` if the publish qos is [`AtMostOnce`].
    ///
    /// [`AtMostOnce`]: PublishQos::AtMostOnce
    #[must_use]
    pub fn is_qos0(&self) -> bool {
        matches!(self, Self::AtMostOnce)
    }

    /// Returns `true` if the publish qos is [`AtLeastOnce`].
    ///
    /// [`AtLeastOnce`]: PublishQos::AtLeastOnce
    #[must_use]
    pub fn is_qos1(&self) -> bool {
        matches!(self, Self::AtLeastOnce { .. })
    }

    /// Returns `true` if the publish qos is [`ExactlyOnce`].
    ///
    /// [`ExactlyOnce`]: PublishQos::ExactlyOnce
    #[must_use]
    pub fn is_qos2(&self) -> bool {
        matches!(self, Self::ExactlyOnce { .. })
    }
}

impl Display for PublishQos {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PublishQos::AtMostOnce => write!(f, "QoS 0"),
            PublishQos::AtLeastOnce { dup, pkid } => write!(f, "QoS(1) pkid({pkid}) dup({dup})"),
            PublishQos::ExactlyOnce { dup, pkid } => write!(f, "QoS(2) pkid({pkid}) dup({dup})"),
        }
    }
}

/// [`ClientPublish`] with borrowed data.
pub type ClientPublishRef<'a> = ClientPublish<&'a str, &'a [u8]>;

/// Client publish data.
///
/// This is a struct can be passed to the connection to publish data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientPublish<S, B> {
    retain: bool,
    topic: PublishTopic<S>,
    payload: B,
}

impl<S, B> ClientPublish<S, B> {
    /// Creates a new PUBLISH with the given topic and payload.
    pub fn new(topic: PublishTopic<S>, payload: B) -> Self {
        Self {
            retain: false,
            topic,
            payload,
        }
    }

    /// Sets the RETAIN flag for the PUBLISH.
    pub fn retain(&mut self) -> &mut Self {
        self.retain = true;

        self
    }
}

/// Quality of service of a publish packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientQos {
    /// At least once delivery (QoS 1).
    AtLeastOnce,
    /// Exactly once delivery (QoS 2).
    ExactlyOnce,
}

impl Display for ClientQos {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ClientQos::AtLeastOnce => write!(f, "at least once delivery (1)"),
            ClientQos::ExactlyOnce => write!(f, "exactly once delivery (2)"),
        }
    }
}

/// Error for the publish [`PublishTopic`]
#[derive(Debug)]
pub enum TopicError {
    /// Invalid UTF-8 string for the topic.
    Str(StrError),
    /// The topic contains wildcards.
    Wildcard,
}

impl Display for TopicError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TopicError::Str(_) => write!(f, "invalid publish topic UTF-8 string"),
            TopicError::Wildcard => write!(f, "the publish topic contains wildcard"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for TopicError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TopicError::Str(err) => Some(err),
            TopicError::Wildcard => None,
        }
    }
}

impl From<StrError> for TopicError {
    fn from(v: StrError) -> Self {
        Self::Str(v)
    }
}

/// [`Topic`](PublishTopic) with borrowed data.
pub type PublishTopicRef<'a> = PublishTopic<&'a str>;

/// The Topic Name identifies the information channel to which payload data is published.
#[derive(Debug, Clone, Copy, Eq)]
pub struct PublishTopic<S>(Str<S>);

impl<S> Display for PublishTopic<S>
where
    S: Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<S> Deref for PublishTopic<S> {
    type Target = Str<S>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> TryFrom<&'a str> for PublishTopicRef<'a> {
    type Error = TopicError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let topic = Str::try_from(value)?;

        if contains_wildcard_char(&topic) {
            return Err(TopicError::Wildcard);
        }

        Ok(PublishTopic(topic))
    }
}

impl<'a> Decode<'a> for PublishTopicRef<'a> {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (str, bytes) = Str::parse(bytes)?;

        let topic = PublishTopic::try_from(str.as_str())?;

        Ok((topic, bytes))
    }
}

impl<S1, S2> PartialEq<PublishTopic<S2>> for PublishTopic<S1>
where
    S1: PartialEq<S2>,
{
    fn eq(&self, other: &PublishTopic<S2>) -> bool {
        self.0.eq(&other.0)
    }
}

fn contains_wildcard_char(topic: &str) -> bool {
    memchr2(b'+', b'#', topic.as_bytes()).is_some()
}

/// A PUBACK Packet is the response to a PUBLISH Packet with QoS level 1.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718043>
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PubAck {
    pkid: PacketId,
}

impl PubAck {
    const REMAINIGN_LENGTH: RemainingLength = RemainingLength::new_const(2);

    /// Create the PUBACK for the given packet id.
    #[must_use]
    pub const fn new(pkid: PacketId) -> Self {
        Self { pkid }
    }

    /// Returns the packet identifier.
    #[must_use]
    pub fn pkid(&self) -> PacketId {
        self.pkid
    }
}

impl Display for PubAck {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} pkid({})", ControlPacketType::PubAck, self.pkid)
    }
}

impl<'a> DecodePacket<'a> for PubAck {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::PubAck
    }

    fn fixed_remaining_length() -> Option<RemainingLength> {
        Some(Self::REMAINIGN_LENGTH)
    }

    fn parse_with_header(header: FixedHeader, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        if !header.flags().is_empty() {
            return Err(DecodeError::Reserved);
        }

        let (pkid, bytes) = PacketId::parse(bytes)?;
        debug_assert!(
            bytes.is_empty(),
            "BUG: remaining length was correct, but bytes are still present after parsing"
        );

        Ok(Self::new(pkid))
    }
}

impl EncodePacket for PubAck {
    fn remaining_len(&self) -> usize {
        2
    }

    fn packet_type() -> ControlPacketType {
        ControlPacketType::PubAck
    }

    fn packet_flags(&self) -> TypeFlags {
        TypeFlags::empty()
    }

    fn write_packet<W>(&self, writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        writer
            .write_u16(self.pkid.get())
            .map_err(EncodeError::Write)
    }
}

/// A PUBREC Packet is the response to a PUBLISH Packet with QoS 2. It is the second packet of the QoS 2 protocol exchange.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718048>
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PubRec {
    pkid: PacketId,
}

impl PubRec {
    const REMAINIGN_LENGTH: RemainingLength = RemainingLength::new_const(2);

    /// Create the PUBACK for the given packet id.
    #[must_use]
    pub const fn new(pkid: PacketId) -> Self {
        Self { pkid }
    }

    /// Returns the packet identifier.
    #[must_use]
    pub fn pkid(&self) -> PacketId {
        self.pkid
    }
}

impl Display for PubRec {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} pkid({})", ControlPacketType::PubRec, self.pkid)
    }
}

impl<'a> DecodePacket<'a> for PubRec {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::PubRec
    }

    fn fixed_remaining_length() -> Option<RemainingLength> {
        Some(Self::REMAINIGN_LENGTH)
    }

    fn parse_with_header(header: FixedHeader, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        if !header.flags().is_empty() {
            return Err(DecodeError::Reserved);
        }

        let (pkid, bytes) = PacketId::parse(bytes)?;
        debug_assert!(
            bytes.is_empty(),
            "BUG: remaining length was correct, but bytes are still present after parsing"
        );

        Ok(Self::new(pkid))
    }
}

impl EncodePacket for PubRec {
    fn remaining_len(&self) -> usize {
        2
    }

    fn packet_type() -> ControlPacketType {
        ControlPacketType::PubRec
    }

    fn packet_flags(&self) -> TypeFlags {
        TypeFlags::empty()
    }

    fn write_packet<W>(&self, writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        writer
            .write_u16(self.pkid.get())
            .map_err(EncodeError::Write)
    }
}

/// A PUBREL Packet is the response to a PUBREC Packet.
///
/// It is the third packet of the QoS 2 protocol exchange.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718053>
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PubRel {
    pkid: PacketId,
}

impl PubRel {
    const REMAINIGN_LENGTH: RemainingLength = RemainingLength::new_const(2);

    /// Create the PUBACK for the given packet id.
    #[must_use]
    pub const fn new(pkid: PacketId) -> Self {
        Self { pkid }
    }

    /// Returns the packet identifier.
    #[must_use]
    pub fn pkid(&self) -> PacketId {
        self.pkid
    }
}

impl Display for PubRel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} pkid({})", ControlPacketType::PubRel, self.pkid)
    }
}

impl<'a> DecodePacket<'a> for PubRel {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::PubRel
    }

    fn fixed_remaining_length() -> Option<RemainingLength> {
        Some(Self::REMAINIGN_LENGTH)
    }

    fn parse_with_header(header: FixedHeader, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        if header.flags() != TypeFlags::PUBREL {
            return Err(DecodeError::Reserved);
        }

        let (pkid, bytes) = PacketId::parse(bytes)?;
        debug_assert!(
            bytes.is_empty(),
            "BUG: remaining length was correct, but bytes are still present after parsing"
        );

        Ok(Self::new(pkid))
    }
}

impl EncodePacket for PubRel {
    fn remaining_len(&self) -> usize {
        2
    }

    fn packet_type() -> ControlPacketType {
        ControlPacketType::PubRel
    }

    fn packet_flags(&self) -> TypeFlags {
        TypeFlags::PUBREL
    }

    fn write_packet<W>(&self, writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        writer
            .write_u16(self.pkid.get())
            .map_err(EncodeError::Write)
    }
}

/// The PUBCOMP Packet is the response to a PUBREL Packet. It is the fourth and final packet of the QoS 2 protocol exchange.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718058>
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PubComp {
    pkid: PacketId,
}

impl PubComp {
    const REMAINIGN_LENGTH: RemainingLength = RemainingLength::new_const(2);

    /// Create the PUBACK for the given packet id.
    #[must_use]
    pub const fn new(pkid: PacketId) -> Self {
        Self { pkid }
    }

    /// Returns the packet identifier.
    #[must_use]
    pub fn pkid(&self) -> PacketId {
        self.pkid
    }
}

impl Display for PubComp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} pkid({})", ControlPacketType::PubComp, self.pkid)
    }
}

impl<'a> DecodePacket<'a> for PubComp {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::PubComp
    }

    fn fixed_remaining_length() -> Option<RemainingLength> {
        Some(Self::REMAINIGN_LENGTH)
    }

    fn parse_with_header(header: FixedHeader, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        if !header.flags().is_empty() {
            return Err(DecodeError::Reserved);
        }

        let (pkid, bytes) = PacketId::parse(bytes)?;
        debug_assert!(
            bytes.is_empty(),
            "BUG: remaining length was correct, but bytes are still present after parsing"
        );

        Ok(Self::new(pkid))
    }
}

impl EncodePacket for PubComp {
    fn remaining_len(&self) -> usize {
        2
    }

    fn packet_type() -> ControlPacketType {
        ControlPacketType::PubComp
    }

    fn packet_flags(&self) -> TypeFlags {
        TypeFlags::empty()
    }

    fn write_packet<W>(&self, writer: &mut W) -> Result<usize, super::EncodeError<W::Err>>
    where
        W: super::Writer,
    {
        writer
            .write_u16(self.pkid.get())
            .map_err(EncodeError::Write)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::v3::tests::TestWriter;

    use super::*;

    #[test]
    fn should_reject_wildcards() {
        let err = PublishTopic::try_from("/foo/+").unwrap_err();

        assert!(matches!(err, TopicError::Wildcard));

        let err = PublishTopic::try_from("/foo/#").unwrap_err();

        assert!(matches!(err, TopicError::Wildcard));
    }

    #[test]
    fn should_encode_and_decode_publish() {
        let publish = Publish {
            qos: PublishQos::AtLeastOnce {
                dup: true,
                pkid: PacketId::try_from(10u16).unwrap(),
            },
            retain: false,
            topic: PublishTopic::try_from("a/b").unwrap(),
            payload: [42u8],
        };

        let exp_bytes = [
            0b0011_1010u8,
            8,
            // topic
            0b0000_0000,
            0b0000_0011,
            0b0110_0001,
            0b0010_1111,
            0b0110_0010,
            // pkid
            0b0000_0000,
            0b0000_1010,
            // payload
            0b0010_1010,
        ];

        let mut writer = TestWriter::new();

        publish.write(&mut writer).unwrap();

        assert_eq!(writer.buf, exp_bytes);

        let (res, bytes) = <PublishRef as DecodePacket>::parse(&exp_bytes).unwrap();
        assert!(bytes.is_empty());

        assert_eq!(res, publish);
    }

    #[test]
    fn should_encode_and_decode_puback() {
        let puback = PubAck {
            pkid: PacketId::try_from(10u16).unwrap(),
        };

        let exp_bytes = [
            0b0100_0000u8,
            2,
            // pkid
            0b0000_0000,
            0b0000_1010,
        ];

        let mut writer = TestWriter::new();

        puback.write(&mut writer).unwrap();

        assert_eq!(writer.buf, exp_bytes);

        let (res, bytes) = <PubAck as DecodePacket>::parse(&exp_bytes).unwrap();
        assert!(bytes.is_empty());

        assert_eq!(res, puback);
    }

    #[test]
    fn should_encode_and_decode_pubrec() {
        let pubrec = PubRec {
            pkid: PacketId::try_from(10u16).unwrap(),
        };

        let exp_bytes = [
            0b0101_0000u8,
            2,
            // pkid
            0b0000_0000,
            0b0000_1010,
        ];

        let mut writer = TestWriter::new();

        pubrec.write(&mut writer).unwrap();

        assert_eq!(writer.buf, exp_bytes);

        let (res, bytes) = <PubRec as DecodePacket>::parse(&exp_bytes).unwrap();
        assert!(bytes.is_empty());

        assert_eq!(res, pubrec);
    }

    #[test]
    fn should_encode_and_decode_pubrel() {
        let pubrel = PubRel {
            pkid: PacketId::try_from(10u16).unwrap(),
        };

        let exp_bytes = [
            0b0110_0010u8,
            2,
            // pkid
            0b0000_0000,
            0b0000_1010,
        ];

        let mut writer = TestWriter::new();

        pubrel.write(&mut writer).unwrap();

        assert_eq!(writer.buf, exp_bytes);

        let (res, bytes) = <PubRel as DecodePacket>::parse(&exp_bytes).unwrap();
        assert!(bytes.is_empty());

        assert_eq!(res, pubrel);
    }

    #[test]
    fn should_encode_and_decode_pubcomp() {
        let pubcomp = PubComp {
            pkid: PacketId::try_from(10u16).unwrap(),
        };

        let exp_bytes = [
            0b0111_0000u8,
            2,
            // pkid
            0b0000_0000,
            0b0000_1010,
        ];

        let mut writer = TestWriter::new();

        pubcomp.write(&mut writer).unwrap();

        assert_eq!(writer.buf, exp_bytes);

        let (res, bytes) = <PubComp as DecodePacket>::parse(&exp_bytes).unwrap();
        assert!(bytes.is_empty());

        assert_eq!(res, pubcomp);
    }
}
