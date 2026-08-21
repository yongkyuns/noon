//! Deterministic renderer-independent path geometry for Noon.

#![forbid(unsafe_code)]

mod morph;
mod tessellation;

pub use morph::*;
pub use tessellation::*;
