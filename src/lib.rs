//! MQTT client implementation.

#![warn(missing_docs, missing_debug_implementations)]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg(feature = "alloc")]
extern crate alloc;

mod bytes;
#[cfg(feature = "std")]
pub mod slab;
#[cfg(feature = "sync")]
pub mod sync;
pub mod v3;
