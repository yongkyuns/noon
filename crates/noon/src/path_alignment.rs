//! Pair alignment uses the same atomic retained-resource transaction as family edits.
use crate::{
    path_editing::{world_path, PreparedPathEdits},
    AuthoringError, Mobject,
};
use noon_core::{SemanticNodeId, SemanticObjectState, SemanticStore};

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
        prepare_alignment(&store, (self.node_id(), left), (other.node_id(), right))?.publish(
            &mut store,
            |store, transaction| {
                transaction
                    .apply(store)
                    .map(|_| ())
                    .map_err(AuthoringError::from)
            },
        )
    }
}
