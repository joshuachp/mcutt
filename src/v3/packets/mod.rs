//! Data representation of MQTT packets

use core::fmt::Display;

pub mod common;
pub mod connect;
pub mod disconnect;
pub mod packet;
pub mod ping;
pub mod publish;
pub mod subscribe;
pub mod unsubscribe;

/// Quality of service for a various message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qos {
    /// At most once delivery.
    AtMostOnce,
    /// At least once delivery.
    AtLeastOnce,
    /// Exactly once delivery.
    ExactlyOnce,
}

impl Display for Qos {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Qos::AtMostOnce => write!(f, "QoS(0)"),
            Qos::AtLeastOnce => write!(f, "QoS(1)"),
            Qos::ExactlyOnce => write!(f, "QoS(2)"),
        }
    }
}

impl From<Qos> for u8 {
    fn from(value: Qos) -> Self {
        match value {
            Qos::AtMostOnce => 0,
            Qos::AtLeastOnce => 1,
            Qos::ExactlyOnce => 2,
        }
    }
}

impl TryFrom<u8> for Qos {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Qos::AtMostOnce),
            1 => Ok(Qos::AtLeastOnce),
            2 => Ok(Qos::ExactlyOnce),
            3.. => Err(value),
        }
    }
}
