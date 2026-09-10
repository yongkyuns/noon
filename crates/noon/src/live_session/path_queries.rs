use super::*;
use crate::path_queries::PathQuery;
use crate::{AuthoringError, UnsupportedAuthoringOperation};

impl LiveSession<'_> {
    /// Capture exact current path controls and transform from one coherent
    /// runtime publication, including the currently revealed prefix of a path.
    pub fn effective_path_query(&self, object: &Mobject) -> Result<PathQuery, LiveSessionError> {
        self.require_mobject(object)?;
        let store = self.store.borrow();
        let observed = self
            .session
            .effective_semantic_object(&store, object.node_id())?;
        let unsupported = || {
            AuthoringError::Unsupported(UnsupportedAuthoringOperation::EffectivePathRenderOverride)
        };
        let geometry = observed
            .render_geometry
            .or_else(|| observed.object.content.geometry())
            .ok_or(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::PathQueryContent,
            ))?;
        let mut path = noon_geometry::canonical_outline_path(geometry).ok_or(
            AuthoringError::Unsupported(UnsupportedAuthoringOperation::PathQueryContent),
        )?;
        if let Some(target) = path.morph_target() {
            // Retained morph meshes currently use flattened sample progress for
            // reveal. Do not report different curve-parameter geometry here.
            if observed.reveal != 1.0 {
                return Err(unsupported().into());
            }
            path = noon_geometry::interpolate_path_preserving_order(&path, target, observed.morph)
                .map_err(AuthoringError::MorphQuery)?;
        } else if observed.morph != 0.0 {
            return Err(unsupported().into());
        }
        if observed.reveal != 1.0 {
            path = noon_geometry::authored_partial_path(&path, 0.0, observed.reveal);
        }
        let state = store
            .semantic_object_state_checked(object.node_id())
            .map_err(AuthoringError::from)?;
        let transform = crate::semantic_mobject::semantic_transform_with_effective_affine(
            state.transform,
            observed
                .render_transform
                .unwrap_or(observed.object.transform),
        );
        PathQuery::prepare(&path, transform).map_err(Into::into)
    }
}
