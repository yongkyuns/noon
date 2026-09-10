//! Retained glyph preparation and GPU residency within the shared renderer.
//!
//! These helpers consume shared text/runtime data. They own disposable raster,
//! atlas and draw caches, while the main renderer preserves shared painter order.

pub mod atlas;
mod preparation;
pub use preparation::*;

#[cfg_attr(test, allow(clippy::single_range_in_vec_init))]
mod gpu;
pub use gpu::*;
