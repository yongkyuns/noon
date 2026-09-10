use super::*;
use crate::path_queries::{prepare_content, PathQuery};
use crate::{AuthoringError, UnsupportedAuthoringOperation};

impl LiveSession<'_> {
    /// Capture retained content and the effective affine transform from one
    /// coherent publication. Active content overrides are explicitly rejected.
    pub fn effective_path_query(&self, object: &Mobject) -> Result<PathQuery, LiveSessionError> {
        self.require_mobject(object)?;
        let store = self.store.borrow();
        let observed = self
            .session
            .effective_semantic_object(&store, object.node_id())?;
        if !observed.authored_content_layout_applicable() {
            return Err(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::EffectivePathRenderOverride,
            )
            .into());
        }
        let state = store
            .semantic_object_state_checked(object.node_id())
            .map_err(AuthoringError::from)?;
        let transform = crate::semantic_mobject::semantic_transform_with_effective_affine(
            state.transform,
            observed.object.transform,
        );
        prepare_content(&store, state.content, transform).map_err(Into::into)
    }
}
