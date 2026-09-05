use noon_core::{Camera2DState, RetainedFamilyAnimationPlan};
use noon_runtime::{FrameChanges, RetainedFamilyFrame, RetainedPlannedFamilyFrame};

use crate::{
    RetainedExecutionDeltaEncoder, RetainedExecutionTransportError,
    RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionTransportError,
};

/// Sequence-owning producer for the additive retained family execution envelope.
///
/// Ordinary retained object state stays encoded by [`RetainedExecutionDeltaEncoder`].
/// This owner only attaches the already-evaluated family state and immutable member
/// plans, so sequencing, slot identity, and snapshot rules remain exactly the same as
/// the base retained transport.
#[derive(Clone, Debug)]
pub struct RetainedFamilyExecutionDeltaEncoder {
    retained: RetainedExecutionDeltaEncoder,
}

impl RetainedFamilyExecutionDeltaEncoder {
    pub(crate) fn with_render_geometries(
        session: u32,
        geometries: std::sync::Arc<[std::sync::Arc<noon_core::GeometryRef>]>,
    ) -> Self {
        Self {
            retained: RetainedExecutionDeltaEncoder::with_render_geometries(session, geometries),
        }
    }
    pub const fn new(session: u32) -> Self {
        Self {
            retained: RetainedExecutionDeltaEncoder::new(session),
        }
    }

    pub fn encode_snapshot(
        &mut self,
        frame: &RetainedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        camera: Camera2DState,
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let retained = self.retained.encode_snapshot(frame.retained, camera)?;
        Ok(RetainedFamilyExecutionDeltaEnvelope::snapshot(
            retained, frame, plans,
        )?)
    }

    pub fn encode_planned_snapshot(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        camera: Camera2DState,
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let retained = self.retained.encode_snapshot(frame.retained, camera)?;
        Ok(RetainedFamilyExecutionDeltaEnvelope::planned_snapshot(
            retained, frame, plans,
        )?)
    }

