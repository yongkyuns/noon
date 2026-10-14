#![forbid(unsafe_code)]

//! CPU text services over shared font and glyph resources.
//!
//! Shaping is consumed by Rust authoring; rasterization is consumed by the
//! retained renderer. Both share font resources and CPU geometry preparation. This provider
//! owns no semantic scene, runtime, platform lifecycle or GPU state.

pub mod latex;
pub mod latex_document;
pub mod markup;
pub mod outline;
pub mod raster;
pub mod shaping;
