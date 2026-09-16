//! Canonical text objects and atomic numeric-label families.
use super::TextAuthoringError;
#[cfg(feature = "native-text")]
use super::{Text, NATIVE_POINT_TO_SCENE_SCALE};
#[cfg(feature = "typst")]
use super::{MathTypst, Typst, TypstSpec};

mod objects;
#[cfg(feature = "native-text")]
pub(crate) use objects::native_text_state;
#[cfg(feature = "typst")]
pub(crate) use objects::{math_typst_state, typst_state};
#[cfg(feature = "native-text")]
mod number_labels;
