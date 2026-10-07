use super::*;
use crate::path_editing::{
    prepare_object_edit, publish_running_path_edits, require_running_path_resource_admission,
    PathEdit,
};

impl LiveSession<'_> {
    /// Apply a pointwise matrix through the same coherent path-edit publication
    /// used by cold authoring. Detached live target editors therefore remain
    /// runtime-local until their later Transform activation consumes them.
    pub fn apply_matrix(
        &mut self,
        object: &Mobject,
        values: &[f64],
        rows: usize,
        columns: usize,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(object)?;
        let captured = self.capture_mobject_state(object)?;
        let prepared = {
            let store = self.store.borrow();
            crate::matrix_authoring::prepare_apply_matrix(
                &store,
                object.node_id(),
                captured,
                values,
                rows,
                columns,
                (about_x, about_y),
            )
            .map_err(LiveSessionError::from)?
        };
        let Some(prepared) = prepared else {
            return Ok(());
        };
        self.publish_path_edits(prepared).map(|_| ())
    }

    /// Apply one pointwise matrix to all path leaves beneath the family in the
    /// active session's single prepared resource/publication transaction.
    pub fn apply_matrix_to_family(
        &mut self,
        family: &MobjectFamily,
        values: &[f64],
        rows: usize,
        columns: usize,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), LiveSessionError> {
        self.require_family(family)?;
        let prepared = {
            let store = self.store.borrow();
            crate::matrix_authoring::prepare_apply_matrix_family(
                &store,
                family,
                values,
                rows,
                columns,
                (about_x, about_y),
            )
            .map_err(LiveSessionError::from)?
        };
        let Some(prepared) = prepared else {
            return Ok(());
        };
        self.publish_path_edits(prepared).map(|_| ())
    }
    pub(crate) fn publish_path_edits(
        &mut self,
        prepared: crate::path_editing::PreparedPathEdits,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        publish_running_path_edits(self.store, self.root, self.session, prepared)
            .map_err(LiveSessionError::from)
    }

    /// Compatibility forwarding to the Scene-owned running subcurve operation.
    pub fn subcurve(
        &mut self,
        source: &Mobject,
        a: f64,
        b: f64,
    ) -> Result<Mobject, LiveSessionError> {
        crate::path_editing::publish_running_subcurve(
            self.store,
            self.root,
            self.session,
            source,
            a,
            b,
        )
        .map_err(LiveSessionError::from)
    }

    pub fn set_points_smoothly(
        &mut self,
        object: &Mobject,
        points: &[noon_core::Vec2],
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::SmoothCorners(points))
    }
    pub fn make_smooth(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::AnchorMode(true))
    }
    pub fn make_jagged(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::AnchorMode(false))
    }
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
    pub fn insert_n_curves(
        &mut self,
        object: &Mobject,
        additional: usize,
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Subdivide(additional))
    }
    pub fn reverse_direction(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Reverse)
    }
    /// Capture both operands from one coherent runtime publication; active
    /// render overrides use the normal capture rejection, never stale geometry.
    pub fn pointwise_become_partial(
        &mut self,
        object: &Mobject,
        source: &Mobject,
        a: f64,
        b: f64,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(source)?;
        let (a, b) = crate::path_editing::partial_interval(a, b)?;
        let source = self.capture_mobject_state(source)?;
        self.edit_path(
            object,
            PathEdit::Partial {
                source: &source,
                a,
                b,
            },
        )
    }
    fn edit_path(&mut self, object: &Mobject, edit: PathEdit<'_>) -> Result<(), LiveSessionError> {
        self.require_mobject(object)?;
        require_running_path_resource_admission(self.store, self.root, self.session)
            .map_err(LiveSessionError::from)?;
        let captured = self.capture_mobject_state(object)?;
        let store = self.store.borrow_mut();
        let Some(prepared) = prepare_object_edit(&store, object.node_id(), captured, edit)? else {
            return Ok(());
        };
        drop(store);
        self.publish_path_edits(prepared).map(|_| ())
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
        self.change_family_anchor_mode(family, true)
    }
    pub fn make_family_jagged(
        &mut self,
        family: &MobjectFamily,
    ) -> Result<(), crate::LiveSessionError> {
        self.change_family_anchor_mode(family, false)
    }
    fn change_family_anchor_mode(
        &mut self,
        family: &MobjectFamily,
        smooth: bool,
    ) -> Result<(), crate::LiveSessionError> {
        self.require_family(family)?;
        require_running_path_resource_admission(self.store, self.root, self.session)
            .map_err(crate::LiveSessionError::from)?;
        let nodes = self
            .store
            .borrow()
            .ordered_leaf_nodes(family.node_id())
            .map_err(crate::AuthoringError::from)?;
        let states = nodes
            .into_iter()
            .map(|node| {
                let object = Mobject::from_node(std::rc::Rc::clone(self.store), node)?;
                Ok((node, self.capture_mobject_state(&object)?))
            })
            .collect::<Result<Vec<_>, crate::LiveSessionError>>()?;
        let prepared = {
            let store = self.store.borrow();
            crate::path_smoothing::prepare_anchor_edits(&store, states, smooth)?
        };
        self.publish_path_edits(prepared).map(|_| ())
    }
}
