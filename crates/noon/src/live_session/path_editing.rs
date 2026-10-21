use super::*;
use crate::path_editing::publish_running_path_edits;

impl LiveSession<'_> {
    /// Replace a family's own raw points through this coherent live publication.
    /// Its identity, children, style and drawing intent remain authoritative.
    pub fn set_family_points_f64(
        &mut self,
        family: &MobjectFamily,
        points: &[(f64, f64)],
    ) -> Result<(), LiveSessionError> {
        crate::family_points::publish_family_points(
            self.store,
            self.root,
            self.session,
            family,
            points,
        )?;
        Ok(())
    }

    /// Clear only a family's own points, retaining its children and identity.
    pub fn clear_family_points(&mut self, family: &MobjectFamily) -> Result<(), LiveSessionError> {
        crate::family_points::publish_clear_family_points(
            self.store,
            self.root,
            self.session,
            family,
        )?;
        Ok(())
    }

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
}

impl From<noon_core::GeometryResourceError> for LiveSessionError {
    fn from(error: noon_core::GeometryResourceError) -> Self {
        crate::AuthoringError::from(error).into()
    }
}
