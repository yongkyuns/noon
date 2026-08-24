//! Versioned language-neutral interchange for Noon authoring and live control.

#![forbid(unsafe_code)]

mod legacy;
mod semantic;

pub use legacy::*;
pub use semantic::*;
