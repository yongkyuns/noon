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
    plan_index_remap: Vec<Option<u32>>,
}

impl RetainedFamilyExecutionDeltaEncoder {
    pub(crate) fn with_render_geometries(
        session: u32,
        geometries: std::sync::Arc<[std::sync::Arc<noon_core::GeometryRef>]>,
    ) -> Self {
        Self {
            retained: RetainedExecutionDeltaEncoder::with_render_geometries(session, geometries),
            plan_index_remap: Vec::new(),
        }
    }
    pub const fn new(session: u32) -> Self {
        Self {
            retained: RetainedExecutionDeltaEncoder::new(session),
            plan_index_remap: Vec::new(),
        }
    }

    pub fn encode_snapshot(
        &mut self,
        frame: &RetainedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        camera: Camera2DState,
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let retained = self.retained.encode_snapshot(frame.retained, camera)?;
        let envelope = RetainedFamilyExecutionDeltaEnvelope::snapshot(retained, frame, plans)?;
        self.plan_index_remap = (0..plans.len()).map(|index| Some(index as u32)).collect();
        Ok(envelope)
    }

    pub fn encode_planned_snapshot(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        camera: Camera2DState,
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let retained = self.retained.encode_snapshot(frame.retained, camera)?;
        let envelope =
            RetainedFamilyExecutionDeltaEnvelope::planned_snapshot(retained, frame, plans)?;
        self.compact_planned_snapshot(envelope, plans)
    }

