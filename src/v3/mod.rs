//! Data representation of MQTT packets

use core::fmt::Display;

#[cfg(feature = "std")]
use std::error::Error;

use header::{
    ControlPacketType, FixedHeader, PacketIdError, RemainingLength, RemainingLengthError, StrError,
    TypeFlags,
};
use publish::TopicError;

use crate::bytes::read_exact;

pub mod connect;
pub mod header;
pub mod packet;
pub mod publish;
pub mod subscribe;

/// The default maximum packet size permitted by the spec.
///
/// This is the size of:
/// - `1` byte for packet type and flags
/// - `4` bytes for the max remaining length
/// - `268_435_455` bytes for the maximum remaining length value
// Safety: The constant is non zero
pub const MAX_PACKET_SIZE: usize = 1 + 4 + 268_435_455;

/// Couldn't decode the packet.
#[derive(Debug)]
pub enum DecodeError {
    /// Not enough bytes to read in the buffer.
    NotEnoughBytes {
        /// The additional bytes needed to continue reading.
        needed: usize,
    },
    /// A field of the packet exceeds it's maximum limit.
    FrameTooBig {
        /// Maximum bytes for the field.
        max: usize,
    },
    /// Couldn't decode the MQTT string.
    Str(StrError),
    /// The remaining length exceeds the maximum of 4 bytes.
    RemainingLengthBytes,
    /// Invalid control packet flags for the given packet type.
    ControlFlags {
        /// The type of the control packet.
        packet_type: ControlPacketType,
        /// The received flags.
        flags: TypeFlags,
    },
    /// Invalid or reserved packet type.
    PacketType(u8),
    /// Couldn't decode the [`RemainingLength`] field.
    RemainingLength(RemainingLengthError),
    /// Invalid [`PacketId`](self::header::PacketId).
    PacketIdentifier(PacketIdError),
    /// Invalid packet type.
    ///
    /// This error is returned when the protocol expects a specific packet identifier.
    MismatchedPacketType {
        /// The expected Control packet type.
        expected: ControlPacketType,
        /// The actual packet type received.
        actual: ControlPacketType,
    },
    /// A reserved field was used.
    ///
    /// This indicates a protocol violation.
    Reserved,
    /// Couldn't decode the publish topic.
    PublishTopic(TopicError),
    /// Not all bytes consumed in the packet
    RemainingBytes,
    /// A [`Subscribe`](subscribe::Subscribe) packet must have at least one topic filter.
    EmptySubscribe,
}

impl DecodeError {
    pub(crate) const fn not_enough(bytes: &[u8], length: usize) -> Self {
        debug_assert!(bytes.len() < length);

        Self::NotEnoughBytes {
            needed: length.saturating_sub(bytes.len()),
        }
    }

