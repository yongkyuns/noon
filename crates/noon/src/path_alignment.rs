//! Pair alignment uses the same atomic retained-resource transaction as family edits.
use crate::{
    path_editing::{world_path, PreparedPathEdits},
    AuthoringError, ExecutionSession, Mobject,
};
use noon_core::{SemanticNodeId, SemanticObjectState, SemanticStore};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn prepare_alignment(
    store: &SemanticStore,
    left: (SemanticNodeId, SemanticObjectState),
    right: (SemanticNodeId, SemanticObjectState),
) -> Result<PreparedPathEdits, AuthoringError> {
    let original_a = world_path(store, &left.1)?;
    let original_b = world_path(store, &right.1)?;
    let (a, b) =
        noon_geometry::align_paths(&original_a, &original_b).map_err(AuthoringError::PathQuery)?;
    let mut replacements = Vec::new();
    if a != original_a {
        replacements.push((left.0, left.1, a));
    }
    if b != original_b {
        replacements.push((right.0, right.1, b));
    }
    PreparedPathEdits::prepare(store, replacements)
}
pub fn publish_alignment(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    left: &Mobject,
    right: &Mobject,
) -> Result<(), AuthoringError> {
    if !Rc::ptr_eq(store, left.integration_store()) || !Rc::ptr_eq(store, right.integration_store())
    {
        return Err(AuthoringError::ForeignStore);
    }
    left.validate()?;
    right.validate()?;
    if left.node_id() == right.node_id() {
        return Ok(());
    }
    execution
        .require_resource_creation_at_root(&store.borrow(), root)
        .map_err(AuthoringError::from)?;
    let left = (
        left.node_id(),
        crate::effective_capture::capture_mobject_state(store, execution, left)?,
    );
    let right = (
        right.node_id(),
        crate::effective_capture::capture_mobject_state(store, execution, right)?,
    );
    let mut store = store.borrow_mut();
    prepare_alignment(&store, left, right)?.publish(&mut store, |store, transaction| {
        execution
            .apply_semantic_transaction_at_root(store, root, transaction)
            .map(|_| ())
            .map_err(AuthoringError::from)
    })?;
    Ok(())
}

impl Mobject {
    /// Match corresponding path contour/curve counts using exact subdivision.
    /// Both operands retain identity, paint and visible shape. Publication is
    /// atomic; aliases are a no-op and unrelated geometry is untouched.
    pub fn align_points(&self, other: &Mobject) -> Result<(), AuthoringError> {
        if !std::rc::Rc::ptr_eq(self.integration_store(), other.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        let left = self.state()?;
        let right = other.state()?;
        if self.node_id() == other.node_id() {
            return Ok(());
        }
        let mut store = self.integration_store().borrow_mut();
        prepare_alignment(&store, (self.node_id(), left), (other.node_id(), right))?
            .publish(&mut store, |store, transaction| {
                transaction.apply(store).map_err(AuthoringError::from)
            })?;
        Ok(())
    }
}
