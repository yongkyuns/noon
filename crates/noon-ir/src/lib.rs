//! Temporary scene/patch codec for the remaining legacy worker transport.
//!
//! Direct native and Rust/WASM execution use typed semantic operations. #959 owns
//! deleting this codec when the remaining legacy transport consumers migrate.

#![forbid(unsafe_code)]

mod legacy;

pub use legacy::*;
