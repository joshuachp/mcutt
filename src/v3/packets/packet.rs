//! Packets that can be received from a Client.

use core::fmt::{Debug, Display};

use crate::bytes::{Decode, Error, ErrorKind, Parsed};

use super::common::fixed::ControlPacketType;
use super::connect::ack::ConnAck;
use super::ping::resp::PingResp;
use super::publish::Publish;
use super::publish::ack::PubAck;
use super::publish::comp::PubComp;
use super::publish::rec::PubRec;
use super::publish::rel::PubRel;
use super::subscribe::ack::SubAck;
use super::unsubscribe::ack::UnsubAck;

/// Packets that can be received from a connection.
#[derive(Debug, Clone, Copy)]
pub enum Packet<'a> {
    /// CONNACK Packet, received after a connect.
    ConnAck(&'a ConnAck),
    /// PUBLISH from the Server on a subscribed topic.
    Publish(Publish<'a>),
    /// PUBACK from the Server after a PUBLISH with QoS 1.
    PubAck(PubAck<'a>),
    /// PUBREC from the Server after a PUBLISH with QoS 2.
    PubRec(PubRec<'a>),
    /// A PUBREL Packet is the response to a PUBREC Packet.
    PubRel(PubRel<'a>),
    /// The PUBCOMP Packet is the response to a PUBREL Packet.
    PubComp(PubComp<'a>),
    /// A SUBACK Packet is the response to a SUBSCRIBE Packet.
    SubAck(&'a SubAck),
    /// A UNSUBACK Packet is the response to a UNSUBSCRIBE Packet.
    UnsubAck(UnsubAck<'a>),
    /// A PINGRESP Packet is the response to a PINGREQ Packet.
    PingResp(PingResp),
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

impl<'a> Parsed for Packet<'a> {
    fn validate(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl<'a> Decode<'a> for Packet<'a> {
    type Out = Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), crate::bytes::Error> {
        let (pkg_type, _falgs) = buf.first()
            .ok_or(Error::new(ErrorKind::NotEnoughSpace, "empty buffer"))
            .copied()
            .and_then(ControlPacketType::parse)?;

        match pkg_type {
            ControlPacketType::ConnAck => {
                ConnAck::parse(buf).map(|(p, rest)| (Packet::ConnAck(p), rest))
            }
            ControlPacketType::Publish => {
                Publish::parse(buf).map(|(p, rest)| (Packet::Publish(p), rest))
            }
            ControlPacketType::PubAck => {
                PubAck::parse(buf).map(|(p, rest)| (Packet::PubAck(p), rest))
            }
            ControlPacketType::PubRec => {
                PubRec::parse(buf).map(|(p, rest)| (Packet::PubRec(p), rest))
            }
            ControlPacketType::PubRel => {
                PubRel::parse(buf).map(|(p, rest)| (Packet::PubRel(p), rest))
            }
            ControlPacketType::PubComp => {
                PubComp::parse(buf).map(|(p, rest)| (Packet::PubComp(p), rest))
            }
            ControlPacketType::SubAck => {
                SubAck::parse(buf).map(|(p, rest)| (Packet::SubAck(p), rest))
            }
            ControlPacketType::UnsubAck => {
                UnsubAck::parse(buf).map(|(p, rest)| (Packet::UnsubAck(p), rest))
            }
            ControlPacketType::PingResp => {
                PingResp::parse(buf).map(|(p, rest)| (Packet::PingResp(p), rest))
            }
            ControlPacketType::Connect
            | ControlPacketType::Subscribe
            | ControlPacketType::Unsubscribe
            | ControlPacketType::PingReq
            | ControlPacketType::Disconnect => {
                Err(Error::new(ErrorKind::Invalid, "packet type received"))
            }
        }
    }
}
