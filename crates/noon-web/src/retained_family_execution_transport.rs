use std::collections::HashSet;

use noon_core::{FamilyAnimationState, ObjectId, RetainedFamilyAnimationPlan, TextResourceArena};
use noon_runtime::{FrameChanges, RetainedFamilyFrame, RetainedFrameState};
use serde::{Deserialize, Serialize};

use crate::{
    RetainedExecutionDeltaEnvelope, RetainedFamilyPlanTransport, RetainedFamilyTransportError,
    RetainedFamilyTransportState,
};

/// Sparse per-object family scheduler state carried alongside an ordinary retained delta.
///
/// An entry with `family_animation: None` is meaningful on incrementals: it clears a
/// previously active family animation after its interval ends.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedFamilyExecutionObjectState {
    pub object: ObjectId,
    #[serde(flatten)]
    pub state: RetainedFamilyTransportState,
}

impl RetainedFamilyExecutionObjectState {
    pub fn new(
        object: ObjectId,
        family_animation: Option<FamilyAnimationState>,
    ) -> Result<Self, RetainedFamilyExecutionTransportError> {
        Ok(Self {
            object,
            state: RetainedFamilyTransportState::new(family_animation)?,
        })
    }
}

/// Additive family-animation envelope over the stable retained execution transport.
///
/// Serde flattening preserves the existing retained v1 JSON shape. Family-aware
/// producers add only sparse evaluated scheduler state and snapshot-only immutable
/// plan descriptors; glyph IDs and renderer payloads remain renderer-local.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedFamilyExecutionDeltaEnvelope {
    #[serde(flatten)]
    pub retained: RetainedExecutionDeltaEnvelope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub family_states: Vec<RetainedFamilyExecutionObjectState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub family_plans: Vec<RetainedFamilyPlanTransport>,
}

impl RetainedFamilyExecutionDeltaEnvelope {
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
            retained,
        };
        envelope.validate()?;
        Ok(envelope)
    }

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
            retained,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn validate(&self) -> Result<(), RetainedFamilyExecutionTransportError> {
        if !self.retained.snapshot && !self.family_plans.is_empty() {
            return Err(RetainedFamilyExecutionTransportError::IncrementalPlanInstall);
        }

        let mut seen = HashSet::with_capacity(self.family_states.len());
        for entry in &self.family_states {
            if !seen.insert(entry.object) {
                return Err(RetainedFamilyExecutionTransportError::DuplicateStateObject(
                    entry.object,
                ));
            }
            entry.state.validate()?;
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

/// Renderer-side family state paired with a resolved retained execution frame.
///
/// Snapshots replace plans/state only after the complete replacement validates.
/// Incrementals likewise validate all sparse updates before mutating live state.
#[derive(Clone, Debug, Default)]
pub struct InstalledRetainedFamilyExecutionState {
    states: Vec<Option<FamilyAnimationState>>,
    plans: Vec<RetainedFamilyAnimationPlan>,
    initialized: bool,
}

impl InstalledRetainedFamilyExecutionState {
    pub fn apply(
        &mut self,
        delta: &RetainedFamilyExecutionDeltaEnvelope,
        frame: &RetainedFrameState,
        texts: &TextResourceArena,
    ) -> Result<(), RetainedFamilyExecutionTransportError> {
        delta.validate()?;

        if delta.retained.snapshot {
            let plans = delta
                .family_plans
                .iter()
                .map(|plan| plan.install(frame, texts))
                .collect::<Result<Vec<_>, _>>()?;
            let mut states = vec![None; frame.objects.len()];
            for (index, state) in validated_state_updates(frame, &plans, &delta.family_states)? {
                states[index] = state;
            }
            self.states = states;
            self.plans = plans;
            self.initialized = true;
            return Ok(());
        }

        if !self.initialized {
            return Err(RetainedFamilyExecutionTransportError::IncrementalBeforeSnapshot);
        }
        if self.states.len() != frame.objects.len() {
            return Err(RetainedFamilyExecutionTransportError::FrameShapeMismatch);
        }

        let updates = validated_state_updates(frame, &self.plans, &delta.family_states)?;
        for (index, state) in updates {
            self.states[index] = state;
        }
        Ok(())
    }

    pub fn frame<'a>(
        &'a self,
        retained: &'a RetainedFrameState,
    ) -> Result<RetainedFamilyFrame<'a>, RetainedFamilyExecutionTransportError> {
        if !self.initialized {
            return Err(RetainedFamilyExecutionTransportError::MissingSnapshot);
        }
        if self.states.len() != retained.objects.len() {
            return Err(RetainedFamilyExecutionTransportError::FrameShapeMismatch);
        }
        Ok(RetainedFamilyFrame {
            retained,
            family_animations: &self.states,
        })
    }

    pub fn plans(&self) -> &[RetainedFamilyAnimationPlan] {
        &self.plans
    }

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
}

