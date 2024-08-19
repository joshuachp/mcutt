//! Packets related publish operations.

use core::{fmt::Display, ops::Deref};

use memchr::memchr2;

use super::{
    header::{ControlPacketType, FixedHeader, PacketId, Str, StrError, TypeFlags},
    Decode, DecodeError, DecodePacket, Encode, EncodePacket,
};

#[cfg(feature = "alloc")]
pub use self::alloc::{ClientPublishOwned, PublishOwned, TopicOwned};

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
    topic: Topic<S>,
    payload: B,
}

impl<S, B> Publish<S, B> {
    /// Create a publish from the client one with the missing data.
    pub fn with_qos(pkid: PacketId, publish: ClientPublish<S, B>, qos: Qos) -> Self {
        let mut publish = Publish::from(publish);

        publish.qos = PublishQos::from_qos(pkid, qos);

        publish
    }

    /// Returns the packet identifier if the QoS > 0.
    pub fn pkid(&self) -> Option<PacketId> {
        self.qos.pkid()
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
    B: Deref<Target = [u8]>,
{
    fn remaining_len(&self) -> usize {
        let mut len = self.topic.encode_len();

        if let Some(pkid) = self.qos.pkid() {
            len = len.saturating_add(pkid.encode_len());
        }

        len.saturating_add(self.payload.deref().encode_len())
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

        Ok(variable.saturating_add(self.payload.deref().write(writer)?))
    }
}

impl<'a> DecodePacket<'a> for PublishRef<'a> {
    fn packet_type() -> ControlPacketType {
        ControlPacketType::Publish
    }

    fn parse_with_header(header: FixedHeader, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let (topic, bytes) = Topic::parse(bytes)?;

        let (qos, payload) = PublishQos::parse_with_header(&header, bytes)?;

        let retain = header.flags().contains(TypeFlags::PUBLISH_RETAIN);

        Ok(Self {
            qos,
            retain,
            topic,
            payload,
        })
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
enum PublishQos {
    AtMostOnce,
    AtLeastOnce { dup: bool, pkid: PacketId },
    ExactlyOnce { dup: bool, pkid: PacketId },
}

impl PublishQos {
    fn from_qos(pkid: PacketId, qos: Qos) -> Self {
        match qos {
            Qos::AtLeastOnce => PublishQos::AtLeastOnce { dup: false, pkid },
            Qos::ExactlyOnce => PublishQos::ExactlyOnce { dup: false, pkid },
        }
    }

    fn pkid(&self) -> Option<PacketId> {
        match self {
            PublishQos::AtMostOnce => None,
            PublishQos::AtLeastOnce { pkid, .. } | PublishQos::ExactlyOnce { pkid, .. } => {
                Some(*pkid)
            }
        }
    }

    fn parse_with_header<'a>(
        header: &FixedHeader,
        bytes: &'a [u8],
    ) -> Result<(Self, &'a [u8]), DecodeError> {
        let dup = header.flags().contains(TypeFlags::PUBLISH_DUP);

        let qos = header.flags() & TypeFlags::PUBLISH_QOS_MASK;

        const QOS_0: TypeFlags = TypeFlags::empty();

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
}

/// [`ClientPublish`] with borrowed data.
pub type ClientPublishRef<'a> = ClientPublish<&'a str, &'a [u8]>;

/// Client publish data.
///
/// This is a struct can be passed to the connection to publish data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientPublish<S, B> {
    retain: bool,
    topic: Topic<S>,
    payload: B,
}

impl<S, B> ClientPublish<S, B> {
    /// Creates a new PUBLISH with the given topic and payload.
    pub fn new(topic: Topic<S>, payload: B) -> Self {
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
pub enum Qos {
    /// At least once delivery (QoS 1).
    AtLeastOnce,
    /// Exactly once delivery (QoS 2).
    ExactlyOnce,
}

impl Display for Qos {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Qos::AtLeastOnce => write!(f, "at least once delivery (1)"),
            Qos::ExactlyOnce => write!(f, "exactly once delivery (2)"),
        }
    }
}

/// Error for the publish [`Topic`]
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

/// [`Topic`] with borrowed data.
pub type TopicRef<'a> = Topic<&'a str>;

/// The Topic Name identifies the information channel to which payload data is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Topic<S>(Str<S>);

impl<S> Deref for Topic<S> {
    type Target = Str<S>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> TryFrom<&'a str> for TopicRef<'a> {
    type Error = TopicError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let topic = Str::try_from(value)?;

        if contains_wildcard_char(&topic) {
            return Err(TopicError::Wildcard);
        }

        Ok(Topic(topic))
    }
}

impl<'a> Decode<'a> for TopicRef<'a> {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (str, bytes) = Str::parse(bytes)?;

        let topic = Topic::try_from(str.as_str())?;

        Ok((topic, bytes))
    }
}

fn contains_wildcard_char(topic: &str) -> bool {
    memchr2(b'+', b'#', topic.as_bytes()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_reject_wildcards() {
        let err = Topic::try_from("/foo/+").unwrap_err();

        assert!(matches!(err, TopicError::Wildcard));

        let err = Topic::try_from("/foo/#").unwrap_err();

        assert!(matches!(err, TopicError::Wildcard));
    }
}
