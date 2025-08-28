//! Remaining length for the MQTT [Fixed Header](super::FixedHeader).

use core::borrow::Borrow;
use core::fmt::Display;

use zerocopy::*;

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};

pub(crate) type RemainingLengthSlice = RemainingLength<[u8]>;
pub(crate) type RemainingLengthArray<const N: usize> = RemainingLength<[u8; N]>;

/// The Remaining Length is the number of bytes remaining within the current packet.
///
/// It includes the data in the variable header and the payload. The Remaining Length does not
/// include the bytes used to encode the Remaining Length.
///
/// It's the encoded 1..=4 bytes for the remaining length in the encoded MQTT
/// [Fixed Header](super::FixedHeader).
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718020>
#[derive(
    Debug, Clone, Copy, KnownLayout, Immutable, FromBytes, IntoBytes, ByteEq, SplitAt, Unaligned,
)]
#[repr(C)]
pub struct RemainingLength<B: ?Sized> {
    pub(super) len: B,
}

impl<B: ?Sized> RemainingLength<B> {
    const CONTINUE_FLAG: u8 = 0b1000_0000;
    pub(crate) const VALUE_MASK: u8 = !Self::CONTINUE_FLAG;
    pub(crate) const MAX_BYTES: usize = 4;

    /// Count the continue flags
    pub(crate) fn has_continue_flag(b: u8) -> bool {
        (Self::CONTINUE_FLAG & b) == Self::CONTINUE_FLAG
    }

    /// Count the continue flags
    pub(crate) fn count_continue_flags(buf: &[u8]) -> Result<usize, Error> {
        let flags = buf
            .iter()
            .take_while(|&&b| Self::has_continue_flag(b))
            .take(Self::MAX_BYTES)
            .count();

        if flags >= 4 {
            return Err(Error::new(
                ErrorKind::Invalid,
                "remaining length continuation flags require more than 4 bytes",
            ));
        }

        Ok(flags)
    }

    pub(crate) fn read_from_buf(buff: &[u8]) -> u32 {
        let mut multiplier = 1;

        buff.iter()
            .map(|byte| {
                let value = u32::from(byte & Self::VALUE_MASK);
                let value = value * multiplier;

                multiplier *= 128;

                value
            })
            .sum()
    }

    /// Reads the value of the remaining length
    pub fn read(&self) -> u32
    where
        B: Borrow<[u8]>,
    {
        let mut multiplier = 1;

        self.len
            .borrow()
            .iter()
            .map(|byte| {
                let value = u32::from(byte & Self::VALUE_MASK);
                let value = value * multiplier;

                multiplier *= 128;

                value
            })
            .sum()
    }

    /// Reads the value of the remaining length as usize
    #[cfg(not(target_pointer_width = "16"))]
    pub fn read_len(&self) -> usize
    where
        B: Borrow<[u8]>,
    {
        let mut multiplier = 1;

        self.len
            .borrow()
            .iter()
            .map(|byte| {
                let value = usize::from(byte & Self::VALUE_MASK);
                let value = value * multiplier;

                multiplier *= 128;

                value
            })
            .sum()
    }

    pub(crate) fn len_from_u32(value: &u32) -> Result<usize, Error> {
        let bytes = match value {
            0..=127 => 1,
            128..=16_383 => 2,
            16_384..=2_097_151 => 3,
            2_097_152..=268_435_455 => 4,
            _ => return Err(Error::new(ErrorKind::OutOfRange, "remaining length")),
        };

        Ok(bytes)
    }

    // This assumes the value has been validated
    pub(crate) fn encode_to_array<const N: usize>(mut value: u32) -> [u8; N] {
        debug_assert_eq!(Self::len_from_u32(&value).unwrap(), N);

        core::array::from_fn(|_| {
            // set the byte
            let mut b = (value % 128) as u8;
            // Get the remaining value
            value = value.div_euclid(128);

            // Set the continuation flag
            if value > 0 {
                b |= 128;
            }

            b
        })
    }
}

impl<B: ?Sized> Display for RemainingLength<B>
where
    B: Borrow<[u8]>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RemainingLength({})", self.read())
    }
}

impl<B: ?Sized> Input<u32> for RemainingLength<B> {
    type Validated = usize;

    fn validate_value(value: &u32) -> Result<usize, Error> {
        Self::len_from_u32(value)
    }
}

