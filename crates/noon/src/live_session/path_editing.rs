use super::*;
use crate::path_editing::{path_is_unchanged, path_replacement_state, path_transaction, PathEdit};

impl LiveSession<'_> {
    /// Replace persistent world-space corners at a coherent publication boundary.
    pub fn set_points_as_corners(
        &mut self,
        object: &Mobject,
        points: &[noon_core::Vec2],
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Corners(points))
    }
    pub fn start_new_path(
        &mut self,
        object: &Mobject,
        point: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Start(point))
    }
    pub fn add_line_to(
        &mut self,
        object: &Mobject,
        point: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Line(point))
    }
    pub fn add_quadratic_bezier_curve_to(
        &mut self,
        object: &Mobject,
        control: noon_core::Vec2,
        anchor: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Quadratic(control, anchor))
    }
    pub fn add_cubic_bezier_curve_to(
        &mut self,
        object: &Mobject,
        c1: noon_core::Vec2,
        c2: noon_core::Vec2,
        anchor: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Cubic(c1, c2, anchor))
    }
    pub fn close_path(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Close)
    }
    fn edit_path(&mut self, object: &Mobject, edit: PathEdit<'_>) -> Result<(), LiveSessionError> {
        self.require_mobject(object)?;
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let captured = self.capture_mobject_state(object)?;
        let before = object.state()?;
        let after = path_replacement_state(captured.clone())?;
        let mut store = self.store.borrow_mut();
        let path = edit.prepare(&store, &captured)?;
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
