//! Deterministic renderer-independent path geometry for Noon.

#![forbid(unsafe_code)]

mod boolean;
mod flatten;
pub use boolean::{boolean_paths, BooleanOperation, BooleanPathError, BOOLEAN_FLATTEN_TOLERANCE};
mod geometry_proportion;
mod isoline;
mod morph;
mod outline;
mod partial;
mod reverse;
mod smoothing;
pub use reverse::reverse_path;
pub use smoothing::change_path_anchor_mode;
mod tessellation;
mod vector_field;
mod vector_field_defaults;

pub use geometry_proportion::*;
pub use isoline::*;
pub use morph::*;
pub use outline::*;
pub use partial::*;
pub use tessellation::*;
pub use vector_field::*;
pub use vector_field_defaults::*;
