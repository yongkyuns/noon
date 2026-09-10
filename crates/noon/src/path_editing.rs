//! Persistent vector edits publish new immutable content through one transaction.
use crate::{AuthoringError, Mobject};
use noon_core::{
    SemanticMutationTransaction, SemanticObjectContent, SemanticObjectState, SemanticStore,
    StoredGeometry, Vec2, VectorPath,
};

pub(crate) fn corners_path(points: &[Vec2]) -> Result<VectorPath, AuthoringError> {
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    // Manim has no complete curve when fewer than two corners are supplied.
    if points.len() < 2 {
        return Ok(VectorPath::new());
    }
    let mut path = VectorPath::new().move_to(points[0]);
    for point in &points[1..] {
        path = path.line_to(*point);
    }
    if points.first() == points.last() {
        path = path.close();
    }
    Ok(path)
}

pub(crate) fn path_replacement_state(
    mut state: SemanticObjectState,
) -> Result<SemanticObjectState, AuthoringError> {
    if !matches!(state.content, SemanticObjectContent::Geometry(_)) {
        return Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::PathEditContent,
        ));
    }
    // Input points are in world XY space. The semantic object retains its other
    // presentation and identity; its XY affine is now represented in the points.
    state.transform.translation.x = 0.;
    state.transform.translation.y = 0.;
    state.transform.scale.x = 1.;
    state.transform.scale.y = 1.;
    state.transform.rotation_z = 0.;
    Ok(state)
}

pub(crate) fn path_is_unchanged(
    store: &SemanticStore,
    before: &SemanticObjectState,
    after: &SemanticObjectState,
    path: &VectorPath,
) -> bool {
    before.transform == after.transform
        && crate::path_queries::content_path(store, before.content)
            .is_ok_and(|old| old.as_ref() == path)
}

pub(crate) fn path_transaction(
    object: noon_core::SemanticNodeId,
    before: &SemanticObjectState,
    mut after: SemanticObjectState,
    handle: noon_core::GeometryResourceHandle,
) -> SemanticMutationTransaction {
    after.content = StoredGeometry::Resource(handle).into();
    let mut transaction = SemanticMutationTransaction::new();
    crate::semantic_mobject::stage_state_changes(&mut transaction, object, before, &after);
    transaction
}

impl Mobject {
    /// Set a polyline from world-space corners, preserving identity and paint.
    /// Copies retain their previous immutable path; no temporary object is used.
    pub fn set_points_as_corners(&mut self, points: &[Vec2]) -> Result<(), AuthoringError> {
        let before = self.state()?;
        let after = path_replacement_state(before.clone())?;
        let path = corners_path(points)?;
        let mut store = self.integration_store().borrow_mut();
        if path_is_unchanged(&store, &before, &after, &path) {
            return Ok(());
        }
        store.with_geometry_path(path, |store, handle| {
            path_transaction(self.node_id(), &before, after, handle)
                .apply(store)
                .map(|_| ())
                .map_err(AuthoringError::from)
        })
    }
}
