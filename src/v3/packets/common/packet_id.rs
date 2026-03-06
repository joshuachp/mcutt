//! Packet id is a common field of the variable headers.

use core::fmt::Display;
use core::num::NonZeroU16;
use std::num::NonZero;

use zerocopy::{byteorder::network_endian::U16, *};

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};

/// Identifier for a Packet with QoS > 0.
///
/// It's a non zero 16-bit value.
#[derive(
    Debug, Clone, Copy, KnownLayout, Immutable, FromBytes, IntoBytes, ByteEq, ByteHash, Unaligned,
)]
#[repr(transparent)]
pub struct PacketId {
    pkid: U16,
}

impl PacketId {
    pub(crate) fn read(&self) -> NonZeroU16 {
        NonZeroU16::new(self.pkid.get()).expect("packet id must be a non zero u16")
    }
}

impl Display for PacketId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PacketId({})", self.pkid)
    }
}

impl Input<NonZero<u16>> for PacketId {
    type Validated = ();

    fn validate_value(_value: &NonZero<u16>) -> Result<(), Error> {
        Ok(())
    }
}

impl Parsed for PacketId {
    fn validate(&self) -> Result<(), Error> {
        if self.pkid == 0 {
            return Err(Error::new(ErrorKind::Invalid, "packet id cannot be 0"));
        }

        Ok(())
    }
}

impl Encode<NonZero<u16>> for PacketId {
    fn encode_len(_value: &NonZero<u16>) -> Result<usize, Error> {
        Ok(size_of::<Self>())
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &NonZero<u16>) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        Self::validate_value(value)?;

        writer
            .write_all(U16::from(value.get()).as_bytes())
            .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "writing packet ID"))?;

        Ok(size_of::<Self>())
    }
}

impl<'a> Decode<'a> for PacketId {
    type Out = &'a Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), Error> {
        let (pkid, rest) = Self::ref_from_prefix(buf)
            .map_err(|_| Error::new(ErrorKind::NotEnoughSpace, "couldn't parse packet id"))?;

        pkid.validate()?;

        Ok((pkid, rest))
    }
}

impl From<PacketId> for usize {
    fn from(value: PacketId) -> Self {
        value.read().get().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_packet_id() {
        let (pkid, rest) = PacketId::parse(&[0x00, 0x01, 0x42]).unwrap();

        assert_eq!(rest, &[0x42]);
        assert_eq!(pkid.read(), NonZeroU16::new(1).unwrap());

        let (pkid, rest) = PacketId::parse(&[0xff, 0xff, 0x42]).unwrap();

        assert_eq!(rest, &[0x42]);
        assert_eq!(pkid.read(), NonZeroU16::new(u16::MAX).unwrap());

        let err = PacketId::parse(&[0x00, 0x00, 0x42]).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::Invalid);
    }
}
