//! Immutable geometry, font, image and text resources and their versioned storage.
//!
//! Semantic authoring and compiled snapshots share these resource contracts.
//! Lookup and atomic replacement operate on resource handles; they do not own
//! scene identity, reactive evaluation, execution state or GPU residency.

mod font;
pub use font::*;

mod geometry;
pub use geometry::*;

mod image;
pub use image::*;

mod lookup;
pub use lookup::*;

mod mutation;
pub use mutation::*;

mod text;
pub use text::*;

mod text_parts;
pub use text_parts::*;

mod text_styles;
pub use text_styles::*;

mod transaction;
pub use transaction::*;
