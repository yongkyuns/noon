use std::collections::{BTreeSet, HashMap, HashSet};

use noon_core::{FamilyAnimationState, ObjectId, RetainedFamilyAnimationPlan, TextResourceLookup};
use noon_runtime::{FrameChanges, FrameState, RetainedFamilyFrame, RetainedPlannedFamilyFrame};
use serde::{Deserialize, Serialize};

use crate::{
    RetainedExecutionDeltaEnvelope, RetainedFamilyPlanTransport, RetainedFamilyTransportError,
    RetainedFamilyTransportState, RetainedResourceBundle,
};

pub(crate) type ValidatedFamilyStateUpdate = (usize, Option<FamilyAnimationState>, Option<u32>);

/// Sparse per-object family scheduler state carried alongside an ordinary retained delta.
///
/// `family_plan_index` identifies the immutable snapshot-installed plan that owns an
/// active state. It is omitted for inactive states and remains optional on decode so
/// pre-multi-plan single-family snapshots remain readable.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedFamilyExecutionObjectState {
    pub object: ObjectId,
    #[serde(flatten)]
    pub state: RetainedFamilyTransportState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_plan_index: Option<u32>,
}

impl RetainedFamilyExecutionObjectState {
    /// Compatibility constructor for the original single-plan transport shape.
    pub fn new(
        object: ObjectId,
        family_animation: Option<FamilyAnimationState>,
    ) -> Result<Self, RetainedFamilyExecutionTransportError> {
        Ok(Self {
            object,
            state: RetainedFamilyTransportState::new(family_animation)?,
            family_plan_index: None,
        })
    }

    /// Construct an explicitly planned state for plural family execution.
    pub fn planned(
        object: ObjectId,
        family_animation: Option<FamilyAnimationState>,
        family_plan_index: Option<u32>,
    ) -> Result<Self, RetainedFamilyExecutionTransportError> {
        let result = Self {
            object,
            state: RetainedFamilyTransportState::new(family_animation)?,
            family_plan_index,
        };
        result.validate_plan_identity()?;
        Ok(result)
    }

    fn validate_plan_identity(&self) -> Result<(), RetainedFamilyExecutionTransportError> {
        if self.state.family_animation.is_none() && self.family_plan_index.is_some() {
            return Err(RetainedFamilyExecutionTransportError::PlanIndexWithoutState(self.object));
        }
        Ok(())
    }
}

/// Additive family-animation envelope over the stable retained execution transport.
///
/// Family-aware producers add sparse evaluated scheduler state plus immutable plan
/// descriptors. Snapshots replace the plan set; incrementals append newly compiled
/// plans in publication order. Plan indices therefore stay stable for the session,
/// while glyph IDs and renderer payloads remain renderer-local.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedFamilyExecutionDeltaEnvelope {
    #[serde(flatten)]
    pub retained: RetainedExecutionDeltaEnvelope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub family_states: Vec<RetainedFamilyExecutionObjectState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub family_plans: Vec<RetainedFamilyPlanTransport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_additions: Option<RetainedResourceBundle>,
}

