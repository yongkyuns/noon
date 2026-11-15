//! Canonical text objects and atomic numeric-label families.
#[cfg(any(feature = "native-text", feature = "typst"))]
use super::TextAuthoringError;
#[cfg(feature = "typst")]
use super::{MathTypst, Typst, TypstSpec};
#[cfg(feature = "native-text")]
use super::{Text, NATIVE_POINT_TO_SCENE_SCALE};

#[cfg(feature = "latex")]
pub(crate) mod decimal_labels;
#[cfg(any(feature = "native-text", feature = "typst"))]
mod objects;
#[cfg(feature = "native-text")]
pub(crate) use objects::prepare_native_text;
#[cfg(feature = "native-text")]
mod number_labels;
#[cfg(all(feature = "native-text", feature = "latex"))]
pub(crate) use decimal_labels::PreparedDecimalLabels;
