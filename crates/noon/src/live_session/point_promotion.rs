//! Coherent live promotion reuses shared copy/content/membership preparation.
use super::*;

impl LiveSession<'_> {
    /// Promote only the root's own points, preserving current child identities.
    /// Effective capture, path clearing and membership publish atomically.
    pub fn promote_own_points(
        &mut self,
        source: MobjectTarget<'_>,
        references: &[MobjectTarget<'_>],
    ) -> Result<crate::SemanticCopy<crate::CompositeEntryHandle>, LiveSessionError> {
        match source {
            MobjectTarget::Object(object) => self.require_mobject(object)?,
            MobjectTarget::Family(family) => self.require_family(family)?,
        }
        self.require_target_capture()?;
        let (prepared, pending) = crate::point_promotion::prepare_point_promotion(
            source,
            references,
            |object, graph_row| {
                crate::effective_capture::capture_mobject_state_with_graph_dependency(
                    self.store,
                    self.session,
                    object,
                    graph_row,
                )
                .map_err(LiveSessionError::from)
            },
            |store, source, target, transaction| {
                crate::effective_capture::stage_effect_copy(
                    store,
                    self.session,
                    source,
                    target,
                    transaction,
                )
                .map_err(LiveSessionError::from)
            },
        )?;
        let result = self.publish_path_edits(prepared)?;
        pending
            .resolve(&result, crate::CompositeEntryHandle::from_node)
            .map_err(LiveSessionError::from)
    }
}
