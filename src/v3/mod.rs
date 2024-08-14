//! Data representation of MQTT packets

use core::fmt::Display;

#[cfg(feature = "std")]
use std::error::Error;

use header::{ControlPacketType, RemainingLengthError, StrError, TypeFlags};

pub mod connect;
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
    Str(StrError),
    RemainingLengthBytes,
    ControlFlags {
        packet_type: ControlPacketType,
        flags: TypeFlags,
    },
    PacketType(u8),
    RemainingLength(RemainingLengthError),
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
            | DecodeError::PacketIdentifier => None,
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

pub trait Decode<'a>: Sized {
    /// Parses the bytes into a value.
    ///
    /// It returns the remaining bytes after the packet was parsed.
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError>;
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
    fn write_u8(&mut self, value: u8) -> Result<(), Self::Err>;

    /// Writes a u16 in big endian.
    fn write_u16(&mut self, value: u16) -> Result<(), Self::Err> {
        self.write_all(&value.to_be_bytes())
    }

    /// Flush this output stream, ensuring that all intermediately buffered contents reach their destination.
    fn flush(&mut self) -> Result<(), Self::Err>;
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

        fn flush(&mut self) -> Result<(), Self::Err> {
            Ok(())
        }
    }
}
