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
            |store, source, style| {
                crate::effective_capture::capture_family_style(store, self.session, source, style)
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

impl LiveSession<'_> {
    fn promotion_own_point_count(
        &self,
        target: MobjectTarget<'_>,
    ) -> Result<usize, LiveSessionError> {
        match target {
            MobjectTarget::Object(object) => {
                self.require_mobject(object)?;
                self.get_num_points(object)
            }
            MobjectTarget::Family(family) => {
                self.require_family(family)?;
                match family.own_object()? {
                    Some(object) => self.get_num_points(&object),
                    None => Ok(0),
                }
            }
        }
    }

    /// Align default own-point states using the current coherent execution.
    /// Frontend-specific predicate/copy/reset/add overrides require their host protocol.
    pub fn align_own_points(
        &mut self,
        left: MobjectTarget<'_>,
        right: MobjectTarget<'_>,
        left_references: &[MobjectTarget<'_>],
        right_references: &[MobjectTarget<'_>],
    ) -> Result<Option<crate::PointAlignment>, LiveSessionError> {
        self.require_target_capture()?;
        let side = crate::point_promotion::alignment_side(
            self.promotion_own_point_count(left)?,
            self.promotion_own_point_count(right)?,
        );
        let Some(side) = side else {
            return Ok(None);
        };
        let copied = match side {
            crate::PointAlignmentSide::Left => self.promote_own_points(left, left_references)?,
            crate::PointAlignmentSide::Right => self.promote_own_points(right, right_references)?,
        };
        Ok(Some(crate::PointAlignment { side, copied }))
    }
}
