//! Leaf and family root alignment share one semantic/resource operation.
mod roots;
use crate::{AuthoringError, ExecutionSession, LayoutAnchor, Mobject};
use noon_core::{SemanticNodeId, SemanticStore};
pub use roots::publish_root_alignment;
use std::{cell::RefCell, rc::Rc};

pub fn publish_alignment(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    left: &Mobject,
    right: &Mobject,
) -> Result<(), AuthoringError> {
    publish_root_alignment(
        store,
        root,
        execution,
        &LayoutAnchor::from(left),
        &LayoutAnchor::from(right),
    )
}

impl Mobject {
    /// Align owned Manim point arrays through shared root path preparation.
    /// Both operands retain identity, paint and visible shape. Publication is
    /// atomic; aliases are a no-op and unrelated geometry is untouched.
    pub fn align_points(&self, other: &Mobject) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).align_path_points(&LayoutAnchor::from(other))
    }
}
