//! Packets that can be received from a Client.

use core::fmt::{Debug, Display};

use crate::v3::{
    bytes::read_exact,
    packets::{header::ControlPacketType, DecodeError},
};

use super::{
    connect::ConnAck,
    header::FixedHeader,
    ping::PingResp,
    publish::{PubAck, PubComp, PubRec, PubRel, PublishRef},
    subscribe::SubAckRef,
    unsubscribe::UnsubAck,
    Decode, DecodePacket,
};

/// Packets that can be received from a connection.
#[derive(Debug, Clone, Copy)]
pub enum Packet<'a> {
    /// CONNACK Packet, received after a connect.
    ConnAck(ConnAck),
    /// PUBLISH from the Server on a subscribed topic.
    Publish(PublishRef<'a>),
    /// PUBACK from the Server after a PUBLISH with QoS 1.
    PubAck(PubAck),
    /// PUBREC from the Server after a PUBLISH with QoS 2.
    PubRec(PubRec),
    /// A PUBREL Packet is the response to a PUBREC Packet.
    PubRel(PubRel),
    /// The PUBCOMP Packet is the response to a PUBREL Packet.
    PubComp(PubComp),
    /// A SUBACK Packet is the response to a SUBSCRIBE Packet.
    SubAck(SubAckRef<'a>),
    /// A UNSUBACK Packet is the response to a UNSUBSCRIBE Packet.
    UnsubAck(UnsubAck),
    /// A PINGRESP Packet is the response to a PINGREQ Packet.
    PingResp(PingResp),
}

impl<'a> Packet<'a> {
    fn parse_packet<P>(header: FixedHeader, bytes: &'a [u8]) -> Result<Self, DecodeError>
    where
        P: DecodePacket<'a> + Into<Self>,
    {
        P::check_header(&header)?;

        let remaining_length = usize::try_from(header.remaining_length())?;

        if remaining_length != bytes.len() {
            return Err(DecodeError::RemainingBytes);
        }

        let val = P::parse_with_header(header, bytes)?;

        Ok(val.into())
    }

    /// Parses the packet for the packet type in the [header](FixedHeader).
    ///
    /// # Errors
    ///
    /// If the packet is invalid for the given header or the decode failed.
    pub fn parse_with_header(
        header: FixedHeader,
        bytes: &'a [u8],
    ) -> Result<Packet<'a>, DecodeError> {
        match header.packet_type() {
            ControlPacketType::Connect => Err(DecodeError::PacketType(header.packet_type().into())),
            ControlPacketType::ConnAck => Self::parse_packet::<ConnAck>(header, bytes),
            ControlPacketType::Publish => Self::parse_packet::<PublishRef>(header, bytes),
            ControlPacketType::PubAck => Self::parse_packet::<PubAck>(header, bytes),
            ControlPacketType::PubRec => Self::parse_packet::<PubRec>(header, bytes),
            ControlPacketType::PubRel => Self::parse_packet::<PubRel>(header, bytes),
            ControlPacketType::PubComp => Self::parse_packet::<PubComp>(header, bytes),
            ControlPacketType::SubAck => Self::parse_packet::<SubAckRef>(header, bytes),
            ControlPacketType::UnsubAck => Self::parse_packet::<UnsubAck>(header, bytes),
            ControlPacketType::PingResp => Self::parse_packet::<PingResp>(header, bytes),
            ControlPacketType::Subscribe
            | ControlPacketType::Unsubscribe
            | ControlPacketType::PingReq
            | ControlPacketType::Disconnect => {
                // The Subscribe is only sent from the Client to the Server
                Err(DecodeError::Reserved)
            }
        }
    }

    /// Returns `true` if the packet is [`ConnAck`].
    ///
    /// [`ConnAck`]: Packet::ConnAck
    #[must_use]
    pub fn is_conn_ack(&self) -> bool {
        matches!(self, Self::ConnAck(..))
    }

    /// Returns a reference to the packet if it's [`ConnAck`].
    ///
    /// [`ConnAck`]: Packet::ConnAck
    #[must_use]
    pub fn as_conn_ack(&self) -> Option<&ConnAck> {
        if let Self::ConnAck(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Converts the packet if it's [`ConnAck`].
    ///
    /// # Errors
    ///
    /// If the packet is of a different type, returns the packet.
    ///
    /// [`ConnAck`]: Packet::ConnAck
    pub fn try_into_conn_ack(self) -> Result<ConnAck, Self> {
        if let Self::ConnAck(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    /// Returns `true` if the packet is [`Publish`].
    ///
    /// [`Publish`]: Packet::Publish
    #[must_use]
    pub fn is_publish(&self) -> bool {
        matches!(self, Self::Publish(..))
    }

    /// Returns a reference to the packet if it's [`Publish`].
    ///
    /// [`Publish`]: Packet::Publish
    #[must_use]
    pub fn as_publish(&self) -> Option<&PublishRef<'a>> {
        if let Self::Publish(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Converts the packet if it's [`Publish`].
    ///
    /// # Errors
    ///
    /// If the packet is of a different type, returns the packet.
    ///
    /// [`Publish`]: Packet::Publish
    pub fn try_into_publish(self) -> Result<PublishRef<'a>, Self> {
        if let Self::Publish(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    /// Returns `true` if the packet is [`PubAck`].
    ///
    /// [`PubAck`]: Packet::PubAck
    #[must_use]
    pub fn is_pub_ack(&self) -> bool {
        matches!(self, Self::PubAck(..))
    }

    /// Returns a reference if the packet is [`PubAck`].
    ///
    /// [`PubAck`]: Packet::PubAck
    #[must_use]
    pub fn as_pub_ack(&self) -> Option<&PubAck> {
        if let Self::PubAck(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Converts the packet if it's [`PubAck`].
    ///
    /// # Errors
    ///
    /// If the packet is of a different type, returns the packet.
    ///
    /// [`PubAck`]: Packet::PubAck
    pub fn try_into_pub_ack(self) -> Result<PubAck, Self> {
        if let Self::PubAck(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }
}

impl Display for Packet<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Packet::ConnAck(value) => Display::fmt(value, f),
            Packet::Publish(value) => Display::fmt(value, f),
            Packet::PubAck(value) => Display::fmt(value, f),
            Packet::PubRec(value) => Display::fmt(value, f),
            Packet::PubRel(value) => Display::fmt(value, f),
            Packet::PubComp(value) => Display::fmt(value, f),
            Packet::SubAck(value) => Display::fmt(value, f),
            Packet::UnsubAck(value) => Display::fmt(value, f),
            Packet::PingResp(value) => Display::fmt(value, f),
        }
    }
}

impl<'a> Decode<'a> for Packet<'a> {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), super::DecodeError> {
        let (header, bytes) = FixedHeader::parse(bytes)?;

        let remaining_length = usize::try_from(header.remaining_length())?;

        let (bytes, rest) = read_exact(bytes, remaining_length)?;

        let packet = Self::parse_with_header(header, bytes)?;

        Ok((packet, rest))
    }
}

impl From<ConnAck> for Packet<'_> {
    fn from(v: ConnAck) -> Self {
        Self::ConnAck(v)
    }
}

impl<'a> From<PublishRef<'a>> for Packet<'a> {
    fn from(v: PublishRef<'a>) -> Self {
        Self::Publish(v)
    }
}

impl From<PubAck> for Packet<'_> {
    fn from(v: PubAck) -> Self {
        Self::PubAck(v)
    }
}

impl From<PubRec> for Packet<'_> {
    fn from(v: PubRec) -> Self {
        Self::PubRec(v)
    }
}

impl From<PubRel> for Packet<'_> {
    fn from(v: PubRel) -> Self {
        Self::PubRel(v)
    }
}

impl From<PubComp> for Packet<'_> {
    fn from(v: PubComp) -> Self {
        Self::PubComp(v)
    }
}

impl<'a> From<SubAckRef<'a>> for Packet<'a> {
    fn from(value: SubAckRef<'a>) -> Self {
        Self::SubAck(value)
    }
}

impl From<UnsubAck> for Packet<'_> {
    fn from(value: UnsubAck) -> Self {
        Self::UnsubAck(value)
    }
}

impl From<PingResp> for Packet<'_> {
    fn from(value: PingResp) -> Self {
        Self::PingResp(value)
    }
}
