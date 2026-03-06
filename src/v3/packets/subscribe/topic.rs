//! Subscribe Topic Filter

use core::fmt::Display;

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};
use crate::v3::packets::common::string::MqttStr;
use crate::v3::packets::common::topic::has_valid_wildcard;

use zerocopy::*;

use super::builder::SubscribeFilter;

/// Indicates a Topic the Clint wants to subscribe.
///
/// The server may or may not support wild card characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicFilter<'a> {
    /// The Topic to subscribe.
    topic: &'a MqttStr,
    /// Requested QoS for the topic.
    ///
    /// This gives the maximum QoS level at which the Server can send Application Messages to the Client.
    qos: &'a RequestedQos,
}

impl<'a> TopicFilter<'a> {
    /// Returns the topic filter
    pub fn topic(&self) -> &'a MqttStr {
        self.topic
    }

    /// Returns the requested QoS
    pub fn qos(&self) -> &'a RequestedQos {
        self.qos
    }
}

impl<'a> Display for TopicFilter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.topic, self.qos)
    }
}

impl PartialEq<SubscribeFilter<'_>> for TopicFilter<'_> {
    fn eq(&self, other: &SubscribeFilter) -> bool {
        let SubscribeFilter { topic, qos } = other;

        *topic == self.topic.as_str() && qos == self.qos
    }
}

impl<'t> Input<SubscribeFilter<'t>> for TopicFilter<'t> {
    type Validated = ();

    fn validate_value(value: &SubscribeFilter<'t>) -> Result<Self::Validated, crate::bytes::Error> {
        if !has_valid_wildcard(value.topic) {
            return Err(Error::new(ErrorKind::Invalid, "SUBSCRIBE topic filter"));
        }

        Ok(())
    }
}

impl<'a> Parsed for TopicFilter<'a> {
    fn validate(&self) -> Result<(), crate::bytes::Error> {
        self.topic.validate()?;

        if !has_valid_wildcard(self.topic.as_str()) {
            return Err(Error::new(ErrorKind::Invalid, "SUBSCRIBE topic filter"));
        }

        Ok(())
    }
}

impl<'a> Encode<SubscribeFilter<'a>> for TopicFilter<'a> {
    fn encode_len(value: &SubscribeFilter<'a>) -> Result<usize, crate::bytes::Error> {
        MqttStr::encode_len(value.topic).and_then(|len| {
            len.checked_add(size_of::<RequestedQos>())
                .ok_or(Error::new(ErrorKind::Overflow, "SUBSCRIBE topic filter"))
        })
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(
        writer: &mut W,
        value: &SubscribeFilter<'a>,
    ) -> Result<usize, crate::bytes::Error>
    where
        W: std::io::Write,
    {
        Self::validate_value(value)?;

        let topic = MqttStr::write_sync(writer, value.topic)?;
        writer.write_all(value.qos.as_bytes()).map_err(|error| {
            Error::new(
                ErrorKind::StdIo(error.kind()),
                "while writing SUBSCRIBE topic filter",
            )
        })?;

        Ok(topic + size_of::<RequestedQos>())
    }
}

impl<'a> Decode<'a> for TopicFilter<'a> {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (topic, buf) = MqttStr::parse(buf)?;

        let (qos, bytes) = RequestedQos::try_ref_from_prefix(buf).map_err(|error| {
            let kind = match error {
                ConvertError::Alignment(_) | ConvertError::Size(_) => ErrorKind::NotEnoughSpace,
                ConvertError::Validity(_) => ErrorKind::Invalid,
            };

            Error::new(kind, "SUBSCRIBE topic filter QoS")
        })?;

        Ok((TopicFilter { topic, qos }, bytes))
    }
}

/// Quality of Service
///
/// Each filter is followed by a byte called the Requested QoS. This gives the maximum QoS level at
/// which the Server can send Application Messages to the Client.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718099>
#[derive(Debug, Clone, Copy, TryFromBytes, IntoBytes, KnownLayout, Immutable, ByteEq, ByteHash)]
#[repr(u8)]
pub enum RequestedQos {
    /// QoS 0: At most once delivery
    AtMostOnce = 0b0000_0000,
    /// QoS 1: At least once delivery
    AtLeastOnce = 0b0000_0001,
    /// QoS 2: Exactly once delivery
    ExactlyOnce = 0b0000_0010,
}

impl Display for RequestedQos {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RequestedQos::AtMostOnce => write!(f, "QoS(0)"),
            RequestedQos::AtLeastOnce => write!(f, "QoS(1)"),
            RequestedQos::ExactlyOnce => write!(f, "QoS(2)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("a/b")]
    #[case("/")]
    fn parse_topic(#[case] topic: &str) {
        let filter = SubscribeFilter {
            topic,
            qos: RequestedQos::AtMostOnce,
        };

        TopicFilter::validate_value(&filter).unwrap();
    }

    #[rstest]
    #[case("#")]
    #[case("sport/#")]
    #[case("sport/tennis/#")]
    fn parse_multi_level_wildcard(#[case] topic: &str) {
        let filter = SubscribeFilter {
            topic,
            qos: RequestedQos::AtMostOnce,
        };

        TopicFilter::validate_value(&filter).unwrap();
    }

    #[rstest]
    #[case("+/tennis/#")]
    #[case("sport/+/player1")]
    #[case("+")]
    #[case("/+")]
    #[case("+/+")]
    fn parse_single_level_wildcard(#[case] topic: &str) {
        let filter = SubscribeFilter {
            topic,
            qos: RequestedQos::AtMostOnce,
        };

        TopicFilter::validate_value(&filter).unwrap();
    }

    #[rstest]
    #[case("sport/tennis#")]
    #[case("sport/tennis/#/ranking")]
    #[case("sport+")]
    fn invalid_wildcard(#[case] topic: &str) {
        let filter = SubscribeFilter {
            topic,
            qos: RequestedQos::AtMostOnce,
        };

        TopicFilter::validate_value(&filter).unwrap_err();
    }
}
