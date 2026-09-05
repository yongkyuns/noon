use noon_compile::CompiledScene;
use noon_core::{
    FamilyAnimationError, FamilyAnimationSpec, FamilyAnimationState, ObjectId,
    RetainedFamilyAnimationPlan,
};

use crate::{EvaluationError, FrameChanges, RetainedPlannedFamilyFrame};

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlannedFamilyInterval {
    plan_index: u32,
    start_time: f64,
    end_time: f64,
}

/// Failure while binding multiple immutable family plans to one retained scene.
#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyPlanSetRuntimeError {
    Animation(FamilyAnimationError),
    Evaluation(EvaluationError),
    UnknownObject(ObjectId),
    TooManyPlans(usize),
    OverlappingAnimations {
        object: ObjectId,
        previous_plan: u32,
        previous_start: f64,
        previous_end: f64,
        next_plan: u32,
        next_start: f64,
    },
}

impl std::fmt::Display for RetainedFamilyPlanSetRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Animation(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::UnknownObject(object) => write!(
                formatter,
                "retained family plan set references unknown compiled object {}",
                object.get()
            ),
            Self::TooManyPlans(count) => write!(
                formatter,
                "retained family plan set has {count} plans, exceeding the u32 plan-index domain"
            ),
            Self::OverlappingAnimations {
                object,
                previous_plan,
                previous_start,
                previous_end,
                next_plan,
                next_start,
            } => write!(
                formatter,
                "retained family plans overlap on object {}: plan {previous_plan} [{previous_start}, {previous_end}] overlaps plan {next_plan} starting at {next_start}",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyPlanSetRuntimeError {}

impl From<FamilyAnimationError> for RetainedFamilyPlanSetRuntimeError {
    fn from(value: FamilyAnimationError) -> Self {
        Self::Animation(value)
    }
}

impl From<EvaluationError> for RetainedFamilyPlanSetRuntimeError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

/// Deterministic retained runtime for any number of immutable semantic-family plans.
///
/// Plans are installed in canonical request order and retain that index for transport.
/// Multiple plans may run concurrently when their object sets are disjoint. Reusing an
/// object is allowed when intervals are sequential; overlapping ownership of the same
/// object is rejected before playback because one retained object can realize only one
/// family operation at a time.
#[derive(Clone, Debug)]
pub struct RetainedFamilyPlanSetSceneInstance {
    inner: crate::SceneInstance,
    plans: Vec<RetainedFamilyAnimationPlan>,
    specs: Vec<FamilyAnimationSpec>,
    object_schedules: Vec<Vec<PlannedFamilyInterval>>,
    states: Vec<Option<FamilyAnimationState>>,
    plan_indices: Vec<Option<u32>>,
    family_changed_indices: Vec<usize>,
}

impl RetainedFamilyPlanSetSceneInstance {
    pub fn new(
        compiled: CompiledScene,
        animations: Vec<(RetainedFamilyAnimationPlan, FamilyAnimationSpec)>,
    ) -> Result<Self, RetainedFamilyPlanSetRuntimeError> {
        if animations.len() > u32::MAX as usize {
            return Err(RetainedFamilyPlanSetRuntimeError::TooManyPlans(
                animations.len(),
            ));
        }

        let object_count = compiled.objects().len();
        let mut object_schedules = vec![Vec::new(); object_count];
        let mut plans = Vec::with_capacity(animations.len());
        let mut specs = Vec::with_capacity(animations.len());

        for (animation_index, (plan, spec)) in animations.into_iter().enumerate() {
            spec.validate()?;
            let plan_index = animation_index as u32;
            for leaf in plan.leaves() {
                let object = leaf.span().object;
                let object_index = compiled
                    .object_index(object)
                    .map(|index| index as usize)
                    .ok_or(RetainedFamilyPlanSetRuntimeError::UnknownObject(object))?;
                object_schedules[object_index].push(PlannedFamilyInterval {
                    plan_index,
                    start_time: spec.start_time,
                    end_time: spec.end_time(),
                });
            }
            plans.push(plan);
            specs.push(spec);
        }

        for (object_index, schedule) in object_schedules.iter_mut().enumerate() {
            schedule.sort_by(|left, right| {
                left.start_time
                    .total_cmp(&right.start_time)
                    .then_with(|| left.plan_index.cmp(&right.plan_index))
            });
            for pair in schedule.windows(2) {
                let previous = pair[0];
                let next = pair[1];
                if next.start_time < previous.end_time {
                    return Err(RetainedFamilyPlanSetRuntimeError::OverlappingAnimations {
                        object: compiled.objects()[object_index].id,
                        previous_plan: previous.plan_index,
                        previous_start: previous.start_time,
                        previous_end: previous.end_time,
                        next_plan: next.plan_index,
                        next_start: next.start_time,
                    });
                }
            }
        }

        Ok(Self {
            inner: crate::SceneInstance::new(compiled),
            plans,
            specs,
            object_schedules,
            states: vec![None; object_count],
            plan_indices: vec![None; object_count],
            family_changed_indices: Vec::new(),
        })
    }

    pub fn frame(&self) -> RetainedPlannedFamilyFrame<'_> {
        RetainedPlannedFamilyFrame {
            retained: self.inner.frame(),
            family_animations: &self.states,
            family_plan_indices: &self.plan_indices,
        }
    }

    pub fn plans(&self) -> &[RetainedFamilyAnimationPlan] {
        &self.plans
    }

    pub fn specs(&self) -> &[FamilyAnimationSpec] {
        &self.specs
    }

    pub fn inner(&self) -> &crate::SceneInstance {
        &self.inner
    }

    pub fn evaluate(
        &mut self,
        time: f64,
    ) -> Result<RetainedPlannedFamilyFrame<'_>, RetainedFamilyPlanSetRuntimeError> {
        self.inner.evaluate(time)?;
        self.update_family_state(time)?;
        Ok(self.frame())
    }

    pub fn seek(
        &mut self,
        time: f64,
    ) -> Result<RetainedPlannedFamilyFrame<'_>, RetainedFamilyPlanSetRuntimeError> {
        self.inner.seek(time)?;
        self.update_family_state(time)?;
        Ok(self.frame())
    }

    pub fn advance_to(
        &mut self,
        time: f64,
    ) -> Result<RetainedPlannedFamilyFrame<'_>, RetainedFamilyPlanSetRuntimeError> {
        self.inner.advance_to(time)?;
        self.update_family_state(time)?;
        Ok(self.frame())
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        let base = self.inner.take_frame_changes();
        if base.is_all() {
            self.family_changed_indices.clear();
            return FrameChanges::all();
        }

        let mut object_indices = base.object_indices().to_vec();
        object_indices.append(&mut self.family_changed_indices);
        FrameChanges::with_structure(
            object_indices,
            base.added_indices().to_vec(),
            base.removed_indices().to_vec(),
        )
    }

    fn update_family_state(&mut self, time: f64) -> Result<(), RetainedFamilyPlanSetRuntimeError> {
        let animation_states = self
            .specs
            .iter()
            .copied()
            .map(|spec| active_state_at(spec, time))
            .collect::<Result<Vec<_>, _>>()?;

        for (object_index, schedule) in self.object_schedules.iter().enumerate() {
            let selected = schedule
                .partition_point(|interval| interval.start_time <= time)
                .checked_sub(1)
                .map(|index| schedule[index])
                .filter(|interval| time <= interval.end_time);
            let next_plan_index = selected.map(|interval| interval.plan_index);
            let next_state = next_plan_index.and_then(|index| animation_states[index as usize]);

            if self.states[object_index] != next_state
                || self.plan_indices[object_index] != next_plan_index
            {
                self.states[object_index] = next_state;
                self.plan_indices[object_index] = next_plan_index;
                self.family_changed_indices.push(object_index);
            }
        }
        Ok(())
    }
}

