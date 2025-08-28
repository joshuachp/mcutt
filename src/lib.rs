//! MQTT client implementation.

#![warn(missing_docs, missing_debug_implementations)]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg(feature = "alloc")]
extern crate alloc;

pub mod bytes;
#[cfg(feature = "std")]
pub mod slab;
#[cfg(feature = "sync")]
pub mod sync;
pub mod v3;

#[cfg(test)]
pub(crate) mod tests {
    use std::borrow::Borrow;
    use std::fmt::Display;

    #[derive(Debug, Clone, Copy)]
    pub(crate) struct Hexdump<T>(pub(crate) T)
    where
        T: Borrow<[u8]>;

    impl<T> Display for Hexdump<T>
    where
        T: Borrow<[u8]>,
    {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let buf = self.0.borrow();
            writeln!(f, "Length: {} ({:#x}) bytes", buf.len(), buf.len())?;

            for (i, b) in buf.iter().map(|b| b.to_le()).enumerate() {
                let b_h = b >> 4;
                let b_l = b & 0x0f;

                let c = if b.is_ascii_graphic() { b as char } else { '.' };

                writeln!(f, "{i:04}: | {b_h:04b} {b_l:04b} | ({b:02x}) '{c}'")?;
            }

            Ok(())
        }
    }

    macro_rules! insta_snapshots {
        ($asserts:block) => {
            ::insta::with_settings!({
                snapshot_path => concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots")
            }, $asserts);
        };
    }

    pub(crate) use insta_snapshots;

    #[test]
    fn use_macro() {
        insta_snapshots!({
            insta::assert_snapshot!("used the macro");
        });
    }
}