fn validated_state_updates(
    frame: &RetainedFrameState,
    plans: &[RetainedFamilyAnimationPlan],
    entries: &[RetainedFamilyExecutionObjectState],
) -> Result<Vec<(usize, Option<FamilyAnimationState>)>, RetainedFamilyExecutionTransportError> {
    entries
        .iter()
        .map(|entry| {
            let index = frame
                .objects
                .iter()
                .position(|object| object.id == entry.object)
                .ok_or(RetainedFamilyExecutionTransportError::UnknownObject(
                    entry.object,
                ))?;
            if entry.state.family_animation.is_some()
                && !plans
                    .iter()
                    .any(|plan| plan.leaf_for_object(entry.object).is_some())
            {
                return Err(RetainedFamilyExecutionTransportError::StateWithoutPlan(
                    entry.object,
                ));
            }
            Ok((index, entry.state.family_animation))
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyExecutionTransportError {
    Family(RetainedFamilyTransportError),
    ExpectedSnapshot,
    ExpectedIncremental,
    IncrementalPlanInstall,
    IncrementalBeforeSnapshot,
    MissingSnapshot,
    FrameShapeMismatch,
    InvalidObjectIndex(usize),
    DuplicateStateObject(ObjectId),
    UnknownObject(ObjectId),
    StateWithoutPlan(ObjectId),
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
            Self::IncrementalPlanInstall => {
                formatter.write_str("retained family plans may only be installed by a snapshot")
            }
            Self::IncrementalBeforeSnapshot => formatter
                .write_str("retained family execution requires a snapshot before incrementals"),
            Self::MissingSnapshot => {
                formatter.write_str("retained family execution has no installed snapshot")
            }
            Self::FrameShapeMismatch => formatter
                .write_str("retained family state shape does not match retained frame objects"),
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
            Self::MultiplePlansUnsupported(count) => write!(
                formatter,
                "retained family renderer currently requires one active plan, got {count}"
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
    use noon_core::{
        Camera2DState, FamilyAnimationMode, GeometryRef, ObjectContentRef, RateFunction, Style,
        Transform2D,
    };
    use noon_runtime::RetainedFrameObjectState;

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

    fn frame() -> RetainedFrameState {
        RetainedFrameState {
            time: 0.0,
            objects: vec![RetainedFrameObjectState {
                id: ObjectId::new(7),
                content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
            }],
            presences: vec![true],
            reveals: vec![1.0],
            morphs: vec![0.0],
            render_geometries: vec![None],
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
                presence: true,
                reveal: 1.0,
                morph: 0.0,
                render_geometry: None,
            }],
        }
    }

    fn geometry_plan() -> RetainedFamilyAnimationPlan {
        let object = noon_core::RetainedObjectDefinition::geometry(
            ObjectId::new(7),
            GeometryRef::circle(1.0),
        );
        let mut semantics = noon_core::SemanticStore::new();
        let leaf = semantics.insert_authoring_object();
        let mut builder =
            noon_core::RetainedFamilyAnimationPlanBuilder::begin(&semantics, leaf).unwrap();
        builder
            .accept_leaf(leaf, &object, &TextResourceArena::new())
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
    }

    #[test]
    fn incremental_plan_install_and_unplanned_active_state_fail_closed() {
        let mut bad = RetainedFamilyExecutionDeltaEnvelope {
            retained: retained(false, 1),
            family_states: Vec::new(),
            family_plans: vec![RetainedFamilyPlanTransport::from_plan(&geometry_plan())],
        };
        assert_eq!(
            bad.validate().unwrap_err(),
            RetainedFamilyExecutionTransportError::IncrementalPlanInstall
        );

        bad.family_plans.clear();
        bad.family_states.push(
            RetainedFamilyExecutionObjectState::new(ObjectId::new(7), Some(family_state(0.5)))
                .unwrap(),
        );
        let mut installed = InstalledRetainedFamilyExecutionState::default();
        installed
            .apply(
                &RetainedFamilyExecutionDeltaEnvelope {
                    retained: retained(true, 0),
                    family_states: Vec::new(),
                    family_plans: Vec::new(),
                },
                &frame(),
                &TextResourceArena::new(),
            )
            .unwrap();
        assert_eq!(
            installed
                .apply(&bad, &frame(), &TextResourceArena::new())
                .unwrap_err(),
            RetainedFamilyExecutionTransportError::StateWithoutPlan(ObjectId::new(7))
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
        };
        assert_eq!(
            installed
                .apply(&invalid, &retained_frame, &TextResourceArena::new())
                .unwrap_err(),
            RetainedFamilyExecutionTransportError::StateWithoutPlan(ObjectId::new(7))
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
