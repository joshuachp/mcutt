//! Message like the MQTT Will.

use core::fmt::Debug;

use zerocopy::{network_endian::U16, *};

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};

/// MQTT buffer with two bytes prefix as length.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718016>
#[derive(SplitAt, ByteEq, FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct Buff {
    pub(crate) len: U16,
    pub(crate) data: [u8],
}

impl Buff {
    /// Returns the length of the buffer.
    pub fn len(&self) -> usize {
        debug_assert_eq!(usize::from(self.len.get()), self.data.len());

        self.len.into()
    }

    /// Returns buffer as a slice of bytes.
    pub fn as_slice(&self) -> &[u8] {
        debug_assert_eq!(usize::from(self.len.get()), self.data.len());

        &self.data
    }

    /// Returns true if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Debug for Buff {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Buff { len, data } = self;

        f.debug_struct("Buff")
            .field("len", &len)
            .field("data", &format_args!("{:02x?}", data))
            .finish()
    }
}

impl Input<[u8]> for Buff {
    type Validated = u16;

    fn validate_value(value: &[u8]) -> Result<Self::Validated, Error> {
        u16::try_from(value.len()).map_err(|_| Error::new(ErrorKind::OutOfRange, "buf len too big"))
    }
}

impl Parsed for Buff {
    fn validate(&self) -> Result<(), Error> {
        debug_assert_eq!(usize::from(self.len), self.data.len());

        Ok(())
    }
}

impl Encode<[u8]> for Buff {
    fn encode_len(value: &[u8]) -> Result<usize, Error> {
        size_of::<u16>()
            .checked_add(value.len())
            .ok_or(Error::new(ErrorKind::Overflow, "buff len"))
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &[u8]) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        let buf_len = Self::validate_value(value)?;

        writer
            .write_all(U16::from(buf_len).as_bytes())
            .map_err(|error| {
                Error::new(ErrorKind::StdIo(error.kind()), "writing message buffer len")
            })?;
        writer.write_all(value).map_err(|error| {
            Error::new(
                ErrorKind::StdIo(error.kind()),
                "writing message buffer value",
            )
        })?;

        // We already checked the size is less than 2 bytes
        Ok(size_of::<u16>() + value.len())
    }
}

impl<'a> Decode<'a> for Buff {
    type Out = &'a Self;
    fn parse(buf: &[u8]) -> Result<(&Self, &[u8]), Error> {
        let this = Self::ref_from_bytes(buf)
            .map_err(|_| Error::new(ErrorKind::NotEnoughSpace, "buf too small"))?;

        // this.len is u16 so always valid, we just need to split
        let (this, rest) = this
            // Split at the u16 size
            .split_at(this.len.into())
            .ok_or(Error::new(ErrorKind::NotEnoughSpace, "for len prefix"))?
            .via_immutable();

        this.validate()?;

        Ok((this, rest))
    }
}
