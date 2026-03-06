//! SUBACK packet in response to a SUBSCRIBE.

use std::fmt::Display;

use zerocopy::*;

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};
use crate::v3::packets::common::fixed::builder::FixedHeaderBuilder;
use crate::v3::packets::common::fixed::{ControlPacketType, FixedHeaderSlice};
use crate::v3::packets::common::packet_id::PacketId;

use super::builder::SubAckBuilder;

/// A SUBACK Packet is sent by the Server to the Client to confirm receipt and processing of a
/// SUBSCRIBE Packet.
///
/// A SUBACK Packet contains a list of return codes, that specify the maximum QoS level that was
/// granted in each Subscription that was requested by the SUBSCRIBE.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718068>
#[derive(Debug, KnownLayout, Immutable, TryFromBytes, IntoBytes, ByteEq)]
#[repr(C)]
pub struct SubAck {
    pkid: PacketId,
    return_codes: [SubAckCode],
}

impl SubAck {
    /// Returns the [`PacketId`] of the packet.
    pub fn pkid(&self) -> &PacketId {
        &self.pkid
    }

    /// Returns the [`PacketId`] of the packet.
    pub fn return_codes(&self) -> &[SubAckCode] {
        &self.return_codes
    }
}

impl Display for SubAck {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} pkid({}) (", ControlPacketType::SubAck, self.pkid)?;

        let mut iter = self.return_codes().iter();

        if let Some(i) = iter.next() {
            write!(f, "{i}")?;

            for i in iter {
                write!(f, ", {i}")?;
            }
        }

        write!(f, ")")
    }
}

impl<'s> Input<SubAckBuilder<'s>> for SubAck {
    type Validated = ();

    fn validate_value<'a>(_value: &SubAckBuilder<'s>) -> Result<Self::Validated, Error> {
        // TODO: evaluate if we need to validate the length of the slice
        Ok(())
    }
}

impl Parsed for SubAck {
    fn validate(&self) -> Result<(), Error> {
        self.pkid.validate()?;

        Ok(())
    }
}

impl<'a> Encode<SubAckBuilder<'a>> for SubAck {
    fn encode_len(value: &SubAckBuilder<'a>) -> Result<usize, Error> {
        PacketId::encode_len(&value.pkid).and_then(|len| {
            len.checked_add(value.return_codes.len())
                .ok_or(Error::new(ErrorKind::Overflow, "SUBACK encode len"))
        })
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &SubAckBuilder<'a>) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        let remaining_length = Self::encode_len(value)?;

        let mut written = FixedHeaderBuilder::new(ControlPacketType::SubAck)
            .remaining_length(remaining_length)?
            .write_sync(writer)?;

        written += PacketId::write_sync(writer, &value.pkid)?;

        let bytes = value.return_codes.as_bytes();
        writer.write_all(bytes).map_err(|error| {
            Error::new(
                ErrorKind::StdIo(error.kind()),
                "while writing SUBACK return codes",
            )
        })?;

        written += bytes.len();

        Ok(written)
    }
}

impl<'a> Decode<'a> for SubAck {
    type Out = &'a Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), Error> {
        let (fixed, rest) = FixedHeaderSlice::parse(buf)?;
        // TODO: should this be an error?
        debug_assert_eq!(fixed.packet_type(), ControlPacketType::SubAck);

        let rem_len = fixed.remaining_length().read_len();

        let codes_count = rem_len
            .checked_sub(size_of::<PacketId>())
            .and_then(|len| len.checked_div(size_of::<SubAckCode>()))
            .ok_or(Error::new(ErrorKind::Overflow, "SUBACK remaining length"))?;

        let (this, rest) = Self::try_ref_from_prefix_with_elems(rest, codes_count).map_err(
            |error| match error {
                ConvertError::Alignment(_) => {
                    Error::new(ErrorKind::Invalid, "SUBACK return code alignment")
                }
                ConvertError::Size(_) => {
                    Error::new(ErrorKind::NotEnoughSpace, "SUBACK not enought space")
                }
                ConvertError::Validity(_) => {
                    Error::new(ErrorKind::Invalid, "SUBACK return code is reserved")
                }
            },
        )?;

        this.validate()?;

        Ok((this, rest))
    }
}

/// Return code for a SUBACK.
#[derive(Debug, Clone, Copy, TryFromBytes, ByteEq, IntoBytes, KnownLayout, Immutable)]
#[repr(u8)]
pub enum SubAckCode {
    /// Success with a maximum QoS of 0.
    Qos0 = 0b0000_0000,
    /// Success with a maximum QoS of 1.
    Qos1 = 0b0000_0001,
    /// Success with a maximum QoS of 2.
    Qos2 = 0b0000_0010,
    /// Failure to subscribe.
    Failure = 0b1000_0000,
}

impl Display for SubAckCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            SubAckCode::Qos0 => "qos0",
            SubAckCode::Qos1 => "qos1",
            SubAckCode::Qos2 => "qos2",
            SubAckCode::Failure => "failure",
        };

        write!(f, "{s}")
    }
}

impl From<SubAckCode> for u8 {
    fn from(value: SubAckCode) -> Self {
        value as u8
    }
}

impl TryFrom<u8> for SubAckCode {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b0000_0000 => Ok(SubAckCode::Qos0),
            0b0000_0001 => Ok(SubAckCode::Qos1),
            0b0000_0010 => Ok(SubAckCode::Qos2),
            0b1000_0000 => Ok(SubAckCode::Failure),
            _ => Err(Error::new(ErrorKind::OutOfRange, "SUBACK return code")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use pretty_assertions::assert_eq;

    use crate::tests::{Hexdump, insta_snapshots};

    use super::*;

    #[test]
    fn suback_roundtrip() {
        let suback = SubAckBuilder {
            pkid: NonZero::new(10u16).unwrap(),
            return_codes: &[
                SubAckCode::Qos0,
                SubAckCode::Qos1,
                SubAckCode::Qos2,
                SubAckCode::Failure,
            ],
        };

        let mut buf = Vec::new();

        let written = SubAck::write_sync(&mut buf, &suback).unwrap();
        assert_eq!(written, buf.len());

        let res = SubAck::consume(&buf).unwrap();

        assert_eq!(res.pkid.read(), suback.pkid);
        assert_eq!(res.return_codes(), suback.return_codes);

        insta_snapshots!({
            insta::assert_snapshot!(Hexdump(buf));
        });
    }
}