impl<B: ?Sized> Parsed for RemainingLength<B>
where
    B: Borrow<[u8]>,
{
    fn validate(&self) -> Result<(), Error> {
        let buf = self.len.borrow();

        if buf.len() > 4 {
            return Err(Error::new(
                ErrorKind::Invalid,
                "remaining length too many bytes",
            ));
        }

        let mut iter = buf.iter().rev();

        let Some(last) = iter.next() else {
            return Ok(());
        };

        if Self::has_continue_flag(*last) {
            return Err(Error::new(
                ErrorKind::Invalid,
                "remaining length last byte continuation set",
            ));
        }

        let is_valid = iter.all(|b| Self::has_continue_flag(*b));

        if !is_valid {
            return Err(Error::new(
                ErrorKind::Invalid,
                "remaining length continuation flag unset",
            ));
        }

        Ok(())
    }
}

#[cfg(feature = "std")]
fn write_sync_array<const N: usize, W>(writer: &mut W, value: &u32) -> Result<usize, Error>
where
    W: std::io::Write,
{
    let buf = RemainingLengthSlice::encode_to_array::<N>(*value);

    writer
        .write_all(&buf)
        .map_err(|error| Error::new(ErrorKind::StdIo(error.kind()), "writing remaining length"))?;

    Ok(N)
}

#[cfg(feature = "std")]
impl Encode<u32> for RemainingLength<[u8]> {
    fn encode_len(value: &u32) -> Result<usize, Error> {
        Self::len_from_u32(value)
    }

    fn write_sync<W>(writer: &mut W, value: &u32) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        match value {
            0..=127 => write_sync_array::<1, _>(writer, value),
            128..=16_383 => write_sync_array::<2, _>(writer, value),
            16_384..=2_097_151 => write_sync_array::<3, _>(writer, value),
            2_097_152..=268_435_455 => write_sync_array::<4, _>(writer, value),
            _ => Err(Error::new(ErrorKind::OutOfRange, "remaining length")),
        }
    }
}

impl<const N: usize> Encode<u32> for RemainingLength<[u8; N]> {
    fn encode_len(_value: &u32) -> Result<usize, Error> {
        Ok(N)
    }

    fn write_sync<W>(writer: &mut W, value: &u32) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        let len = Self::validate_value(value)?;

        if len != N {
            return Err(Error::new(ErrorKind::OutOfRange, "remaining length array"));
        }

        write_sync_array::<N, _>(writer, value)
    }
}

impl<'a> Decode<'a> for RemainingLength<[u8]> {
    type Out = &'a Self;

    fn parse(buff: &[u8]) -> Result<(&Self, &[u8]), Error> {
        let flags = Self::count_continue_flags(buff)?;
        debug_assert!((0..=3).contains(&flags));

        let (this, rest) = buff
            .split_at_checked(flags + 1)
            .ok_or(Error::new(ErrorKind::NotEnoughSpace, "remaining length"))?;

        let this: &Self = transmute_ref!(this);

        // We don't need to validate since we already know the size is less than 4 and we counted
        // the continue flags
        debug_assert!((1..=4).contains(&this.len.len()));

        Ok((this, rest))
    }
}

impl<'a, const N: usize> Decode<'a> for RemainingLength<[u8; N]> {
    type Out = &'a Self;

    fn parse(bytes: &[u8]) -> Result<(&Self, &[u8]), Error> {
        let (this, rest) = RemainingLength::ref_from_prefix(bytes)
            .map_err(|_| Error::new(ErrorKind::OutOfRange, "remaining length"))?;

        this.validate()?;

        Ok((this, rest))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn should_encode_remaining_length() {
        let data: &[(u32, &[u8])] = &[
            (0, &[0x00]),
            (127, &[0x7F]),
            (128, &[0x80, 0x01]),
            (16_383, &[0xFF, 0x7F]),
            (16_384, &[0x80, 0x80, 0x01]),
            (2_097_151, &[0xFF, 0xFF, 0x7F]),
            (2_097_152, &[0x80, 0x80, 0x80, 0x01]),
            (268_435_455, &[0xFF, 0xFF, 0xFF, 0x7F]),
        ];

        for (exp, case) in data {
            let (len, rest) = RemainingLengthSlice::parse(case).unwrap();

            assert_eq!(rest.len(), 0);
            assert_eq!(len.read(), *exp);

            let mut buf = Vec::new();

            let written = RemainingLengthSlice::write_sync(&mut buf, exp)
                .unwrap_or_else(|_| panic!("case {exp}"));

            assert_eq!(written, case.len());
        }
    }

    #[test]
    fn should_check_too_big() {
        let err = RemainingLengthArray::<4>::write_sync(&mut Vec::new(), &u32::MAX).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::OutOfRange);
    }
}