impl RetainedFamilyExecutionDeltaEnvelope {
    /// Compatibility snapshot for one family plan.
    pub fn snapshot(
        retained: RetainedExecutionDeltaEnvelope,
        frame: &RetainedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
    ) -> Result<Self, RetainedFamilyExecutionTransportError> {
        if !retained.snapshot {
            return Err(RetainedFamilyExecutionTransportError::ExpectedSnapshot);
        }
        let envelope = Self {
            family_states: family_states_for_indices(frame, 0..frame.retained.objects.len())?,
            family_plans: plans
                .iter()
                .map(RetainedFamilyPlanTransport::from_plan)
                .collect(),
            resource_additions: None,
            retained,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    /// Snapshot with explicit active-plan ownership for plural family execution.
    pub fn planned_snapshot(
        retained: RetainedExecutionDeltaEnvelope,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
    ) -> Result<Self, RetainedFamilyExecutionTransportError> {
        Self::planned_snapshot_indices(retained, frame, plans, 0..frame.retained.objects.len())
    }

    /// Snapshot with explicit active-plan ownership for the selected live rows.
    pub fn planned_snapshot_indices(
        retained: RetainedExecutionDeltaEnvelope,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        indices: impl IntoIterator<Item = usize>,
    ) -> Result<Self, RetainedFamilyExecutionTransportError> {
        if !retained.snapshot {
            return Err(RetainedFamilyExecutionTransportError::ExpectedSnapshot);
        }
        let envelope = Self {
            family_states: planned_family_states_for_indices(frame, indices)?,
            family_plans: plans
                .iter()
                .map(RetainedFamilyPlanTransport::from_plan)
                .collect(),
            resource_additions: None,
            retained,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    /// Compatibility incremental for one family plan.
    pub fn incremental(
        retained: RetainedExecutionDeltaEnvelope,
        frame: &RetainedFamilyFrame<'_>,
        changes: &FrameChanges,
    ) -> Result<Self, RetainedFamilyExecutionTransportError> {
        if retained.snapshot {
            return Err(RetainedFamilyExecutionTransportError::ExpectedIncremental);
        }
        let envelope = Self {
            family_states: family_states_for_indices(
                frame,
                changes.object_indices().iter().copied(),
            )?,
            family_plans: Vec::new(),
            resource_additions: None,
            retained,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    /// Sparse plural-family incremental. Plan identity is sent only for changed objects.
    pub fn planned_incremental(
        retained: RetainedExecutionDeltaEnvelope,
        frame: &RetainedPlannedFamilyFrame<'_>,
        changes: &FrameChanges,
    ) -> Result<Self, RetainedFamilyExecutionTransportError> {
        Self::planned_incremental_with_plans(retained, frame, changes, &[])
    }

    /// Sparse plural-family incremental with an append-only plan suffix.
    pub fn planned_incremental_with_plans(
        retained: RetainedExecutionDeltaEnvelope,
        frame: &RetainedPlannedFamilyFrame<'_>,
        changes: &FrameChanges,
        added_plans: &[RetainedFamilyAnimationPlan],
    ) -> Result<Self, RetainedFamilyExecutionTransportError> {
        if retained.snapshot {
            return Err(RetainedFamilyExecutionTransportError::ExpectedIncremental);
        }
        let envelope = Self {
            family_states: planned_family_states_for_indices(
                frame,
                changes.object_indices().iter().copied(),
            )?,
            family_plans: added_plans
                .iter()
                .map(RetainedFamilyPlanTransport::from_plan)
                .collect(),
            resource_additions: None,
            retained,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn validate(&self) -> Result<(), RetainedFamilyExecutionTransportError> {
        let mut seen = HashSet::with_capacity(self.family_states.len());
        for entry in &self.family_states {
            if !seen.insert(entry.object) {
                return Err(RetainedFamilyExecutionTransportError::DuplicateStateObject(
                    entry.object,
                ));
            }
            entry.state.validate()?;
            entry.validate_plan_identity()?;
        }
        for plan in &self.family_plans {
            plan.validate()?;
        }
        Ok(())
    }
}

fn family_states_for_indices(
    frame: &RetainedFamilyFrame<'_>,
    indices: impl IntoIterator<Item = usize>,
) -> Result<Vec<RetainedFamilyExecutionObjectState>, RetainedFamilyExecutionTransportError> {
    if frame.family_animations.len() != frame.retained.objects.len() {
        return Err(RetainedFamilyExecutionTransportError::FrameShapeMismatch);
    }
    indices
        .into_iter()
        .map(|index| {
            let object = frame.retained.objects.get(index).ok_or(
                RetainedFamilyExecutionTransportError::InvalidObjectIndex(index),
            )?;
            RetainedFamilyExecutionObjectState::new(object.id, frame.family_animation(index))
        })
        .collect()
}

fn planned_family_states_for_indices(
    frame: &RetainedPlannedFamilyFrame<'_>,
    indices: impl IntoIterator<Item = usize>,
) -> Result<Vec<RetainedFamilyExecutionObjectState>, RetainedFamilyExecutionTransportError> {
    if frame.family_animations.len() != frame.retained.objects.len()
        || frame.family_plan_indices.len() != frame.retained.objects.len()
    {
        return Err(RetainedFamilyExecutionTransportError::FrameShapeMismatch);
    }
    indices
        .into_iter()
        .map(|index| {
            let object = frame.retained.objects.get(index).ok_or(
                RetainedFamilyExecutionTransportError::InvalidObjectIndex(index),
            )?;
            RetainedFamilyExecutionObjectState::planned(
                object.id,
                frame.family_animation(index),
                frame.family_plan_index(index),
            )
        })
        .collect()
}

/// Renderer-side family state paired with a resolved retained execution frame.
///
/// Snapshots replace plans/state only after the complete replacement validates.
/// Incrementals likewise validate all sparse updates before mutating live state.
#[derive(Clone, Debug, Default)]
pub struct InstalledRetainedFamilyExecutionState {
    states: Vec<Option<FamilyAnimationState>>,
    plan_indices: Vec<Option<u32>>,
    plans: Vec<RetainedFamilyAnimationPlan>,
    plan_objects: Vec<HashSet<ObjectId>>,
    active_indices: BTreeSet<usize>,
    initialized: bool,
}

pub(crate) enum PreparedInstalledFamilyUpdate {
    Snapshot(InstalledRetainedFamilyExecutionState),
    Incremental {
        frame_len: usize,
        added_plans: Vec<RetainedFamilyAnimationPlan>,
        added_plan_objects: Vec<HashSet<ObjectId>>,
        updates: Vec<ValidatedFamilyStateUpdate>,
    },
}

impl InstalledRetainedFamilyExecutionState {
    pub fn apply(
        &mut self,
        delta: &RetainedFamilyExecutionDeltaEnvelope,
        frame: &FrameState,
        texts: &(impl TextResourceLookup + ?Sized),
    ) -> Result<(), RetainedFamilyExecutionTransportError> {
        let object_indices = frame
            .objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.id, index))
            .collect::<HashMap<_, _>>();
        let prepared = self.prepare_with_lookup(
            delta,
            frame.objects.len(),
            texts,
            |object| object_indices.get(&object).copied(),
            |object| {
                object_indices
                    .get(&object)
                    .map(|&index| &frame.objects[index])
            },
        )?;
        self.commit_prepared(prepared);
        Ok(())
    }

    pub(crate) fn prepare_with_lookup<'a>(
        &self,
        delta: &RetainedFamilyExecutionDeltaEnvelope,
        frame_len: usize,
        texts: &(impl TextResourceLookup + ?Sized),
        mut index_for_object: impl FnMut(ObjectId) -> Option<usize>,
        mut object_for_id: impl FnMut(ObjectId) -> Option<&'a noon_runtime::FrameObjectState>,
    ) -> Result<PreparedInstalledFamilyUpdate, RetainedFamilyExecutionTransportError> {
        delta.validate()?;
        if !delta.retained.snapshot && !self.initialized {
            return Err(RetainedFamilyExecutionTransportError::IncrementalBeforeSnapshot);
        }
        if !delta.retained.snapshot
            && (self.states.len() > frame_len || self.plan_indices.len() > frame_len)
        {
            return Err(RetainedFamilyExecutionTransportError::FrameShapeMismatch);
        }

        let added_plans = delta
            .family_plans
            .iter()
            .map(|plan| plan.install_with_object_lookup(texts, &mut object_for_id))
            .collect::<Result<Vec<_>, _>>()?;
        let added_plan_objects = delta
            .family_plans
            .iter()
            .map(|plan| {
                plan.bindings
                    .iter()
                    .map(|binding| binding.object)
                    .collect::<HashSet<_>>()
            })
            .collect::<Vec<_>>();

        if delta.retained.snapshot {
            let mut next = Self {
                states: vec![None; frame_len],
                plan_indices: vec![None; frame_len],
                plans: added_plans,
                plan_objects: added_plan_objects,
                active_indices: BTreeSet::new(),
                initialized: true,
            };
            let updates = validated_state_updates(
                &delta.family_states,
                frame_len,
                next.plans.len(),
                &mut index_for_object,
                |plan_index, object| {
                    next.plan_objects
                        .get(plan_index as usize)
                        .map(|objects| objects.contains(&object))
                },
            )?;
            next.apply_updates(updates);
            return Ok(PreparedInstalledFamilyUpdate::Snapshot(next));
        }

        let installed_count = self.plans.len();
        let plan_count = installed_count + added_plans.len();
        let updates = validated_state_updates(
            &delta.family_states,
            frame_len,
            plan_count,
            &mut index_for_object,
            |plan_index, object| {
                let index = plan_index as usize;
                if index < installed_count {
                    self.plan_objects
                        .get(index)
                        .map(|objects| objects.contains(&object))
                } else {
                    added_plan_objects
                        .get(index - installed_count)
                        .map(|objects| objects.contains(&object))
                }
            },
        )?;
        Ok(PreparedInstalledFamilyUpdate::Incremental {
            frame_len,
            added_plans,
            added_plan_objects,
            updates,
        })
    }

    pub(crate) fn commit_prepared(&mut self, prepared: PreparedInstalledFamilyUpdate) {
        match prepared {
            PreparedInstalledFamilyUpdate::Snapshot(next) => *self = next,
            PreparedInstalledFamilyUpdate::Incremental {
                frame_len,
                added_plans,
                added_plan_objects,
                updates,
            } => {
                self.states.resize(frame_len, None);
                self.plan_indices.resize(frame_len, None);
                self.plans.extend(added_plans);
                self.plan_objects.extend(added_plan_objects);
                self.apply_updates(updates);
            }
        }
    }

    fn apply_updates(&mut self, updates: Vec<ValidatedFamilyStateUpdate>) {
        for (index, state, plan_index) in updates {
            self.states[index] = state;
            self.plan_indices[index] = plan_index;
            if state.is_some() {
                self.active_indices.insert(index);
            } else {
                self.active_indices.remove(&index);
            }
        }
    }

    pub fn frame<'a>(
        &'a self,
        retained: &'a FrameState,
    ) -> Result<RetainedFamilyFrame<'a>, RetainedFamilyExecutionTransportError> {
        self.validate_frame_shape(retained)?;
        Ok(RetainedFamilyFrame {
            retained,
            family_animations: &self.states,
        })
    }

    pub fn planned_frame<'a>(
        &'a self,
        retained: &'a FrameState,
    ) -> Result<RetainedPlannedFamilyFrame<'a>, RetainedFamilyExecutionTransportError> {
        self.validate_frame_shape(retained)?;
        Ok(RetainedPlannedFamilyFrame {
            retained,
            family_animations: &self.states,
            family_plan_indices: &self.plan_indices,
        })
    }

    pub fn plans(&self) -> &[RetainedFamilyAnimationPlan] {
        &self.plans
    }

    pub fn active_indices(&self) -> &BTreeSet<usize> {
        &self.active_indices
    }

    /// Legacy convenience for callers that deliberately operate on one plan only.
    pub fn single_plan(
        &self,
    ) -> Result<Option<&RetainedFamilyAnimationPlan>, RetainedFamilyExecutionTransportError> {
        match self.plans.as_slice() {
            [] => Ok(None),
            [plan] => Ok(Some(plan)),
            plans => {
                Err(RetainedFamilyExecutionTransportError::MultiplePlansUnsupported(plans.len()))
            }
        }
    }

    fn validate_frame_shape(
        &self,
        retained: &FrameState,
    ) -> Result<(), RetainedFamilyExecutionTransportError> {
        if !self.initialized {
            return Err(RetainedFamilyExecutionTransportError::MissingSnapshot);
        }
        if self.states.len() != retained.objects.len()
            || self.plan_indices.len() != retained.objects.len()
        {
            return Err(RetainedFamilyExecutionTransportError::FrameShapeMismatch);
        }
        Ok(())
    }
}

fn validated_state_updates(
    entries: &[RetainedFamilyExecutionObjectState],
    frame_len: usize,
    plan_count: usize,
    mut index_for_object: impl FnMut(ObjectId) -> Option<usize>,
    mut plan_contains_object: impl FnMut(u32, ObjectId) -> Option<bool>,
) -> Result<Vec<ValidatedFamilyStateUpdate>, RetainedFamilyExecutionTransportError> {
    entries
        .iter()
        .map(|entry| {
            let index = index_for_object(entry.object).ok_or(
                RetainedFamilyExecutionTransportError::UnknownObject(entry.object),
            )?;
            if index >= frame_len {
                return Err(RetainedFamilyExecutionTransportError::InvalidObjectIndex(
                    index,
                ));
            }
            let state = entry.state.family_animation;
            let plan_index = match (state, entry.family_plan_index) {
                (None, _) => None,
                (Some(_), Some(plan_index)) => Some(plan_index),
                (Some(_), None) if plan_count == 1 => Some(0),
                (Some(_), None) => {
                    return Err(RetainedFamilyExecutionTransportError::MissingPlanIndex(
                        entry.object,
                    ));
                }
            };

            if let Some(plan_index) = plan_index {
                let owns_object = plan_contains_object(plan_index, entry.object).ok_or(
                    RetainedFamilyExecutionTransportError::InvalidPlanIndex {
                        object: entry.object,
                        plan_index,
                        plan_count,
                    },
                )?;
                if !owns_object {
                    return Err(
                        RetainedFamilyExecutionTransportError::PlanDoesNotOwnObject {
                            object: entry.object,
                            plan_index,
                        },
                    );
                }
            } else if state.is_some() {
                return Err(RetainedFamilyExecutionTransportError::StateWithoutPlan(
                    entry.object,
                ));
            }
            Ok((index, state, plan_index))
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyExecutionTransportError {
    Family(RetainedFamilyTransportError),
    ExpectedSnapshot,
    ExpectedIncremental,
    IncrementalBeforeSnapshot,
    MissingSnapshot,
    FrameShapeMismatch,
    PlanSetShrank {
        published: usize,
        available: usize,
    },
    InvalidObjectIndex(usize),
    DuplicateStateObject(ObjectId),
    UnknownObject(ObjectId),
    StateWithoutPlan(ObjectId),
    PlanIndexWithoutState(ObjectId),
    MissingPlanIndex(ObjectId),
    InvalidPlanIndex {
        object: ObjectId,
        plan_index: u32,
        plan_count: usize,
    },
    PlanDoesNotOwnObject {
        object: ObjectId,
        plan_index: u32,
    },
    MultiplePlansUnsupported(usize),
}

impl std::fmt::Display for RetainedFamilyExecutionTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Family(error) => error.fmt(formatter),
            Self::ExpectedSnapshot => formatter
                .write_str("family execution snapshot requires a retained snapshot envelope"),
            Self::ExpectedIncremental => formatter
                .write_str("family execution incremental requires a retained incremental envelope"),
            Self::IncrementalBeforeSnapshot => formatter
                .write_str("retained family execution requires a snapshot before incrementals"),
            Self::MissingSnapshot => {
                formatter.write_str("retained family execution has no installed snapshot")
            }
            Self::FrameShapeMismatch => formatter
                .write_str("retained family state shape does not match retained frame objects"),
            Self::PlanSetShrank {
                published,
                available,
            } => write!(
                formatter,
                "retained family plan set shrank from {published} published plans to {available}"
            ),
            Self::InvalidObjectIndex(index) => {
                write!(formatter, "invalid retained family object index {index}")
            }
            Self::DuplicateStateObject(object) => write!(
                formatter,
                "retained family delta repeats object {}",
                object.get()
            ),
            Self::UnknownObject(object) => write!(
                formatter,
                "retained family delta references unknown object {}",
                object.get()
            ),
            Self::StateWithoutPlan(object) => write!(
                formatter,
                "retained family state for object {} has no installed member plan",
                object.get()
            ),
            Self::PlanIndexWithoutState(object) => write!(
                formatter,
                "retained family object {} has a plan index without an active state",
                object.get()
            ),
            Self::MissingPlanIndex(object) => write!(
                formatter,
                "active retained family state for object {} does not identify an installed plan",
                object.get()
            ),
            Self::InvalidPlanIndex {
                object,
                plan_index,
                plan_count,
            } => write!(
                formatter,
                "retained family object {} references plan index {plan_index}, but only {plan_count} plans are installed",
                object.get()
            ),
            Self::PlanDoesNotOwnObject { object, plan_index } => write!(
                formatter,
                "retained family plan {plan_index} does not own object {}",
                object.get()
            ),
            Self::MultiplePlansUnsupported(count) => write!(
                formatter,
                "single-plan compatibility helper received {count} retained family plans"
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyExecutionTransportError {}

impl From<RetainedFamilyTransportError> for RetainedFamilyExecutionTransportError {
    fn from(value: RetainedFamilyTransportError) -> Self {
        Self::Family(value)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use noon_core::{
        Camera2DState, FamilyAnimationMode, GeometryRef, ObjectContentRef, RateFunction, Style,
        TextResourceArena, Transform2D,
    };
    use noon_runtime::FrameObjectState;

    use super::*;
    use crate::{
        RetainedTransportObjectState, TransportObjectContent, TransportSlotId,
        RETAINED_EXECUTION_TRANSPORT_CHANNEL, RETAINED_EXECUTION_TRANSPORT_VERSION,
    };

    fn family_state(progress: f64) -> FamilyAnimationState {
        FamilyAnimationState {
            mode: FamilyAnimationMode::Reveal,
            overall_progress: progress,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        }
    }

    fn frame() -> FrameState {
        FrameState {
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
            time: 0.0,
            objects: vec![FrameObjectState {
                id: ObjectId::new(7),
                content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
                text_bounds: None,
            }],
            presences: vec![true],
            reveals: vec![1.0],
            morphs: vec![0.0],
            render_geometries: vec![None],
            render_transforms: vec![None],
        }
    }

    fn retained(snapshot: bool, sequence: u64) -> RetainedExecutionDeltaEnvelope {
        RetainedExecutionDeltaEnvelope {
            channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: RETAINED_EXECUTION_TRANSPORT_VERSION,
            session: 1,
            sequence,
            snapshot,
            time: 0.0,
            camera: Camera2DState::default(),
            objects: vec![RetainedTransportObjectState {
                slot: TransportSlotId {
                    slot: 0,
                    generation: 0,
                },
                order: 0,
                object: ObjectId::new(7),
                content: TransportObjectContent::Geometry {
                    geometry: GeometryRef::circle(1.0),
                },
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
                text_bounds: None,
                presence: true,
                reveal: 1.0,
                morph: 0.0,
                render_geometry: None,
                render_transform: None,
                render_geometry_resource: None,
            }],
            removed_slots: Vec::new(),
            painter_order: None,
        }
    }

    fn geometry_plan() -> RetainedFamilyAnimationPlan {
        let object = noon_runtime::FrameObjectState {
            id: ObjectId::new(7),
            content: noon_core::ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
            transform: noon_core::Transform2D::IDENTITY,
            style: noon_core::Style::default(),
            appearance: 1.0,
            text_bounds: None,
        };
        let mut semantics = noon_core::SemanticStore::new();
        let leaf = semantics.insert_authoring_object();
        let mut builder =
            noon_core::RetainedFamilyAnimationPlanBuilder::begin(&semantics, leaf).unwrap();
        builder
            .accept_leaf(leaf, object.id, &object.content, &TextResourceArena::new())
            .unwrap();
        builder.finish().unwrap()
    }

    fn family_snapshot(progress: f64) -> RetainedFamilyExecutionDeltaEnvelope {
        let retained_frame = frame();
        let states = [Some(family_state(progress))];
        let family_frame = RetainedFamilyFrame {
            retained: &retained_frame,
            family_animations: &states,
        };
        RetainedFamilyExecutionDeltaEnvelope::snapshot(
            retained(true, 0),
            &family_frame,
            std::slice::from_ref(&geometry_plan()),
        )
        .unwrap()
    }

    #[test]
    fn ordinary_retained_json_decodes_with_empty_family_sidecar() {
        let json = serde_json::to_string(&retained(true, 0)).unwrap();
        let decoded: RetainedFamilyExecutionDeltaEnvelope = serde_json::from_str(&json).unwrap();
        assert!(decoded.family_states.is_empty());
        assert!(decoded.family_plans.is_empty());
        decoded.validate().unwrap();
    }

    #[test]
    fn snapshot_installs_plan_and_incremental_none_clears_state() {
        let retained_frame = frame();
        let snapshot = family_snapshot(0.5);
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("glyph"));

        let mut installed = InstalledRetainedFamilyExecutionState::default();
        installed
            .apply(&snapshot, &retained_frame, &TextResourceArena::new())
            .unwrap();
        assert_eq!(
            installed
                .frame(&retained_frame)
                .unwrap()
                .family_animation(0),
            Some(family_state(0.5))
        );
        assert_eq!(
            installed
                .planned_frame(&retained_frame)
                .unwrap()
                .family_plan_index(0),
            Some(0),
            "legacy single-plan state receives deterministic compatibility index zero",
        );

        let cleared = [None];
        let cleared_frame = RetainedFamilyFrame {
            retained: &retained_frame,
            family_animations: &cleared,
        };
        let incremental = RetainedFamilyExecutionDeltaEnvelope::incremental(
            retained(false, 1),
            &cleared_frame,
            &FrameChanges::objects(vec![0]),
        )
        .unwrap();
        installed
            .apply(&incremental, &retained_frame, &TextResourceArena::new())
            .unwrap();
        assert_eq!(
            installed
                .frame(&retained_frame)
                .unwrap()
                .family_animation(0),
            None
        );
        assert_eq!(
            installed
                .planned_frame(&retained_frame)
                .unwrap()
                .family_plan_index(0),
            None
        );
    }

    #[test]
    fn plural_state_requires_and_validates_exact_plan_identity() {
        let retained_frame = frame();
        let plan = geometry_plan();
        let snapshot = RetainedFamilyExecutionDeltaEnvelope {
            retained: retained(true, 0),
            family_states: vec![RetainedFamilyExecutionObjectState::planned(
                ObjectId::new(7),
                Some(family_state(0.5)),
                Some(1),
            )
            .unwrap()],
            family_plans: vec![
                RetainedFamilyPlanTransport::from_plan(&plan),
                RetainedFamilyPlanTransport::from_plan(&plan),
            ],
            resource_additions: None,
        };
        let mut installed = InstalledRetainedFamilyExecutionState::default();
        installed
            .apply(&snapshot, &retained_frame, &TextResourceArena::new())
            .unwrap();
        assert_eq!(
            installed
                .planned_frame(&retained_frame)
                .unwrap()
                .family_plan_index(0),
            Some(1)
        );

        let missing = RetainedFamilyExecutionDeltaEnvelope {
            retained: retained(false, 1),
            family_states: vec![RetainedFamilyExecutionObjectState::new(
                ObjectId::new(7),
                Some(family_state(0.25)),
            )
            .unwrap()],
            family_plans: Vec::new(),
            resource_additions: None,
        };
        assert_eq!(
            installed
                .apply(&missing, &retained_frame, &TextResourceArena::new())
                .unwrap_err(),
            RetainedFamilyExecutionTransportError::MissingPlanIndex(ObjectId::new(7))
        );
    }

    #[test]
    fn incremental_plan_append_installs_before_resolving_sparse_state() {
        let retained_frame = frame();
        let mut installed = InstalledRetainedFamilyExecutionState::default();
        installed
            .apply(
                &family_snapshot(0.5),
                &retained_frame,
                &TextResourceArena::new(),
            )
            .unwrap();

        let appended = RetainedFamilyExecutionDeltaEnvelope {
            retained: retained(false, 1),
            family_states: vec![RetainedFamilyExecutionObjectState::planned(
                ObjectId::new(7),
                Some(family_state(0.25)),
                Some(1),
            )
            .unwrap()],
            family_plans: vec![RetainedFamilyPlanTransport::from_plan(&geometry_plan())],
            resource_additions: None,
        };
        installed
            .apply(&appended, &retained_frame, &TextResourceArena::new())
            .unwrap();
        assert_eq!(installed.plans().len(), 2);
        assert_eq!(
            installed
                .planned_frame(&retained_frame)
                .unwrap()
                .family_plan_index(0),
            Some(1)
        );
    }

    #[test]
    fn sparse_prepare_visits_only_changed_state_and_added_plan_leaf() {
        let retained_frame = frame();
        let mut installed = InstalledRetainedFamilyExecutionState::default();
        installed
            .apply(
                &family_snapshot(0.5),
                &retained_frame,
                &TextResourceArena::new(),
            )
            .unwrap();
        let appended = RetainedFamilyExecutionDeltaEnvelope {
            retained: retained(false, 1),
            family_states: vec![RetainedFamilyExecutionObjectState::planned(
                ObjectId::new(7),
                Some(family_state(0.25)),
                Some(1),
            )
            .unwrap()],
            family_plans: vec![RetainedFamilyPlanTransport::from_plan(&geometry_plan())],
            resource_additions: None,
        };
        let index_lookups = Cell::new(0);
        let object_lookups = Cell::new(0);
        let prepared = installed
            .prepare_with_lookup(
                &appended,
                retained_frame.objects.len(),
                &TextResourceArena::new(),
                |object| {
                    index_lookups.set(index_lookups.get() + 1);
                    (object == ObjectId::new(7)).then_some(0)
                },
                |object| {
                    object_lookups.set(object_lookups.get() + 1);
                    (object == ObjectId::new(7)).then_some(&retained_frame.objects[0])
                },
            )
            .unwrap();

        assert_eq!(index_lookups.get(), 1);
        assert_eq!(object_lookups.get(), 1);
        assert_eq!(
            installed.plans().len(),
            1,
            "prepare does not mutate live plans"
        );
        installed.commit_prepared(prepared);
        assert_eq!(installed.plans().len(), 2);
        assert_eq!(
            installed
                .planned_frame(&retained_frame)
                .unwrap()
                .family_plan_index(0),
            Some(1)
        );
    }

    #[test]
    fn failed_incremental_plan_append_preserves_installed_plan_and_state() {
        let retained_frame = frame();
        let mut installed = InstalledRetainedFamilyExecutionState::default();
        installed
            .apply(
                &family_snapshot(0.5),
                &retained_frame,
                &TextResourceArena::new(),
            )
            .unwrap();
        let invalid = RetainedFamilyExecutionDeltaEnvelope {
            retained: retained(false, 1),
            family_states: Vec::new(),
            family_plans: vec![RetainedFamilyPlanTransport {
                target: noon_core::SemanticNodeId::new(1, 0),
                bindings: Vec::new(),
                global_span: None,
            }],
            resource_additions: None,
        };

        assert!(installed
            .apply(&invalid, &retained_frame, &TextResourceArena::new())
            .is_err());
        assert_eq!(installed.plans().len(), 1);
        assert_eq!(
            installed
                .frame(&retained_frame)
                .unwrap()
                .family_animation(0),
            Some(family_state(0.5))
        );
    }

    #[test]
    fn unplanned_active_state_fails_closed() {
        let bad = RetainedFamilyExecutionDeltaEnvelope {
            retained: retained(false, 1),
            family_states: vec![RetainedFamilyExecutionObjectState::new(
                ObjectId::new(7),
                Some(family_state(0.5)),
            )
            .unwrap()],
            family_plans: Vec::new(),
            resource_additions: None,
        };
        let mut installed = InstalledRetainedFamilyExecutionState::default();
        installed
            .apply(
                &RetainedFamilyExecutionDeltaEnvelope {
                    retained: retained(true, 0),
                    family_states: Vec::new(),
                    family_plans: Vec::new(),
                    resource_additions: None,
                },
                &frame(),
                &TextResourceArena::new(),
            )
            .unwrap();
        assert_eq!(
            installed
                .apply(&bad, &frame(), &TextResourceArena::new())
                .unwrap_err(),
            RetainedFamilyExecutionTransportError::MissingPlanIndex(ObjectId::new(7))
        );
    }

    #[test]
    fn failed_snapshot_does_not_replace_installed_family_state() {
        let retained_frame = frame();
        let mut installed = InstalledRetainedFamilyExecutionState::default();
        installed
            .apply(
                &family_snapshot(0.5),
                &retained_frame,
                &TextResourceArena::new(),
            )
            .unwrap();

        let invalid = RetainedFamilyExecutionDeltaEnvelope {
            retained: retained(true, 0),
            family_states: vec![RetainedFamilyExecutionObjectState::new(
                ObjectId::new(7),
                Some(family_state(0.25)),
            )
            .unwrap()],
            family_plans: Vec::new(),
            resource_additions: None,
        };
        assert_eq!(
            installed
                .apply(&invalid, &retained_frame, &TextResourceArena::new())
                .unwrap_err(),
            RetainedFamilyExecutionTransportError::MissingPlanIndex(ObjectId::new(7))
        );
        assert!(installed.single_plan().unwrap().is_some());
        assert_eq!(
            installed
                .frame(&retained_frame)
                .unwrap()
                .family_animation(0),
            Some(family_state(0.5))
        );
    }
}
