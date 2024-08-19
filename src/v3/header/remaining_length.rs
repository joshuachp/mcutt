//! Remaining length for the MQTT [Fixed Header](super::FixedHeader).

use core::{fmt::Display, num::TryFromIntError, ops::Deref};

use crate::{
    bytes::read_chunk,
    v3::{Decode, DecodeError, Encode, EncodeError, Writer},
};

/// Error for an invalid [`RemainingLength`].
#[derive(Debug)]
#[non_exhaustive]
pub enum RemainingLengthError {
    /// The value is greater than the [`max`](RemainingLength::MAX)
    Max {
        /// The invalid value.
        value: u32,
    },
    /// Checked integer conversion failed
    TryFromInt(TryFromIntError),
    /// Invalid remaining length for the packet
    InvalidLength {
        /// The expected length.
        expected: u32,
        /// The actual length.
        actual: u32,
    },
}

impl Display for RemainingLengthError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RemainingLengthError::Max { value } => write!(
                f,
                "remaining length {value} is greater than the maximum {}",
                RemainingLength::MAX
            ),
            RemainingLengthError::TryFromInt(..) => write!(f, "couldn't convert the value to u32"),
            RemainingLengthError::InvalidLength { expected, actual } => {
                write!(f, "packet has must have remaining length of {expected}, but received a length of {actual}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RemainingLengthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RemainingLengthError::Max { .. } | RemainingLengthError::InvalidLength { .. } => None,
            RemainingLengthError::TryFromInt(err) => Some(err),
        }
    }
}

impl From<TryFromIntError> for RemainingLengthError {
    fn from(value: TryFromIntError) -> Self {
        Self::TryFromInt(value)
    }
}

/// The Remaining Length is the number of bytes remaining within the current packet.
///
/// It includes the data in the variable header and the payload. The Remaining Length does not
/// include the bytes used to encode the Remaining Length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemainingLength(u32);

impl RemainingLength {
    const CONTINUE_FLAG: u8 = 0b1000_0000;
    const VALUE_MASK: u8 = !Self::CONTINUE_FLAG;
    /// Maximum value of the remaining length
    pub const MAX: u32 = 268_435_455;

    // count the continue flags
    fn count_continue_flags(bytes: &[u8]) -> usize {
        bytes
            .iter()
            .take_while(|&b| RemainingLength::CONTINUE_FLAG & *b != 0)
            .take(4)
            .count()
    }

    /// Number of bytes the length will occupy
    fn bytes_len(&self) -> usize {
        match self.0 {
            0..=127 => 1,
            128..=16_383 => 2,
            16_384..=2_097_151 => 3,
            2_097_152..=268_435_455 => 4,
            _ => unreachable!("remaining length cannot exceed the max"),
        }
    }

    fn parse_with_length<const N: usize>(bytes: &[u8]) -> Result<(Self, &[u8]), DecodeError> {
        let (len, bytes) = RawRemainingLength::<N>::parse(bytes)?;

        Ok((Self(len.read()), bytes))
    }

    fn write_with_len<const N: usize, W>(
        &self,
        writer: &mut W,
    ) -> Result<usize, EncodeError<W::Err>>
    where
        W: Writer,
    {
        let mut buf = [0u8; N];

        RawRemainingLength::from_len(self, &mut buf).write(writer)
    }
}

impl<'a> Decode<'a> for RemainingLength {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let n_continue = Self::count_continue_flags(bytes);

        match n_continue {
            0 => Self::parse_with_length::<1>(bytes),
            1 => Self::parse_with_length::<2>(bytes),
            2 => Self::parse_with_length::<3>(bytes),
            3 => Self::parse_with_length::<4>(bytes),
            _ => Err(DecodeError::RemainingLengthBytes),
        }
    }
}

impl Encode for RemainingLength {
    fn encode_len(&self) -> usize {
        self.bytes_len()
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: Writer,
    {
        match self.bytes_len() {
            1 => self.write_with_len::<1, W>(writer),
            2 => self.write_with_len::<2, W>(writer),
            3 => self.write_with_len::<3, W>(writer),
            4 => self.write_with_len::<4, W>(writer),
            _ => unreachable!("remaining length cannot exceed the max"),
        }
    }
}

impl TryFrom<u32> for RemainingLength {
    type Error = RemainingLengthError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value > Self::MAX {
            return Err(RemainingLengthError::Max { value });
        }

        Ok(Self(value))
    }
}

impl TryFrom<usize> for RemainingLength {
    type Error = RemainingLengthError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        let value = u32::try_from(value)?;

        RemainingLength::try_from(value)
    }
}

impl TryFrom<RemainingLength> for usize {
    type Error = RemainingLengthError;

    fn try_from(value: RemainingLength) -> Result<Self, Self::Error> {
        usize::try_from(value.0).map_err(RemainingLengthError::TryFromInt)
    }
}

