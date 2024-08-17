//! MQTT client implementation.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs, missing_debug_implementations)]

mod bytes;
#[cfg(feature = "sync")]
pub mod sync;
pub mod v3;
