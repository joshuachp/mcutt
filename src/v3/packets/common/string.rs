//! UTF-8 encoded string.

use core::fmt::{Debug, Display};

use zerocopy::{network_endian::U16, *};

use crate::bytes::{Decode, Encode, Error, ErrorKind, Input, Parsed};

use super::message::Buff;

/// UTF-8 encoded string.
///
/// Text fields in the Control Packets described later are encoded as UTF-8 strings.
///
/// <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718016>
#[derive(ByteEq, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C, packed)]
pub struct MqttStr {
    len: U16,
    data: str,
}

impl MqttStr {
    /// Max number of bytes in a string
    pub const MAX_BYTES: usize = u16::MAX as usize;

    /// Returns the length of the string in bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the string is empty.
    pub fn is_empty(&self) -> bool {
        self.data.len() == 0
    }

    /// Returns the inner string.
    ///
    /// # Panics
    ///
    /// If the string is invalid, this should never happen since it's validated when constructing.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.data
    }

    /// Returns the string as a byte slice.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.data.as_bytes()
    }
}

impl Debug for MqttStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(self.as_str(), f)
    }
}

impl Display for MqttStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self.as_str(), f)
    }
}

impl Input<str> for MqttStr {
    type Validated = ();

    fn validate_value(value: &str) -> Result<(), Error> {
        // check for all valid chars
        if !value.chars().all(is_valid_char) {
            return Err(Error::new(
                ErrorKind::Invalid,
                "string contains invalid characters",
            ));
        }

        Ok(())
    }
}

impl Parsed for MqttStr {
    fn validate(&self) -> Result<(), Error> {
        debug_assert_eq!(usize::from(self.len), self.data.len());

        // check for all valid chars
        if !self.data.chars().all(is_valid_char) {
            return Err(Error::new(
                ErrorKind::Invalid,
                "string contains invalid characters",
            ));
        }

        Ok(())
    }
}

impl Encode<str> for MqttStr {
    fn encode_len(value: &str) -> Result<usize, Error> {
        std::mem::size_of::<u16>()
            .checked_add(value.len())
            .ok_or(Error::new(ErrorKind::Overflow, "string length"))
    }

    #[cfg(feature = "std")]
    fn write_sync<W>(writer: &mut W, value: &str) -> Result<usize, Error>
    where
        W: std::io::Write,
    {
        Self::validate_value(value)?;

        Buff::write_sync(writer, value.as_bytes())
            .map_err(|error| Error::new(error.kind(), "writing MQTT string"))
    }
}

impl<'a> Decode<'a> for MqttStr {
    type Out = &'a Self;

    fn parse(buf: &'a [u8]) -> Result<(Self::Out, &'a [u8]), Error> {
        let (buff, rest) = Buff::parse(buf)?;

        let this = MqttStr::try_ref_from_bytes(buff.as_bytes()).map_err(|error| {
            let kind = match error {
                ConvertError::Alignment(_) | ConvertError::Size(_) => ErrorKind::NotEnoughSpace,
                ConvertError::Validity(_) => ErrorKind::Invalid,
            };

            Error::new(kind, "mqtt string")
        })?;

        this.validate()?;

        Ok((this, rest))
    }
}

impl PartialEq<&str> for &MqttStr {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// Check if the UTF-8 character is valid for the MQTT string.
///
/// This will check for NULL, control and non characters.
///
/// See <https://www.unicode.org/faq/private_use.html#nonchar1> of a list of non characters. This
/// set is formally immutable.
fn is_valid_char(c: char) -> bool {
    match c {
        '\u{0000}'
        | '\u{0001}'..='\u{001F}'
        | '\u{FDD0}'..='\u{FDEF}'
        | '\u{FFFE}'..='\u{FFFF}' => false,
        _ => {
            // Last two of the supplementary planes
            !is_last_two_supplementary(c)
        }
    }
}

/// The last two code points of each of the 16 supplementary planes.
///
// ```
// U+1FFFE, U+1FFFF, U+2FFFE, U+2FFFF, ... U+10FFFE, U+10FFFF
// ```
#[inline]
fn is_last_two_supplementary(c: char) -> bool {
    let val = u32::from(c);

    let last_bits = val & 0xFF;

    (last_bits == 0xFE || last_bits == 0xFF) && (0x01..=0x10).contains(&(val >> 16))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn should_parse_example_str() {
        const EXAMPLE_STRING: &[u8] = &[0x00, 0x05, 0x41, 0xF0, 0xAA, 0x9B, 0x94];
        const EXPECTED: &str = "A𪛔";

        let (str, rest) = MqttStr::parse(EXAMPLE_STRING).unwrap();

        assert_eq!(rest, []);
        assert_eq!(str.as_str(), EXPECTED);
    }

    #[test]
    fn should_be_a_valid_char() {
        let null = '\u{0000}';

        let ranges = [
            '\u{0001}'..='\u{001F}',
            '\u{FDD0}'..='\u{FDEF}',
            '\u{FFFE}'..='\u{FFFF}',
        ];

        let supplementary = [
            '\u{1FFFE}',
            '\u{1FFFF}',
            '\u{2FFFE}',
            '\u{2FFFF}',
            '\u{3FFFE}',
            '\u{3FFFF}',
            '\u{4FFFE}',
            '\u{4FFFF}',
            '\u{5FFFE}',
            '\u{5FFFF}',
            '\u{6FFFE}',
            '\u{6FFFF}',
            '\u{7FFFE}',
            '\u{7FFFF}',
            '\u{8FFFE}',
            '\u{8FFFF}',
            '\u{9FFFE}',
            '\u{9FFFF}',
            '\u{10FFFE}',
            '\u{10FFFF}',
        ];

        let iter = core::iter::once(null)
            .chain(ranges.into_iter().flatten())
            .chain(supplementary);

        for c in iter {
            assert!(
                !is_valid_char(c),
                "should not be valid {:08X}",
                u32::from(c)
            );
        }
    }

    #[test]
    fn alpha_numeric_should_be_valid() {
        let iter = ['a'..='z', 'A'..='Z', '0'..='9'];
        for c in iter.into_iter().flatten() {
            assert!(is_valid_char(c));
        }
    }

    #[test]
    fn mqttstr_eq_str() {
        let str = MqttStr::consume(&[0x00, 0x05, b'H', b'e', b'l', b'l', b'o']).unwrap();

        assert_eq!(str, "Hello");
    }
}