    /// Check if the error requires the connection to be closed.
    #[must_use]
    pub fn must_close(&self) -> bool {
        match self {
            DecodeError::NotEnoughBytes { .. } => false,
            DecodeError::FrameTooBig { .. }
            | DecodeError::Str(_)
            | DecodeError::RemainingLengthBytes
            | DecodeError::ControlFlags { .. }
            | DecodeError::PacketType(_)
            | DecodeError::RemainingLength(_)
            | DecodeError::PacketIdentifier(_)
            | DecodeError::MismatchedPacketType { .. }
            | DecodeError::Reserved
            | DecodeError::PublishTopic(_)
            | DecodeError::RemainingBytes
            | DecodeError::EmptySubscribe => true,
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
            DecodeError::PacketIdentifier(_) => {
                write!(f, "couldn't decode the packet identifier")
            }
            DecodeError::MismatchedPacketType { expected, actual } => {
                write!(
                    f,
                    "expected control packet {expected}, but received {actual}"
                )
            }
            DecodeError::Reserved => write!(f, "invalid reserved bits were set in the packet"),
            DecodeError::PublishTopic(_) => write!(f, "couldn't decode the publish topic"),
            DecodeError::RemainingBytes => {
                write!(f, "not all the bytes in the packet were consumed")
            }
            DecodeError::EmptySubscribe => {
                write!(f, "a subscribe packet must have at leaset one topic filter")
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
            | DecodeError::MismatchedPacketType { .. }
            | DecodeError::Reserved
            | DecodeError::RemainingBytes
            | DecodeError::EmptySubscribe => None,
            DecodeError::PacketIdentifier(err) => Some(err),
            DecodeError::Str(err) => Some(err),
            DecodeError::RemainingLength(err) => Some(err),
            DecodeError::PublishTopic(err) => Some(err),
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

impl From<TopicError> for DecodeError {
    fn from(value: TopicError) -> Self {
        DecodeError::PublishTopic(value)
    }
}

impl From<PacketIdError> for DecodeError {
    fn from(value: PacketIdError) -> Self {
        Self::PacketIdentifier(value)
    }
}

/// Error returned while encoding a packet.
#[derive(Debug)]
#[non_exhaustive]
pub enum EncodeError<W> {
    /// Couldn't write to the underling [`Writer`].
    Write(W),
    /// Couldn't encode the [`RemainingLength`].
    RemainingLength(RemainingLengthError),
    /// A field length exceeds the maximum value.
    FrameTooBig {
        /// The maximum value for the field.
        max: usize,
    },
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

/// Decode a MQTT value.
pub trait Decode<'a>: Sized {
    /// Parses the bytes into a packet.
    ///
    /// It's a utility to parse a full packet from the given bytes and returns remaining unparsed
    /// ones.
    ///
    /// # Errors
    ///
    /// This function returns an error if the decode failed because the data is invalid, ore more
    /// bytes are needed to decode.
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError>;
}

impl<'a, T> Decode<'a> for T
where
    T: DecodePacket<'a>,
{
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        <T as DecodePacket>::parse(bytes)
    }
}

/// Decode a MQTT packet that is received.
pub trait DecodePacket<'a>: Sized {
    /// Parses the bytes into a packet.
    ///
    /// It's a utility to parse a full packet from the given bytes and returns remaining unparsed
    /// ones.
    ///
    /// # Errors
    ///
    /// This function returns an error if the decode failed because the data is invalid, ore more
    /// bytes are needed to decode.
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (header, bytes) = FixedHeader::parse(bytes)?;

        Self::check_header(&header)?;

        let remaining_length = header.remaining_length().try_into()?;

        let (bytes, rest) = read_exact(bytes, remaining_length)?;

        let val = Self::parse_with_header(header, bytes)?;

        Ok((val, rest))
    }

    /// Checks to perform on the [`FixedHeader`].
    ///
    /// # Errors
    ///
    /// The header is invalid if:
    ///
    /// - The expected packet type is different from the received one.
    /// - The packet expect a fixed remaining length and it's different from received one.
    fn check_header(header: &FixedHeader) -> Result<(), DecodeError> {
        let actual = header.packet_type();
        let expected = Self::packet_type();
        if actual != expected {
            return Err(DecodeError::MismatchedPacketType { expected, actual });
        }

        let opt_fixed_len =
            Self::fixed_remaining_length().filter(|len| *len != header.remaining_length());
        if let Some(expected) = opt_fixed_len {
            return Err(DecodeError::RemainingLength(
                RemainingLengthError::InvalidLength {
                    expected: *expected,
                    actual: *header.remaining_length(),
                },
            ));
        }

        Ok(())
    }

    /// Check if the remaining length is valid.
    ///
    /// For packets with a fixed length, this function can be overwritten to check it.
    #[must_use]
    fn fixed_remaining_length() -> Option<RemainingLength> {
        None
    }

    /// The control packet type to be decoded.
    fn packet_type() -> ControlPacketType;

    /// Parses the bytes into a packet from fixed header.
    ///
    /// It must consume all of the bytes in the message.
    ///
    /// # Errors
    ///
    /// This function returns an error if the decode failed because the data is invalid or it didn't
    /// consume all the bytes in the payload.
    fn parse_with_header(header: FixedHeader, bytes: &'a [u8]) -> Result<Self, DecodeError>;
}

/// Encode a MQTT packet to be sent.
pub trait EncodePacket {
    /// Number of bytes that will be encoded in the Variable Header and Payload.
    fn remaining_len(&self) -> usize;

    /// Returns the [`FixedHeader`] control packet type.
    fn packet_type() -> ControlPacketType;

    /// Returns the [`FixedHeader`] packet flags.
    fn packet_flags(&self) -> TypeFlags;

    /// Returns the fixed headers for the packet.
    ///
    /// # Errors
    ///
    /// If the remaining length is invalid and cannot be encoded.
    fn fixed_header(&self) -> Result<FixedHeader, RemainingLengthError> {
        RemainingLength::try_from(self.remaining_len()).map(|remaining_length| {
            FixedHeader::new(Self::packet_type(), self.packet_flags(), remaining_length)
        })
    }

    /// Writes the variable header and payload of the packet.
    ///
    /// # Errors
    ///
    /// If the underling write fails, the remaining length is incorrect or the frame is to big to
    /// encode.
    fn write_packet<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: Writer;

    /// Write the full packet.
    ///
    /// It will write into the [`Writer`] the fixed header, variable header and payload of the
    /// packet.
    ///
    /// # Errors
    ///
    /// It will return error if the underling [`Writer`] operation failed or the encode failed.
    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: Writer,
    {
        let fixed_header = self.fixed_header()?;

        let fixed = fixed_header.write(writer)?;
        debug_assert_eq!(fixed, fixed_header.encode_len());

        let variable = self.write_packet(writer)?;
        debug_assert_eq!(variable, self.remaining_len());

        Ok(fixed.saturating_add(variable))
    }
}

/// Encode a MQTT value.
pub trait Encode {
    /// Number of bytes that will be encoded.
    ///
    /// This is the number of bytes the value will take up when encoded in the final packet. It is
    /// used to pre-allocate resources and calculating the remaining length of the payload before
    /// encoding.
    fn encode_len(&self) -> usize;

    /// Parses the bytes into a value.
    ///
    /// It returns the remaining bytes after the packet was parsed.
    ///
    /// # Errors
    ///
    /// It will return error if the underling [`Writer`] operation failed or the encode failed.
    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: Writer;
}

impl<T> Encode for &T
where
    T: Encode,
{
    fn encode_len(&self) -> usize {
        <T as Encode>::encode_len(self)
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: Writer,
    {
        <T as Encode>::write(self, writer)
    }
}

/// Encode a raw buffer, without prefixing it with a length.
impl Encode for &[u8] {
    fn encode_len(&self) -> usize {
        self.len()
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: Writer,
    {
        writer.write_slice(self).map_err(EncodeError::Write)?;

        Ok(self.len())
    }
}

/// Writer trait to be compatible with `no_std`.
///
/// It always returns the usize to make it easier to count the bytes written
pub trait Writer {
    /// The error returned from the write operations
    type Err;

    /// Attempts to write an entire buffer into this writer.
    ///
    /// # Errors
    ///
    /// It will return error if the underling write failed.
    fn write_slice(&mut self, buf: &[u8]) -> Result<usize, Self::Err>;

    /// Writes a single byte.
    ///
    /// # Errors
    ///
    /// It will return error if the underling write failed.
    fn write_u8(&mut self, value: u8) -> Result<usize, Self::Err> {
        self.write_slice(&[value])
    }

    /// Writes a u16 in big endian.
    ///
    /// # Errors
    ///
    /// It will return error if the underling write failed.
    fn write_u16(&mut self, value: u16) -> Result<usize, Self::Err> {
        self.write_slice(&value.to_be_bytes())
    }
}

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

#[cfg(test)]
pub mod tests {
    use super::*;

    #[derive(Debug)]
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

        fn write_slice(&mut self, buf: &[u8]) -> Result<usize, Self::Err> {
            self.buf.extend(buf);

            Ok(buf.len())
        }
    }
}
