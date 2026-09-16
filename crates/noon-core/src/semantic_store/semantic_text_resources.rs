//! Text resource import and atomic cold glyph-text admission.
use super::SemanticStore;

mod batch;
mod import;
pub use import::SemanticTextImportError;
