//! Builder for a publish packet

use core::num::NonZero;
use std::fmt::Display;

use crate::v3::packets::common::fixed::TypeFlags;

/// QoS of a publish packet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PublishQos {
    /// At most once delivery.
    #[default]
    AtMostOnce,
    /// At least once delivery.
    AtLeastOnce {
        /// Whether the packet is a duplicate.
        dup: bool,
        /// Packet identifier for the publish.
        pkid: NonZero<u16>,
    },
    /// Exactly once delivery.
    ExactlyOnce {
        /// Whether the packet is a duplicate.
        dup: bool,
        /// Packet identifier for the publish.
        pkid: NonZero<u16>,
    },
}

impl Display for PublishQos {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PublishQos::AtMostOnce => write!(f, "QoS(0) PkI"),
            PublishQos::AtLeastOnce { dup, pkid } => write!(f, "QoS(1) Pkid({pkid}) Dup({dup})"),
            PublishQos::ExactlyOnce { dup, pkid } => write!(f, "QoS(2) Pkid({pkid}) Dup({dup})"),
        }
    }
}

/// Builder for a Publish packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishBuilder<'a> {
    pub(crate) flags: TypeFlags,
    pub(crate) pkid: Option<NonZero<u16>>,
    pub(crate) topic: &'a str,
    // TODO: make this a generic for a trait like WriteTo, to write this into the writer
    pub(crate) payload: &'a [u8],
}

impl<'a> PublishBuilder<'a> {
    /// Create a new publish packet with the given topic and payload.
    pub fn new(topic: &'a str, payload: &'a [u8]) -> Self {
        Self {
            flags: TypeFlags::empty(),
            pkid: None,
            topic,
            payload,
        }
    }

    /// Sets the retain flag on the packet
    pub fn retain(mut self) -> Self {
        self.flags |= TypeFlags::PUBLISH_RETAIN;

        self
    }

    /// Sets the QoS of the publish
    pub fn qos(mut self, qos: PublishQos) -> Self {
        // TODO: maybe add a type parameter to the builder so this can only be called once
        self.flags &= !(TypeFlags::PUBLISH_QOS_1 | TypeFlags::PUBLISH_QOS_2);

        match qos {
            PublishQos::AtMostOnce => {}
            PublishQos::AtLeastOnce { dup, pkid } => {
                if dup {
                    self.flags |= TypeFlags::PUBLISH_DUP;
                }

                self.flags |= TypeFlags::PUBLISH_QOS_1;
                self.pkid = Some(pkid);
            }
            PublishQos::ExactlyOnce { dup, pkid } => {
                if dup {
                    self.flags |= TypeFlags::PUBLISH_DUP;
                }

                self.flags |= TypeFlags::PUBLISH_QOS_2;
                self.pkid = Some(pkid);
            }
        }

        self
    }
}
