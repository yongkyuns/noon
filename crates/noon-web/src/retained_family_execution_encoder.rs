use noon_core::{
    Camera2DState, FontResourceLookup, GeometryResourceLookup, RetainedFamilyAnimationPlan,
    TextResourceHandle, TextResourceLookup,
};
use noon_runtime::{FrameChanges, RetainedFamilyFrame, RetainedPlannedFamilyFrame};

use std::collections::HashMap;

use crate::{
    RetainedExecutionDeltaEncoder, RetainedExecutionTransportError,
    RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionTransportError,
    RetainedResourceBundle, RetainedResourceInventory, RetainedResourceTransportError,
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
    plan_index_remap: HashMap<usize, u32>,
    published_plan_count: usize,
    observed_plan_count: usize,
    resources: RetainedResourceInventory,
}

#[derive(Debug)]
struct StagedPlanMappings {
    mappings: HashMap<usize, u32>,
    added_plan_indices: Vec<usize>,
    next_published_plan_count: usize,
}

impl RetainedFamilyExecutionDeltaEncoder {
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) const fn session(&self) -> u32 {
        self.retained.session()
    }

    pub fn new(session: u32) -> Self {
        Self {
            retained: RetainedExecutionDeltaEncoder::new(session),
            plan_index_remap: HashMap::new(),
            published_plan_count: 0,
            observed_plan_count: 0,
            resources: RetainedResourceInventory::default(),
        }
    }

    pub(crate) fn new_with_resources(session: u32, resources: &RetainedResourceBundle) -> Self {
        Self {
            retained: RetainedExecutionDeltaEncoder::new(session),
            plan_index_remap: HashMap::new(),
            published_plan_count: 0,
            observed_plan_count: 0,
            resources: resources.inventory(),
        }
    }

    pub(crate) fn attach_resource_additions(
        &mut self,
        envelope: &mut RetainedFamilyExecutionDeltaEnvelope,
        text_handles: impl IntoIterator<Item = TextResourceHandle>,
        texts: &(impl TextResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
    ) -> Result<(), RetainedResourceTransportError> {
        let new_texts = text_handles
            .into_iter()
            .filter(|handle| {
                !self.resources.contains_text(
                    crate::TransportTextResourceHandle::from_source_handle(*handle),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        if new_texts.is_empty() {
            return Ok(());
        }
        let mut additions = RetainedResourceBundle::capture_additions(
            new_texts,
            texts,
            geometries,
            fonts,
            &self.resources,
        )?;
        additions.retain_additions(&mut self.resources);
        debug_assert!(!additions.is_empty());
        envelope.resource_additions = Some(additions);
        Ok(())
    }

    pub fn encode_snapshot(
        &mut self,
        frame: &RetainedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        camera: Camera2DState,
    ) -> Result<RetainedFamilyExecutionDeltaEnvelope, RetainedFamilyExecutionEncodeError> {
        let retained = self.retained.encode_snapshot(frame.retained, camera)?;
        let envelope = RetainedFamilyExecutionDeltaEnvelope::snapshot(retained, frame, plans)?;
        self.plan_index_remap = (0..plans.len())
            .map(|index| (index, index as u32))
            .collect();
        self.published_plan_count = plans.len();
        self.observed_plan_count = plans.len();
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
        let staged = self.stage_plan_mappings(frame, plans, changes.object_indices())?;
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
            let added_plans = staged
                .added_plan_indices
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
        remap_family_state_indices(&mut envelope, &self.plan_index_remap, &staged.mappings)?;
        self.commit_plan_mappings(staged, plans.len());
        Ok(Some(envelope))
    }

    /// Encode one sparse family-aware update with a compact painter-order splice.
    /// Newly admitted rows may begin an active family animation in this same atomic
    /// delta; their plan descriptor is appended before their state references it.
    pub fn encode_planned_incremental_with_painter_order(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        changes: &FrameChanges,
        camera: Camera2DState,
        painter_order: &[u32],
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, RetainedFamilyExecutionEncodeError>
    {
        self.validate_plan_count(plans)?;
        let family_changes = FrameChanges::objects(changes.object_indices().to_vec());
        let staged = self.stage_plan_mappings(frame, plans, family_changes.object_indices())?;
        let Some(retained) = self.retained.encode_incremental_with_painter_order(
            frame.retained,
            changes,
            camera,
            painter_order,
        )?
        else {
            return Ok(None);
        };
        let added_plans = staged
            .added_plan_indices
            .iter()
            .map(|&index| plans[index].clone())
            .collect::<Vec<_>>();
        let mut envelope = RetainedFamilyExecutionDeltaEnvelope::planned_incremental_with_plans(
            retained,
            frame,
            &family_changes,
            &added_plans,
        )?;
        remap_family_state_indices(&mut envelope, &self.plan_index_remap, &staged.mappings)?;
        self.commit_plan_mappings(staged, plans.len());
        Ok(Some(envelope))
    }

    fn stage_plan_mappings(
        &self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        object_indices: &[usize],
    ) -> Result<StagedPlanMappings, RetainedFamilyExecutionTransportError> {
        let mut mappings = HashMap::new();
        let mut added_plan_indices = Vec::new();
        let mut next_published_plan_count = self.published_plan_count;
        for &object_index in object_indices {
            if frame.family_animation(object_index).is_none() {
                continue;
            }
            let object = &frame.retained.objects[object_index];
            let core_index = frame.family_plan_index(object_index).ok_or(
                RetainedFamilyExecutionTransportError::MissingPlanIndex(object.id),
            )? as usize;
            if core_index >= plans.len() {
                return Err(RetainedFamilyExecutionTransportError::InvalidPlanIndex {
                    object: object.id,
                    plan_index: core_index as u32,
                    plan_count: plans.len(),
                });
            }
            if self.plan_index_remap.contains_key(&core_index) || mappings.contains_key(&core_index)
            {
                continue;
            }
            mappings.insert(core_index, next_published_plan_count as u32);
            added_plan_indices.push(core_index);
            next_published_plan_count += 1;
        }
        Ok(StagedPlanMappings {
            mappings,
            added_plan_indices,
            next_published_plan_count,
        })
    }

    fn commit_plan_mappings(&mut self, staged: StagedPlanMappings, observed_plan_count: usize) {
        self.plan_index_remap.extend(staged.mappings);
        self.published_plan_count = staged.next_published_plan_count;
        self.observed_plan_count = observed_plan_count;
    }

    fn validate_plan_count(
        &self,
        plans: &[RetainedFamilyAnimationPlan],
    ) -> Result<(), RetainedFamilyExecutionTransportError> {
        if plans.len() < self.observed_plan_count {
            return Err(RetainedFamilyExecutionTransportError::PlanSetShrank {
                published: self.observed_plan_count,
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
        let mut remap = HashMap::new();
        for entry in &envelope.family_states {
            if entry.state.family_animation.is_none() {
                continue;
            }
            let core_index = entry.family_plan_index.ok_or(
                RetainedFamilyExecutionTransportError::MissingPlanIndex(entry.object),
            )? as usize;
            if core_index >= plans.len() {
                return Err(RetainedFamilyExecutionTransportError::InvalidPlanIndex {
                    object: entry.object,
                    plan_index: core_index as u32,
                    plan_count: plans.len(),
                }
                .into());
            }
            let next = remap.len() as u32;
            remap.entry(core_index).or_insert(next);
        }
        envelope.family_plans = remap
            .iter()
            .map(|(&index, &wire)| {
                (
                    wire,
                    crate::RetainedFamilyPlanTransport::from_plan(&plans[index]),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_values()
            .collect();
        remap_family_state_indices(&mut envelope, &remap, &HashMap::new())?;
        envelope.validate()?;
        self.plan_index_remap = remap;
        self.published_plan_count = self.plan_index_remap.len();
        self.observed_plan_count = plans.len();
        Ok(envelope)
    }
}

fn remap_family_state_indices(
    envelope: &mut RetainedFamilyExecutionDeltaEnvelope,
    remap: &HashMap<usize, u32>,
    staged: &HashMap<usize, u32>,
) -> Result<(), RetainedFamilyExecutionTransportError> {
    for entry in &mut envelope.family_states {
        let Some(core_index) = entry.family_plan_index else {
            continue;
        };
        entry.family_plan_index = Some(
            remap
                .get(&(core_index as usize))
                .or_else(|| staged.get(&(core_index as usize)))
                .copied()
                .ok_or(RetainedFamilyExecutionTransportError::MissingPlanIndex(
                    entry.object,
                ))?,
        );
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
        RateFunction, RetainedFamilyAnimationPlanBuilder, SemanticStore, Style, TextResourceArena,
        Transform2D,
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
        let first = noon_runtime::FrameObjectState {
            z_index: 0.0,
            id: ObjectId::new(10),
            content: noon_core::ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
            transform: noon_core::Transform2D::IDENTITY,
            style: noon_core::Style::default(),
            appearance: 1.0,
            text_bounds: None,
        };
        let second = noon_runtime::FrameObjectState {
            z_index: 0.0,
            id: ObjectId::new(11),
            content: noon_core::ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
            transform: noon_core::Transform2D::IDENTITY,
            style: noon_core::Style::default(),
            appearance: 1.0,
            text_bounds: None,
        };
        let mut semantics = SemanticStore::new();
        let first_leaf = semantics.insert_authoring_object();
        let second_leaf = semantics.insert_authoring_object();
        let family = semantics.insert_family();
        semantics.add_member(family, first_leaf).unwrap();
        semantics.add_member(family, second_leaf).unwrap();
        let texts = TextResourceArena::new();
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&semantics, family).unwrap();
        builder
            .accept_leaf(first_leaf, first.id, &first.content, &texts)
            .unwrap();
        builder
            .accept_leaf(second_leaf, second.id, &second.content, &texts)
            .unwrap();
        let plan = builder.finish().unwrap();

        let frame = FrameState {
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
            time: 0.5,
            objects: vec![
                FrameObjectState {
                    z_index: 0.0,
                    id: first.id,
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
                },
                FrameObjectState {
                    z_index: 0.0,
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
    fn sparse_new_rows_publish_active_family_state_with_appended_plan() {
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(24);
        let empty = FrameState {
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
            time: 0.0,
            objects: Vec::new(),
            presences: Vec::new(),
            reveals: Vec::new(),
            morphs: Vec::new(),
            render_geometries: Vec::new(),
            render_transforms: Vec::new(),
        };
        encoder
            .encode_planned_snapshot(
                &RetainedPlannedFamilyFrame {
                    retained: &empty,
                    family_animations: &[],
                    family_plan_indices: &[],
                },
                &[],
                Camera2DState::default(),
            )
            .unwrap();

        let (plan, frame, states) = fixture();
        let plan_indices = [Some(0), Some(0)];
        let delta = encoder
            .encode_planned_incremental_with_painter_order(
                &RetainedPlannedFamilyFrame {
                    retained: &frame,
                    family_animations: &states,
                    family_plan_indices: &plan_indices,
                },
                &[plan],
                &FrameChanges::with_structure(vec![0, 1], vec![0, 1], Vec::new())
                    .with_painter_order(0..2),
                Camera2DState::default(),
                &[0, 1],
            )
            .unwrap()
            .unwrap();

        assert!(!delta.retained.snapshot);
        assert_eq!(delta.retained.objects.len(), 2);
        assert_eq!(delta.family_plans.len(), 1);
        assert_eq!(delta.family_states.len(), 2);
        assert!(delta
            .family_states
            .iter()
            .all(|state| state.family_plan_index == Some(0)));
    }

    #[test]
    fn incremental_plan_mapping_stays_sparse_across_unpublished_history() {
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(25);
        let empty = FrameState {
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
            time: 0.0,
            objects: Vec::new(),
            presences: Vec::new(),
            reveals: Vec::new(),
            morphs: Vec::new(),
            render_geometries: Vec::new(),
            render_transforms: Vec::new(),
        };
        encoder
            .encode_planned_snapshot(
                &RetainedPlannedFamilyFrame {
                    retained: &empty,
                    family_animations: &[],
                    family_plan_indices: &[],
                },
                &[],
                Camera2DState::default(),
            )
            .unwrap();

        let (plan, frame, states) = fixture();
        let plans = vec![plan; 1_024];
        let plan_indices = [Some(1_023), Some(1_023)];
        let delta = encoder
            .encode_planned_incremental_with_painter_order(
                &RetainedPlannedFamilyFrame {
                    retained: &frame,
                    family_animations: &states,
                    family_plan_indices: &plan_indices,
                },
                &plans,
                &FrameChanges::with_structure(vec![0, 1], vec![0, 1], Vec::new())
                    .with_painter_order(0..2),
                Camera2DState::default(),
                &[0, 1],
            )
            .unwrap()
            .unwrap();

        assert_eq!(delta.family_plans.len(), 1);
        assert!(delta
            .family_states
            .iter()
            .all(|state| state.family_plan_index == Some(0)));
        assert_eq!(encoder.plan_index_remap.len(), 1);
        assert_eq!(encoder.published_plan_count, 1);
        assert_eq!(encoder.observed_plan_count, plans.len());
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
