//! Packets that can be received from a Client.

use core::fmt::{Debug, Display};

use crate::{
    bytes::read_exact,
    v3::{header::ControlPacketType, DecodeError},
};

use super::{
    connect::ConnAck,
    header::FixedHeader,
    publish::{PubAck, PublishRef},
    Decode, DecodePacket,
};

/// Packets that can be received from a connection.
#[derive(Debug, Clone, Copy)]
pub enum Packet<'a> {
    /// CONNACK packet, received after a connect.
    ConnAck(ConnAck),
    /// PUBLISH from the Server on a subscribed topic.
    Publish(PublishRef<'a>),
    /// PUBACK from the Server after a PUBLISH with QoS 1.
    PubAck(PubAck),
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
    pub fn parse_with_header(header: FixedHeader, bytes: &'a [u8]) -> Result<Packet, DecodeError> {
        match header.packet_type() {
            ControlPacketType::Connect => Err(DecodeError::PacketType(header.packet_type().into())),
            ControlPacketType::ConnAck => Self::parse_packet::<ConnAck>(header, bytes),
            ControlPacketType::Publish => Self::parse_packet::<PublishRef>(header, bytes),
            ControlPacketType::PubAck => Self::parse_packet::<PubAck>(header, bytes),
            ControlPacketType::PubRec => todo!(),
            ControlPacketType::PubRel => todo!(),
            ControlPacketType::PubComp => todo!(),
            ControlPacketType::Subscribe => todo!(),
            ControlPacketType::SubAck => todo!(),
            ControlPacketType::Unsubscribe => todo!(),
            ControlPacketType::UnsubAck => todo!(),
            ControlPacketType::PingReq => todo!(),
            ControlPacketType::PingResp => todo!(),
            ControlPacketType::Disconnect => todo!(),
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

impl<'a> Display for Packet<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Packet::ConnAck(value) => Display::fmt(value, f),
            Packet::Publish(value) => Display::fmt(value, f),
            Packet::PubAck(value) => Display::fmt(value, f),
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

impl<'a> From<PubAck> for Packet<'a> {
    fn from(v: PubAck) -> Self {
        Self::PubAck(v)
    }
}

impl<'a> From<PublishRef<'a>> for Packet<'a> {
    fn from(v: PublishRef<'a>) -> Self {
        Self::Publish(v)
    }
}

impl<'a> From<ConnAck> for Packet<'a> {
    fn from(v: ConnAck) -> Self {
        Self::ConnAck(v)
    }
}