fn active_state_at(
    spec: FamilyAnimationSpec,
    time: f64,
) -> Result<Option<FamilyAnimationState>, FamilyAnimationError> {
    if time < spec.start_time || time > spec.end_time() {
        if !time.is_finite() {
            spec.state_at(time)?;
        }
        return Ok(None);
    }
    Ok(Some(spec.state_at(time)?))
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledObject;
    use noon_core::{
        FamilyAnimationMode, GeometryRef, RateFunction, RetainedFamilyAnimationPlanBuilder,
        RetainedObjectDefinition, SemanticStore, TextResourceArena,
    };

    use super::*;

    fn plan_for(object: RetainedObjectDefinition) -> RetainedFamilyAnimationPlan {
        let mut semantics = SemanticStore::new();
        let leaf = semantics.insert_authoring_object();
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&semantics, leaf).unwrap();
        builder
            .accept_leaf(leaf, &object, &TextResourceArena::new())
            .unwrap();
        builder.finish().unwrap()
    }

    fn spec(start: f64, duration: f64, mode: FamilyAnimationMode) -> FamilyAnimationSpec {
        FamilyAnimationSpec::new(
            mode,
            start,
            duration,
            0.0,
            RateFunction::Linear,
            false,
            false,
        )
        .unwrap()
    }

    #[test]
    fn sequential_same_object_selects_exact_plan_and_next_wins_touching_boundary() {
        let object =
            RetainedObjectDefinition::geometry(ObjectId::new(10), GeometryRef::circle(1.0));
        let compiled = CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object.id,
                object.content.clone(),
                object.transform,
                object.style,
            )],
            &[],
        )
        .unwrap();
        let animations = vec![
            (
                plan_for(object.clone()),
                spec(0.0, 1.0, FamilyAnimationMode::Reveal),
            ),
            (
                plan_for(object),
                spec(1.0, 1.0, FamilyAnimationMode::DrawBorderThenFill),
            ),
        ];
        let mut runtime = RetainedFamilyPlanSetSceneInstance::new(compiled, animations).unwrap();

        let first = runtime.seek(0.5).unwrap();
        assert_eq!(first.family_plan_index(0), Some(0));
        assert_eq!(
            first.family_animation(0).unwrap().mode,
            FamilyAnimationMode::Reveal
        );

        let boundary = runtime.seek(1.0).unwrap();
        assert_eq!(boundary.family_plan_index(0), Some(1));
        assert_eq!(
            boundary.family_animation(0).unwrap().mode,
            FamilyAnimationMode::DrawBorderThenFill
        );

        let second = runtime.seek(1.5).unwrap();
        assert_eq!(second.family_plan_index(0), Some(1));
        assert_eq!(runtime.seek(2.1).unwrap().family_plan_index(0), None);
    }

    #[test]
    fn overlapping_same_object_fails_before_playback() {
        let object =
            RetainedObjectDefinition::geometry(ObjectId::new(10), GeometryRef::circle(1.0));
        let compiled = CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object.id,
                object.content.clone(),
                object.transform,
                object.style,
            )],
            &[],
        )
        .unwrap();
        let error = RetainedFamilyPlanSetSceneInstance::new(
            compiled,
            vec![
                (
                    plan_for(object.clone()),
                    spec(0.0, 1.0, FamilyAnimationMode::Reveal),
                ),
                (
                    plan_for(object),
                    spec(0.5, 1.0, FamilyAnimationMode::Reveal),
                ),
            ],
        )
        .unwrap_err();
        assert!(matches!(
            error,
            RetainedFamilyPlanSetRuntimeError::OverlappingAnimations { object, .. }
                if object == ObjectId::new(10)
        ));
    }

    #[test]
    fn concurrent_disjoint_plans_can_use_different_modes() {
        let first = RetainedObjectDefinition::geometry(ObjectId::new(10), GeometryRef::circle(1.0));
        let second =
            RetainedObjectDefinition::geometry(ObjectId::new(11), GeometryRef::circle(2.0));
        let compiled = CompiledScene::compile_objects(
            vec![
                CompiledObject::new(
                    first.id,
                    first.content.clone(),
                    first.transform,
                    first.style,
                ),
                CompiledObject::new(
                    second.id,
                    second.content.clone(),
                    second.transform,
                    second.style,
                ),
            ],
            &[],
        )
        .unwrap();
        let mut runtime = RetainedFamilyPlanSetSceneInstance::new(
            compiled,
            vec![
                (plan_for(first), spec(0.0, 2.0, FamilyAnimationMode::Reveal)),
                (
                    plan_for(second),
                    spec(0.0, 2.0, FamilyAnimationMode::DrawBorderThenFill),
                ),
            ],
        )
        .unwrap();
        let frame = runtime.seek(1.0).unwrap();
        assert_eq!(frame.family_plan_index(0), Some(0));
        assert_eq!(frame.family_plan_index(1), Some(1));
        assert_eq!(
            frame.family_animation(0).unwrap().mode,
            FamilyAnimationMode::Reveal
        );
        assert_eq!(
            frame.family_animation(1).unwrap().mode,
            FamilyAnimationMode::DrawBorderThenFill
        );
    }

    #[test]
    fn direct_seek_matches_forward_playback_for_state_and_plan_identity() {
        let object =
            RetainedObjectDefinition::geometry(ObjectId::new(10), GeometryRef::circle(1.0));
        let compiled = CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object.id,
                object.content.clone(),
                object.transform,
                object.style,
            )],
            &[],
        )
        .unwrap();
        let animations = vec![
            (
                plan_for(object.clone()),
                spec(0.0, 1.0, FamilyAnimationMode::Reveal),
            ),
            (
                plan_for(object),
                spec(1.0, 1.0, FamilyAnimationMode::Reveal),
            ),
        ];
        let mut forward =
            RetainedFamilyPlanSetSceneInstance::new(compiled.clone(), animations.clone()).unwrap();
        forward.advance_to(0.5).unwrap();
        let forward_frame = forward.advance_to(1.5).unwrap();
        let forward_state = forward_frame.family_animation(0);
        let forward_plan = forward_frame.family_plan_index(0);

        let mut direct = RetainedFamilyPlanSetSceneInstance::new(compiled, animations).unwrap();
        let direct_frame = direct.seek(1.5).unwrap();
        assert_eq!(direct_frame.family_animation(0), forward_state);
        assert_eq!(direct_frame.family_plan_index(0), forward_plan);
        assert!(direct.take_frame_changes().is_all());
    }
}
