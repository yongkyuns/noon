//! Leaf and family root alignment share one semantic/resource operation.
mod roots;
use crate::{AuthoringError, LayoutAnchor, Mobject};

impl Mobject {
    /// Align owned Manim point arrays through shared root path preparation.
    /// Both operands retain identity, paint and visible shape. Publication is
    /// atomic; aliases are a no-op and unrelated geometry is untouched.
    pub fn align_points(&self, other: &Mobject) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).align_path_points(&LayoutAnchor::from(other))
    }
}
