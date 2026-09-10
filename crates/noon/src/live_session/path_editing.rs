use super::*;
use crate::path_editing::{
    corners_path, path_is_unchanged, path_replacement_state, path_transaction,
};

impl LiveSession<'_> {
    /// Replace persistent world-space corners at a coherent publication boundary.
    pub fn set_points_as_corners(
        &mut self,
        object: &Mobject,
        points: &[noon_core::Vec2],
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(object)?;
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let captured = self.capture_mobject_state(object)?;
        let before = object.state()?;
        let after = path_replacement_state(captured)?;
        let path = corners_path(points)?;
        let mut store = self.store.borrow_mut();
        if path_is_unchanged(&store, &before, &after, &path) {
            return Ok(());
        }
        store.with_geometry_path(path, |store, handle| {
            self.session
                .apply_semantic_transaction_at_root(
                    store,
                    self.root,
                    path_transaction(object.node_id(), &before, after, handle),
                )
                .map(|_| ())
                .map_err(LiveSessionError::from)
        })
    }
}

impl From<noon_core::GeometryResourceError> for LiveSessionError {
    fn from(error: noon_core::GeometryResourceError) -> Self {
        crate::AuthoringError::from(error).into()
    }
}
