use super::*;
use crate::path_editing::{publish_running_path_edits, PathEdit};

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

    /// Compatibility forwarding for the Scene-owned running subcurve operation.
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
    /// Replace persistent world-space corners at a coherent publication boundary.
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
}
