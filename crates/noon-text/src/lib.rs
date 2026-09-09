#![forbid(unsafe_code)]

//! CPU text services over shared font and glyph resources.
//!
//! Shaping is consumed by Rust authoring; rasterization is consumed by the
//! retained renderer. Both use the same Swash dependency boundary. This provider
//! owns no semantic scene, runtime, platform lifecycle or GPU state.

pub mod raster;
pub mod shaping;