    /// Encode one sparse family-aware retained update.
    ///
    /// `plans` are normally used only by the initial snapshot. They are supplied here
    /// as well because the base retained encoder may legitimately promote an
    /// `FrameChanges::all()` update to an authoritative snapshot.
    pub fn encode_incremental(
        &mut self,
        frame: &RetainedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        changes: &FrameChanges,
        camera: Camera2DState,
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, RetainedFamilyExecutionEncodeError>
    {
        let Some(retained) = self
            .retained
            .encode_incremental(frame.retained, changes, camera)?
        else {
            return Ok(None);
        };

        let envelope = if retained.snapshot {
            RetainedFamilyExecutionDeltaEnvelope::snapshot(retained, frame, plans)?
        } else {
            RetainedFamilyExecutionDeltaEnvelope::incremental(retained, frame, changes)?
        };
        Ok(Some(envelope))
    }

    pub fn encode_planned_incremental(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        changes: &FrameChanges,
        camera: Camera2DState,
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, RetainedFamilyExecutionEncodeError>
    {
        let Some(retained) = self
            .retained
            .encode_incremental(frame.retained, changes, camera)?
        else {
            return Ok(None);
        };

        let envelope = if retained.snapshot {
            RetainedFamilyExecutionDeltaEnvelope::planned_snapshot(retained, frame, plans)?
        } else {
            RetainedFamilyExecutionDeltaEnvelope::planned_incremental(retained, frame, changes)?
        };
        Ok(Some(envelope))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyExecutionEncodeError {
    Retained(RetainedExecutionTransportError),
    Family(RetainedFamilyExecutionTransportError),
}

impl std::fmt::Display for RetainedFamilyExecutionEncodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retained(error) => error.fmt(formatter),
            Self::Family(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedFamilyExecutionEncodeError {}

impl From<RetainedExecutionTransportError> for RetainedFamilyExecutionEncodeError {
    fn from(value: RetainedExecutionTransportError) -> Self {
        Self::Retained(value)
    }
}

impl From<RetainedFamilyExecutionTransportError> for RetainedFamilyExecutionEncodeError {
    fn from(value: RetainedFamilyExecutionTransportError) -> Self {
        Self::Family(value)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        FamilyAnimationMode, FamilyAnimationState, GeometryRef, ObjectContentRef, ObjectId,
        RateFunction, RetainedFamilyAnimationPlanBuilder, RetainedObjectDefinition, SemanticStore,
        Style, TextResourceArena, Transform2D,
    };
    use noon_runtime::{RetainedFrameObjectState, RetainedFrameState};

    use super::*;

    fn state(progress: f64) -> FamilyAnimationState {
        FamilyAnimationState {
            mode: FamilyAnimationMode::Reveal,
            overall_progress: progress,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        }
    }

    fn fixture() -> (
        RetainedFamilyAnimationPlan,
        RetainedFrameState,
        Vec<Option<FamilyAnimationState>>,
    ) {
        let first = RetainedObjectDefinition::geometry(ObjectId::new(10), GeometryRef::circle(1.0));
        let second =
            RetainedObjectDefinition::geometry(ObjectId::new(11), GeometryRef::circle(2.0));
        let mut semantics = SemanticStore::new();
        let first_leaf = semantics.insert_authoring_object();
        let second_leaf = semantics.insert_authoring_object();
        let family = semantics.insert_family();
        semantics.add_member(family, first_leaf).unwrap();
        semantics.add_member(family, second_leaf).unwrap();
        let texts = TextResourceArena::new();
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&semantics, family).unwrap();
        builder.accept_leaf(first_leaf, &first, &texts).unwrap();
        builder.accept_leaf(second_leaf, &second, &texts).unwrap();
        let plan = builder.finish().unwrap();

        let frame = RetainedFrameState {
            time: 0.5,
            objects: vec![
                RetainedFrameObjectState {
                    id: first.id,
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
                RetainedFrameObjectState {
                    id: second.id,
                    content: ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
            ],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
            render_transforms: vec![None, None],
        };
        (plan, frame, vec![Some(state(0.5)), Some(state(0.5))])
    }

    #[test]
    fn snapshot_and_incremental_share_base_sequence_and_sparse_family_indices() {
        let (plan, frame, states) = fixture();
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(17);

        let snapshot = encoder
            .encode_snapshot(
                &family,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();
        assert!(snapshot.retained.snapshot);
        assert_eq!(snapshot.retained.sequence, 0);
        assert_eq!(snapshot.family_plans.len(), 1);
        assert_eq!(snapshot.family_states.len(), 2);

        let incremental = encoder
            .encode_incremental(
                &family,
                std::slice::from_ref(&plan),
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert!(!incremental.retained.snapshot);
        assert_eq!(incremental.retained.sequence, 1);
        assert!(incremental.family_plans.is_empty());
        assert_eq!(incremental.family_states.len(), 1);
        assert_eq!(incremental.family_states[0].object, ObjectId::new(11));
    }

    #[test]
    fn planned_encoder_carries_sparse_plan_identity() {
        let (plan, frame, states) = fixture();
        let plan_indices = [Some(0), Some(0)];
        let family = RetainedPlannedFamilyFrame {
            retained: &frame,
            family_animations: &states,
            family_plan_indices: &plan_indices,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(19);
        let snapshot = encoder
            .encode_planned_snapshot(
                &family,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();
        assert_eq!(snapshot.family_states[0].family_plan_index, Some(0));
        assert_eq!(snapshot.family_states[1].family_plan_index, Some(0));

        let incremental = encoder
            .encode_planned_incremental(
                &family,
                std::slice::from_ref(&plan),
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(incremental.family_states.len(), 1);
        assert_eq!(incremental.family_states[0].family_plan_index, Some(0));
    }

    #[test]
    fn empty_incremental_does_not_consume_sequence() {
        let (plan, frame, states) = fixture();
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(23);
        encoder
            .encode_snapshot(
                &family,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();

        assert!(encoder
            .encode_incremental(
                &family,
                std::slice::from_ref(&plan),
                &FrameChanges::default(),
                Camera2DState::default(),
            )
            .unwrap()
            .is_none());
        let next = encoder
            .encode_incremental(
                &family,
                std::slice::from_ref(&plan),
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(next.retained.sequence, 1);
    }

    #[test]
    fn all_changes_promote_to_snapshot_and_reinstall_plan() {
        let (plan, frame, states) = fixture();
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(31);
        encoder
            .encode_snapshot(
                &family,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();

        let replacement = encoder
            .encode_incremental(
                &family,
                std::slice::from_ref(&plan),
                &FrameChanges::all(),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert!(replacement.retained.snapshot);
        assert_eq!(replacement.retained.sequence, 1);
        assert_eq!(replacement.family_plans.len(), 1);
        assert_eq!(replacement.family_states.len(), 2);
    }
}