    /// Encode an authoritative snapshot for the execution rows that still own a
    /// live slot. The family sidecar uses the same exact row selection as the base
    /// retained envelope, so retired rows cannot reappear through plan state.
    pub fn encode_planned_snapshot_indices(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        camera: Camera2DState,
        indices: impl IntoIterator<Item = usize>,
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let indices = indices.into_iter().collect::<Vec<_>>();
        let retained = self.retained.encode_snapshot_indices(
            frame.retained,
            camera,
            indices.iter().copied(),
        )?;
        let envelope = RetainedFamilyExecutionDeltaEnvelope::planned_snapshot_indices(
            retained, frame, plans, indices,
        )?;
        self.compact_planned_snapshot(envelope, plans)
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
        self.validate_plan_count(plans)?;
        let Some(retained) = self
            .retained
            .encode_incremental(frame.retained, changes, camera)?
        else {
            return Ok(None);
        };

        let snapshot = retained.snapshot;
        let envelope = if snapshot {
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
        self.validate_plan_count(plans)?;
        let mut next_remap = self.plan_index_remap.clone();
        next_remap.resize(plans.len(), None);
        let mut added_plan_indices = Vec::new();
        for &object_index in changes.object_indices() {
            if frame.family_animation(object_index).is_none() {
                continue;
            }
            let object = &frame.retained.objects[object_index];
            let core_index = frame.family_plan_index(object_index).ok_or(
                RetainedFamilyExecutionTransportError::MissingPlanIndex(object.id),
            )? as usize;
            let next_wire_index = next_remap.iter().flatten().count() as u32;
            let Some(mapping) = next_remap.get_mut(core_index) else {
                return Err(RetainedFamilyExecutionTransportError::InvalidPlanIndex {
                    object: object.id,
                    plan_index: core_index as u32,
                    plan_count: plans.len(),
                }
                .into());
            };
            if mapping.is_none() {
                *mapping = Some(next_wire_index);
                added_plan_indices.push(core_index);
            }
        }
        let Some(retained) = self
            .retained
            .encode_incremental(frame.retained, changes, camera)?
        else {
            return Ok(None);
        };

        let snapshot = retained.snapshot;
        let mut envelope = if snapshot {
            let envelope =
                RetainedFamilyExecutionDeltaEnvelope::planned_snapshot(retained, frame, plans)?;
            return self.compact_planned_snapshot(envelope, plans).map(Some);
        } else {
            let added_plans = added_plan_indices
                .iter()
                .map(|&index| plans[index].clone())
                .collect::<Vec<_>>();
            RetainedFamilyExecutionDeltaEnvelope::planned_incremental_with_plans(
                retained,
                frame,
                changes,
                &added_plans,
            )?
        };
        remap_family_state_indices(&mut envelope, &next_remap)?;
        self.plan_index_remap = next_remap;
        Ok(Some(envelope))
    }

    fn validate_plan_count(
        &self,
        plans: &[RetainedFamilyAnimationPlan],
    ) -> Result<(), RetainedFamilyExecutionTransportError> {
        if plans.len() < self.plan_index_remap.len() {
            return Err(RetainedFamilyExecutionTransportError::PlanSetShrank {
                published: self.plan_index_remap.len(),
                available: plans.len(),
            });
        }
        Ok(())
    }

    fn compact_planned_snapshot(
        &mut self,
        mut envelope: RetainedFamilyExecutionDeltaEnvelope,
        plans: &[RetainedFamilyAnimationPlan],
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let mut remap = vec![None; plans.len()];
        for entry in &envelope.family_states {
            if entry.state.family_animation.is_none() {
                continue;
            }
            let core_index = entry.family_plan_index.ok_or(
                RetainedFamilyExecutionTransportError::MissingPlanIndex(entry.object),
            )? as usize;
            let next = remap.iter().flatten().count() as u32;
            let mapping = remap.get_mut(core_index).ok_or(
                RetainedFamilyExecutionTransportError::InvalidPlanIndex {
                    object: entry.object,
                    plan_index: core_index as u32,
                    plan_count: plans.len(),
                },
            )?;
            mapping.get_or_insert(next);
        }
        envelope.family_plans = remap
            .iter()
            .enumerate()
            .filter_map(|(index, wire)| {
                wire.map(|wire| {
                    (
                        wire,
                        crate::RetainedFamilyPlanTransport::from_plan(&plans[index]),
                    )
                })
            })
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_values()
            .collect();
        remap_family_state_indices(&mut envelope, &remap)?;
        envelope.validate()?;
        self.plan_index_remap = remap;
        Ok(envelope)
    }
}

fn remap_family_state_indices(
    envelope: &mut RetainedFamilyExecutionDeltaEnvelope,
    remap: &[Option<u32>],
) -> Result<(), RetainedFamilyExecutionTransportError> {
    for entry in &mut envelope.family_states {
        let Some(core_index) = entry.family_plan_index else {
            continue;
        };
        entry.family_plan_index = Some(remap.get(core_index as usize).copied().flatten().ok_or(
            RetainedFamilyExecutionTransportError::MissingPlanIndex(entry.object),
        )?);
    }
    Ok(())
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
    use noon_runtime::{FrameObjectState, FrameState};

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
        FrameState,
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

        let frame = FrameState {
            time: 0.5,
            objects: vec![
                FrameObjectState {
                    id: first.id,
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
                },
                FrameObjectState {
                    id: second.id,
                    content: ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
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
    fn planned_encoder_publishes_only_the_new_plan_suffix() {
        let (plan, frame, states) = fixture();
        let initial_indices = [Some(0), Some(0)];
        let initial = RetainedPlannedFamilyFrame {
            retained: &frame,
            family_animations: &states,
            family_plan_indices: &initial_indices,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(20);
        encoder
            .encode_planned_snapshot(
                &initial,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();

        let plans = [plan.clone(), plan];
        let appended_indices = [Some(1), Some(0)];
        let appended = RetainedPlannedFamilyFrame {
            retained: &frame,
            family_animations: &states,
            family_plan_indices: &appended_indices,
        };
        let delta = encoder
            .encode_planned_incremental(
                &appended,
                &plans,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert!(!delta.retained.snapshot);
        assert_eq!(delta.family_plans.len(), 1);
        assert_eq!(delta.family_states.len(), 1);
        assert_eq!(delta.family_states[0].family_plan_index, Some(1));
    }

    #[test]
    fn planned_snapshot_omits_inactive_historical_plan_descriptors() {
        let (plan, frame, _) = fixture();
        let states = [None, None];
        let plan_indices = [None, None];
        let inactive = RetainedPlannedFamilyFrame {
            retained: &frame,
            family_animations: &states,
            family_plan_indices: &plan_indices,
        };
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(21);
        let snapshot = encoder
            .encode_planned_snapshot(
                &inactive,
                std::slice::from_ref(&plan),
                Camera2DState::default(),
            )
            .unwrap();
        assert!(snapshot.family_plans.is_empty());
        assert!(snapshot
            .family_states
            .iter()
            .all(|entry| entry.family_plan_index.is_none()));
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
