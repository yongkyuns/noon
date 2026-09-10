//! Deterministic renderer-independent path geometry for Noon.

#![forbid(unsafe_code)]

mod geometry_proportion;
mod morph;
mod outline;
mod partial;
mod reverse;
mod smoothing;
pub use reverse::reverse_path;
pub use smoothing::change_path_anchor_mode;
mod tessellation;

pub use geometry_proportion::*;
pub use morph::*;
pub use outline::*;
pub use partial::*;
pub use tessellation::*;