impl Deref for RemainingLength {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Raw remaining length.
///
/// It's the encoded 1..=4 bytes for the remaining length in the encoded MQTT
/// [Fixed Header](super::FixedHeader).
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718020>
#[derive(Debug, Clone, Copy)]
pub(super) struct RawRemainingLength<'a, const N: usize>(&'a [u8; N]);

impl<'a, const N: usize> RawRemainingLength<'a, N> {
    const _CHECK: () = assert!(N <= 4);

    /// The bytes must be a valid remaining length.
    fn read(&self) -> u32 {
        debug_assert_eq!(RemainingLength::count_continue_flags(self.0) + 1, N);

        let mut multiplier = 1;

        self.0
            .iter()
            .map(|b| {
                let value = u32::from(b & RemainingLength::VALUE_MASK);
                let value = value * multiplier;

                multiplier *= 128;

                value
            })
            .sum()
    }

    fn from_len(len: &RemainingLength, buf: &'a mut [u8; N]) -> Self {
        debug_assert_eq!(len.bytes_len(), N);

        let mut value = len.0;

        let iter = core::iter::from_fn(|| {
            // If the value is 0, no bytes should be written
            if value == 0 {
                return None;
            }

            // This will never fail
            let mut encoded = u8::try_from(value.rem_euclid(128)).ok()?;

            value = value.div_euclid(128);

            if value > 0 {
                encoded |= RemainingLength::CONTINUE_FLAG;
            }

            Some(encoded)
        });

        buf.iter_mut().zip(iter).for_each(|(b, v)| {
            *b = v;
        });

        Self(buf)
    }
}

impl<'a, const N: usize> Decode<'a> for RawRemainingLength<'a, N> {
    fn parse(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), DecodeError> {
        let (remaining_length, bytes) = read_chunk(bytes)?;

        Ok((Self(remaining_length), bytes))
    }
}

impl<'a, const N: usize> Encode for RawRemainingLength<'a, N> {
    fn encode_len(&self) -> usize {
        N
    }

    fn write<W>(&self, writer: &mut W) -> Result<usize, EncodeError<W::Err>>
    where
        W: Writer,
    {
        writer.write_slice(self.0).map_err(EncodeError::Write)?;

        Ok(self.encode_len())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::v3::{
        header::{ControlPacketType, FixedHeader, TypeFlags},
        tests::TestWriter,
        Decode,
    };

    use super::*;

    #[test]
    fn should_parse_fixed_headers_remaining_length() {
        let data: &[(u32, &[u8])] = &[
            (0, &[0b00010000, 0x00]),
            (127, &[0b00010000, 0x7F]),
            (128, &[0b00010000, 0x80, 0x01]),
            (16_383, &[0b00010000, 0xFF, 0x7F]),
            (16_384, &[0b00010000, 0x80, 0x80, 0x01]),
            (2_097_151, &[0b00010000, 0xFF, 0xFF, 0x7F]),
            (2_097_152, &[0b00010000, 0x80, 0x80, 0x80, 0x01]),
            (268_435_455, &[0b00010000, 0xFF, 0xFF, 0xFF, 0x7F]),
        ];

        for (exp, bytes) in data {
            let (fixed, rest) = FixedHeader::parse(bytes).unwrap();

            assert!(rest.is_empty());

            assert_eq!(*fixed.remaining_length, *exp);
        }
    }

    #[test]
    fn fixed_headers_encode() {
        let data: &[(u32, &[u8])] = &[
            (0, &[0b00010000, 0x00]),
            (127, &[0b00010000, 0x7F]),
            (128, &[0b00010000, 0x80, 0x01]),
            (16_383, &[0b00010000, 0xFF, 0x7F]),
            (16_384, &[0b00010000, 0x80, 0x80, 0x01]),
            (2_097_151, &[0b00010000, 0xFF, 0xFF, 0x7F]),
            (2_097_152, &[0b00010000, 0x80, 0x80, 0x80, 0x01]),
            (268_435_455, &[0b00010000, 0xFF, 0xFF, 0xFF, 0x7F]),
        ];

        for (v, exp) in data {
            let remaining_length = RemainingLength::try_from(*v).unwrap();
            let fix = FixedHeader::new(
                ControlPacketType::Connect,
                TypeFlags::empty(),
                remaining_length,
            );

            let mut buf = TestWriter::new();

            let written = fix.write(&mut buf).unwrap();
            assert_eq!(written, exp.len());
            assert_eq!(&buf.buf, exp);
        }
    }

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

        for (v, exp) in data {
            let r = RemainingLength::try_from(*v).unwrap();

            let mut buf = TestWriter::new();
            let written = r.write(&mut buf).unwrap();

            assert_eq!(written, exp.len());
            assert_eq!(&buf.buf, exp);
        }
    }
}
