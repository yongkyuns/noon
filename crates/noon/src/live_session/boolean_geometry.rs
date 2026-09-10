use super::*;
use crate::{BooleanOperation, ManimGeometryOptions};

impl LiveSession<'_> {
    /// Prepare an inert boolean constructor from one coherent publication. Bound
    /// operands observe current affine transforms; detached operands use authored
    /// state. Active content overrides are rejected instead of reading old paths.
    pub fn boolean_geometry_options(
        &self,
        operation: BooleanOperation,
        operands: &[Mobject],
    ) -> Result<ManimGeometryOptions, LiveSessionError> {
        let states = operands
            .iter()
            .map(|object| {
                self.require_mobject(object)?;
                let mut state = object.state()?;
                if self.session.semantic_object_is_reachable(object.node_id()) {
                    let store = self.store.borrow();
                    let observed = self
                        .session
                        .effective_semantic_object(&store, object.node_id())?;
                    if !observed.authored_content_layout_applicable() {
                        return Err(crate::AuthoringError::Unsupported(
                            crate::UnsupportedAuthoringOperation::EffectivePathRenderOverride,
                        )
                        .into());
                    }
                    state.transform =
                        crate::semantic_mobject::semantic_transform_with_effective_affine(
                            state.transform,
                            observed.object.transform,
                        );
                }
                Ok(state)
            })
            .collect::<Result<Vec<_>, LiveSessionError>>()?;
        crate::boolean_authoring::boolean_options(&self.store.borrow(), operation, &states)
            .map_err(Into::into)
    }
}
