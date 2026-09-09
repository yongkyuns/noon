//! Immutable geometry, font and text resources and their versioned storage.
//!
//! Semantic authoring and compiled snapshots share these resource contracts.
//! Lookup and atomic replacement operate on resource handles; they do not own
//! scene identity, reactive evaluation, execution state or GPU residency.

mod font;
pub use font::*;

mod geometry;
pub use geometry::*;

mod lookup;
pub use lookup::*;

mod mutation;
pub use mutation::*;

mod text;
pub use text::*;

mod transaction;
pub use transaction::*;
