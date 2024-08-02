//! Data representation of MQTT packets

use zerocopy::byteorder::network_endian::U16;
use zerocopy::{ByteSlice, Ref};

/// UTF-8 encoded string.
///
/// Text fields in the Control Packets described later are encoded as UTF-8 strings.
///
/// > https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718016
pub fn parse_str<'a, B: ByteSlice + 'a>(bytes: B) -> Option<(&'a str, B)> {
    let (lenght, bytes) = Ref::<B, U16>::new_from_prefix(bytes).unwrap();

    let lenght = usize::from(lenght.read());

    let (data, bytes) = Ref::new_slice_from_prefix(bytes, lenght)?;

    core::str::from_utf8(data.into_slice())
        .ok()
        .map(|s| (s, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_example_str() {
        const EXAMPLE_STRING: &[u8] = &[0x00, 0x05, 0x41, 0xF0, 0xAA, 0x9B, 0x94];
        const EXPECTED: &str = "A𪛔";

        let (str, rest) = parse_str(EXAMPLE_STRING).unwrap();

        assert!(rest.is_empty());

        assert_eq!(str, EXPECTED);
    }
}
