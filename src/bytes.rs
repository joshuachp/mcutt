//! Trait to read and write some bytes from a buffer

use std::io::Write;

/// Error information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Value is out of range.
    OutOfRange,
    /// Not enough space to store value.
    NotEnoughSpace,
    /// Integer operation overflow.
    Overflow,
    /// Invalid value.
    Invalid,
    #[cfg(feature = "std")]
    /// Std io error
    StdIo(std::io::ErrorKind),
}

impl core::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ErrorKind::OutOfRange => write!(f, "value is out of range"),
            ErrorKind::NotEnoughSpace => write!(f, "not enough space"),
            ErrorKind::Invalid => write!(f, "value is invalid"),
            ErrorKind::Overflow => write!(f, "overflowed"),
            ErrorKind::StdIo(kind) => write!(f, "{kind}"),
        }
    }
}

/// Error returned while encoding a packet.
#[derive(Debug)]
#[non_exhaustive]
pub struct Error {
    kind: ErrorKind,
    // TODO: make available with format
    msg: &'static str,
}

impl Error {
    pub(crate) const fn new(kind: ErrorKind, msg: &'static str) -> Self {
        Self { kind, msg }
    }

    /// Returns the kind of the error
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} {}", self.kind, self.msg,)
    }
}

pub(crate) trait Input<V>
where
    V: ?Sized,
{
    type Validated;

    fn validate_value(value: &V) -> Result<Self::Validated, Error>;
}

pub(crate) trait Parsed {
    fn validate(&self) -> Result<(), Error>;
}

/// Encodes a value
pub(crate) trait Encode<V>: Input<V>
where
    V: ?Sized,
{
    /// Calculates the encode length from the value.
    ///
    /// The value should not be validated inside this call.
    fn encode_len(value: &V) -> Result<usize, Error>;

    /// Writes the value into the buffer.
    ///
    /// It will return the number of bytes written.
    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &V) -> Result<usize, Error>
    where
        W: Write;
}

/// Decodes a value from bytes
pub(crate) trait Decode<'a>: Parsed {
    type Out;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), Error>;

    fn consume(buf: &'a [u8]) -> Result<Self::Out, Error> {
        let (this, rest) = Self::parse(buf)?;

        if !rest.is_empty() {
            return Err(Error::new(
                ErrorKind::Invalid,
                "remaining bytes in consumed packet",
            ));
        }

        Ok(this)
    }
}
