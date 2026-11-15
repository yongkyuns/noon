//! Leaf and family root alignment share one semantic/resource operation.
mod roots;
use crate::{AuthoringError, LayoutAnchor, Mobject, Scene};
use std::rc::Rc;

impl Scene {
    /// Align the addressed roots atomically, preserving both identities and shapes.
    pub fn align_path_points(
        &mut self,
        left: &LayoutAnchor,
        right: &LayoutAnchor,
    ) -> Result<(), AuthoringError> {
        if self.running_execution().is_some() {
            return self.with_running_execution(|store, root, execution| {
                roots::publish_root_alignment(store, root, execution, left, right)
            });
        }
        if !Rc::ptr_eq(self.integration_store(), left.integration_store())
            || !Rc::ptr_eq(self.integration_store(), right.integration_store())
        {
            return Err(AuthoringError::ForeignStore);
        }
        left.align_path_points(right)
    }
}

impl Mobject {
    /// Align owned Manim point arrays through shared root path preparation.
    /// Both operands retain identity, paint and visible shape. Publication is
    /// atomic; aliases are a no-op and unrelated geometry is untouched.
    pub fn align_points(&self, other: &Mobject) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).align_path_points(&LayoutAnchor::from(other))
    }
}
