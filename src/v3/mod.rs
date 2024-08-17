//! Data representation of MQTT packets

use core::fmt::Display;

#[cfg(feature = "std")]
use std::error::Error;

use header::{ControlPacketType, FixedHeader, RemainingLengthError, StrError, TypeFlags};

use crate::bytes::read_exact;

pub mod connect;
pub mod header;

/// The default maximum packet size permitted by the spec.
///
/// This is the size of:
/// - 1 byte for packet type and flags
/// - 4 bytes for the max remaining length
/// - 268_435_455 bytes for the maximum remaining length value
// Safety: The constant is non zero
pub const MAX_PACKET_SIZE: usize = 1 + 4 + 268_435_455;

#[derive(Debug)]
pub enum DecodeError {
    NotEnoughBytes {
        needed: usize,
    },
    FrameTooBig {
        max: usize,
    },
    Str(StrError),
    RemainingLengthBytes,
    ControlFlags {
        packet_type: ControlPacketType,
        flags: TypeFlags,
    },
    PacketType(u8),
    RemainingLength(RemainingLengthError),
    PacketIdentifier,
    MismatchedPacketType {
        expected: ControlPacketType,
        actual: ControlPacketType,
    },
    Reserved,
}

impl DecodeError {
    pub(crate) const fn not_enough(bytes: &[u8], length: usize) -> Self {
        debug_assert!(bytes.len() < length);

        Self::NotEnoughBytes {
            needed: length.saturating_sub(bytes.len()),
        }
    }
}

impl Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::NotEnoughBytes { needed } => {
                write!(f, "not enough bytes, {needed} more are needed")
            }
            DecodeError::FrameTooBig { max } => write!(
                f,
                "packet requires a length that exceeds the maximum of {max} bytes"
            ),
            DecodeError::Str(_) => write!(f, "couldn't decode the string"),
            DecodeError::RemainingLengthBytes => {
                write!(f, "remaining length exceeded the maximum of 4 bytes")
            }
            DecodeError::ControlFlags { packet_type, flags } => {
                write!(f, "invalid control packet flags {flags} for {packet_type}")
            }
            DecodeError::PacketType(packet_type) => {
                write!(f, "invalid control packet type {packet_type}")
            }
            DecodeError::RemainingLength(_) => {
                write!(f, "invalid remaining length")
            }
            DecodeError::PacketIdentifier => {
                write!(f, "the packet identifier needs to be non-zero")
            }
            DecodeError::MismatchedPacketType { expected, actual } => {
                write!(
                    f,
                    "expected control packet {expected}, but received {actual}"
                )
            }
            DecodeError::Reserved => write!(f, "invalid reserved bits were set in the packet"),
        }
    }
}

#[cfg(feature = "std")]
impl Error for DecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DecodeError::NotEnoughBytes { .. }
            | DecodeError::FrameTooBig { .. }
            | DecodeError::RemainingLengthBytes
            | DecodeError::ControlFlags { .. }
            | DecodeError::PacketType(_)
            | DecodeError::PacketIdentifier
            | DecodeError::MismatchedPacketType { .. }
            | DecodeError::Reserved => None,
            DecodeError::Str(err) => Some(err),
            DecodeError::RemainingLength(err) => Some(err),
        }
    }
}

impl From<RemainingLengthError> for DecodeError {
    fn from(value: RemainingLengthError) -> Self {
        Self::RemainingLength(value)
    }
}

impl From<StrError> for DecodeError {
    fn from(value: StrError) -> Self {
        DecodeError::Str(value)
    }
}

/// Decode a MQTT value.
pub trait Decode<'a>: Sized {
    /// Parses the bytes into a packet.
    ///
    /// It's a utility to parse a full packet from the given bytes and returns remaining unparsed
    /// ones.
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError>;
}

pub trait DecodePacket<'a>: Sized {
    /// Parses the bytes into a packet.
    ///
    /// It's a utility to parse a full packet from the given bytes and returns remaining unparsed
    /// ones.
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (header, bytes) = FixedHeader::parse(bytes)?;

        Self::check_header(&header)?;

        let remaining_length = header.remaining_length().try_into()?;

        let (bytes, rest) = read_exact(bytes, remaining_length)?;

        let val = Self::parse_with_header(header, bytes)?;

        Ok((val, rest))
    }

    /// Checks to perform on the [`FixedHeader`].
    fn check_header(header: &FixedHeader) -> Result<(), DecodeError> {
        let actual = header.packet_type();
        let expected = Self::packet_type();
        if actual != expected {
            return Err(DecodeError::MismatchedPacketType { expected, actual });
        }

        let opt_fixed_len =
            Self::fixed_remaining_length().filter(|len| *len != *header.remaining_length());
        if let Some(expected) = opt_fixed_len {
            return Err(DecodeError::RemainingLength(
                RemainingLengthError::InvalidLength {
                    expected,
                    actual: *header.remaining_length(),
                },
            ));
        }

        Ok(())
    }

    /// Check if the remaining length is valid.
    ///
    /// For packets with a fixed length, this function can be overwritten to check it.
    fn fixed_remaining_length() -> Option<u32> {
        None
    }

    fn packet_type() -> ControlPacketType;

    /// Parses the bytes into a packet from fixed header.
    ///
    /// It must consume all of the bytes in the message.
    fn parse_with_header(header: FixedHeader, bytes: &'a [u8]) -> Result<Self, DecodeError>;
}

#[derive(Debug)]
#[non_exhaustive]
pub enum EncodeError<W> {
    Write(W),
    RemainingLength(RemainingLengthError),
    FrameTooBig { max: usize },
}

impl<W> Display for EncodeError<W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EncodeError::Write(_) => write!(f, "couldn't write to the writer"),
            EncodeError::RemainingLength(_) => write!(f, "invalid remaining length"),
            EncodeError::FrameTooBig { max } => write!(
                f,
                "packet requires a length that exceeds the maximum of {max} bytes"
            ),
        }
    }
}

#[cfg(feature = "std")]
impl<W> std::error::Error for EncodeError<W>
where
    W: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            EncodeError::Write(err) => Some(err),
            EncodeError::RemainingLength(err) => Some(err),
            EncodeError::FrameTooBig { .. } => None,
        }
    }
}

impl<W> From<RemainingLengthError> for EncodeError<W> {
    fn from(value: RemainingLengthError) -> Self {
        Self::RemainingLength(value)
    }
}

pub trait Encode {
    /// Parses the bytes into a value.
    ///
    /// It returns the remaining bytes after the packet was parsed.
    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: Writer;
}

/// Writer trait to be compatible with `no_std`.
///
/// It always returns the usize to make it easier to count the bytes written
pub trait Writer {
    type Err;

    /// Attempts to write an entire buffer into this writer.
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Err>;

    /// Writes a single byte.
    fn write_u8(&mut self, value: u8) -> Result<(), Self::Err> {
        self.write_all(&[value])
    }

    /// Writes a u16 in big endian.
    fn write_u16(&mut self, value: u16) -> Result<(), Self::Err> {
        self.write_all(&value.to_be_bytes())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub struct TestWriter {
        pub buf: Vec<u8>,
    }
    impl TestWriter {
        pub(crate) fn new() -> Self {
            Self { buf: Vec::new() }
        }
    }

    impl Writer for TestWriter {
        type Err = ();

        fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Err> {
            self.buf.extend(buf);

            Ok(())
        }

        fn write_u8(&mut self, value: u8) -> Result<(), Self::Err> {
            self.buf.push(value);

            Ok(())
        }
    }
}
