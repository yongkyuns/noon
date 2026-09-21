use super::*;
use crate::path_editing::{publish_running_path_edits, PathEdit};

impl LiveSession<'_> {
    /// Compatibility forwarding to the Scene-owned running path edit authority.
    pub fn apply_matrix(
        &mut self,
        object: &Mobject,
        values: &[f64],
        rows: usize,
        columns: usize,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_apply_matrix(
            self.store,
            self.root,
            self.session,
            object,
            values,
            (rows, columns),
            (about_x, about_y),
        )
        .map_err(LiveSessionError::from)
    }

    pub(super) fn publish_path_edits(
        &mut self,
        prepared: crate::path_editing::PreparedPathEdits,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        publish_running_path_edits(self.store, self.root, self.session, prepared)
            .map_err(LiveSessionError::from)
    }

    /// Copy a selected interval from one coherent effective publication.
    /// This keeps its distinct node-creation/resource transaction until the
    /// dedicated subcurve migration slice.
    pub fn subcurve(
        &mut self,
        source: &Mobject,
        a: f64,
        b: f64,
    ) -> Result<Mobject, LiveSessionError> {
        self.require_mobject(source)?;
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let captured = self.capture_mobject_state(source)?;
        let mut store = self.store.borrow_mut();
        let (mut state, path) = crate::path_editing::prepare_subcurve(&store, &captured, a, b)?;
        let mut publish = |store: &mut noon_core::SemanticStore, state| {
            self.session
                .apply_semantic_transaction_at_root(
                    store,
                    self.root,
                    crate::path_editing::subcurve_creation(state),
                )
                .map_err(LiveSessionError::from)
        };
        let result = if let Some(path) = path {
            store.with_geometry_path(path, |store, handle| {
                state.content = noon_core::StoredGeometry::Resource(handle).into();
                publish(store, state)
            })?
        } else {
            publish(&mut store, state)?
        };
        let id = crate::path_editing::created_subcurve_id(&result);
        drop(store);
        Mobject::from_node(Rc::clone(self.store), id).map_err(LiveSessionError::from)
    }

    pub fn set_points_smoothly(
        &mut self,
        object: &Mobject,
        points: &[noon_core::Vec2],
    ) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::SmoothCorners(points),
        )
        .map_err(LiveSessionError::from)
    }

    pub fn make_smooth(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::AnchorMode(true),
        )
        .map_err(LiveSessionError::from)
    }

    pub fn make_jagged(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::AnchorMode(false),
        )
        .map_err(LiveSessionError::from)
    }

    pub fn set_points_as_corners(
        &mut self,
        object: &Mobject,
        points: &[noon_core::Vec2],
    ) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::Corners(points),
        )
        .map_err(LiveSessionError::from)
    }

    pub fn start_new_path(
        &mut self,
        object: &Mobject,
        point: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::Start(point),
        )
        .map_err(LiveSessionError::from)
    }

    pub fn add_line_to(
        &mut self,
        object: &Mobject,
        point: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::Line(point),
        )
        .map_err(LiveSessionError::from)
    }

    pub fn add_quadratic_bezier_curve_to(
        &mut self,
        object: &Mobject,
        control: noon_core::Vec2,
        anchor: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::Quadratic(control, anchor),
        )
        .map_err(LiveSessionError::from)
    }

    pub fn add_cubic_bezier_curve_to(
        &mut self,
        object: &Mobject,
        c1: noon_core::Vec2,
        c2: noon_core::Vec2,
        anchor: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::Cubic(c1, c2, anchor),
        )
        .map_err(LiveSessionError::from)
    }

    pub fn close_path(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::Close,
        )
        .map_err(LiveSessionError::from)
    }

    pub fn insert_n_curves(
        &mut self,
        object: &Mobject,
        additional: usize,
    ) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::Subdivide(additional),
        )
        .map_err(LiveSessionError::from)
    }

    pub fn reverse_direction(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_object_edit(
            self.store,
            self.root,
            self.session,
            object,
            PathEdit::Reverse,
        )
        .map_err(LiveSessionError::from)
    }

    pub fn pointwise_become_partial(
        &mut self,
        object: &Mobject,
        source: &Mobject,
        a: f64,
        b: f64,
    ) -> Result<(), LiveSessionError> {
        crate::path_editing::publish_running_pointwise_partial(
            self.store,
            self.root,
            self.session,
            object,
            source,
            a,
            b,
        )
        .map_err(LiveSessionError::from)
    }
}

impl From<noon_core::GeometryResourceError> for LiveSessionError {
    fn from(error: noon_core::GeometryResourceError) -> Self {
        crate::AuthoringError::from(error).into()
    }
}

impl crate::LiveSession<'_> {
    pub fn make_family_smooth(
        &mut self,
        family: &MobjectFamily,
    ) -> Result<(), crate::LiveSessionError> {
        crate::path_editing::publish_running_family_anchor_mode(
            self.store,
            self.root,
            self.session,
            family,
            true,
        )
        .map_err(crate::LiveSessionError::from)
    }

    pub fn make_family_jagged(
        &mut self,
        family: &MobjectFamily,
    ) -> Result<(), crate::LiveSessionError> {
        crate::path_editing::publish_running_family_anchor_mode(
            self.store,
            self.root,
            self.session,
            family,
            false,
        )
        .map_err(crate::LiveSessionError::from)
    }
}
