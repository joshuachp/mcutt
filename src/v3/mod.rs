//! Data representation of MQTT packets

use core::fmt::Display;

#[cfg(feature = "std")]
use std::error::Error;

use header::{ControlPacketType, RemainingLength, TypeFlags};

pub mod header;

#[derive(Debug)]
pub enum DecodeError {
    NotEnoughBytes {
        needed: usize,
        actual: usize,
    },
    FrameTooBig {
        max: usize,
    },
    Utf8,
    RemainingLengthBytes,
    ControlFlags {
        packet_type: ControlPacketType,
        flags: TypeFlags,
    },
    PacketType(u8),
    MaxRemainingLength(u32),
    PacketIdentifier,
}

impl Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::NotEnoughBytes { needed, actual } => {
                write!(
                    f,
                    "not enough bytes, buffer contains {actual} and {needed} more are needed"
                )
            }
            DecodeError::FrameTooBig { max } => write!(
                f,
                "packet requires a length that exceeds the maximum of {max} bytes"
            ),
            DecodeError::Utf8 => write!(f, "the string was not UTF-8 encoded"),
            DecodeError::RemainingLengthBytes => {
                write!(f, "remaining length exceeded the maximum of 4 bytes")
            }
            DecodeError::ControlFlags { packet_type, flags } => {
                write!(f, "invalid control packet flags {flags} for {packet_type}")
            }
            DecodeError::PacketType(packet_type) => {
                write!(f, "invalid control packet type {packet_type}")
            }
            DecodeError::MaxRemainingLength(value) => {
                write!(
                    f,
                    "remaining length {value} is bigger than the maximum {}",
                    RemainingLength::MAX
                )
            }
            DecodeError::PacketIdentifier => {
                write!(f, "the packet identifier needs to be non-zero")
            }
        }
    }
}

#[cfg(feature = "std")]
impl Error for DecodeError {}

pub trait Decode<'a>: Sized {
    /// Parses the bytes into a value.
    ///
    /// It returns the remaining bytes after the packet was parsed.
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError>;
}
