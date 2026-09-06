use std::collections::BTreeMap;

use noon_compile::{CompileError, CompiledScene, SemanticExecutionLoweringOutput};
use noon_core::{
    ComputeProgram, ComputeState, ObjectId, PreparedComputeInputBatch,
    PreparedComputeInputEnrollment, PreparedComputeInputEnrollmentBatch, Property,
    PublicationContext, ReactiveBinding, ReactiveError, ReactiveEvaluationStats, ReactiveProgram,
    ReactiveValue, SemanticScene, SignalId,
};

use crate::{frame_row_mut, FrameRowMut, FrameState, SceneInstance};

#[derive(Clone, Debug, PartialEq)]
pub enum SceneBuildError {
    Compile(CompileError),
    Reactive(ReactiveError),
}

impl std::fmt::Display for SceneBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compile(error) => write!(formatter, "scene compilation failed: {error}"),
            Self::Reactive(error) => write!(formatter, "reactive compilation failed: {error}"),
        }
    }
}

impl std::error::Error for SceneBuildError {}

impl From<CompileError> for SceneBuildError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

impl From<ReactiveError> for SceneBuildError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

/// Work performed by the most recent native reactive input update.
///
/// These counters deliberately exclude total scene/object counts. Once a semantic
/// scene is lowered, an input update performs dependency evaluation plus direct
/// writes to precomputed dense frame targets only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReactiveRuntimeStats {
    pub derived_signals_evaluated: usize,
    pub bindings_invalidated: usize,
    pub dense_targets_applied: usize,
    pub dense_targets_changed: usize,
}

#[derive(Clone, Copy, Debug)]
struct ReactiveTarget {
    signal: SignalId,
    object_index: usize,
    property: Property,
}

#[derive(Clone, Debug)]
pub(crate) struct ReactiveRuntime {
    state: ComputeState,
    targets: Vec<ReactiveTarget>,
    target_lookup: BTreeMap<(u64, u8), usize>,
    targets_by_object: Vec<Vec<usize>>,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedReactiveRuntimeUpdate {
    compute: PreparedComputeInputBatch,
    property_changes: Vec<(usize, Property, ReactiveValue)>,
    stats: ReactiveRuntimeStats,
}

#[derive(Clone, Debug)]
pub struct PreparedReactiveSignalEnrollment {
    compute: PreparedComputeInputEnrollment,
}

#[derive(Clone, Debug)]
pub struct PreparedReactiveSignalEnrollmentBatch {
    compute: PreparedComputeInputEnrollmentBatch,
}

impl PreparedReactiveRuntimeUpdate {
    pub(crate) fn is_empty(&self) -> bool {
        self.compute.is_empty()
    }
    pub(crate) fn property_changes(&self) -> &[(usize, Property, ReactiveValue)] {
        &self.property_changes
    }
}

impl ReactiveRuntime {
    pub(crate) fn state_value(&self, signal: noon_core::SignalId) -> Option<&ReactiveValue> {
        self.state.value(signal)
    }

    fn new(
        compiled: &CompiledScene,
        bindings: &[ReactiveBinding],
        program: ComputeProgram,
    ) -> Self {
        let mut targets = Vec::with_capacity(bindings.len());
        let mut target_lookup = BTreeMap::new();
        let mut targets_by_object = vec![Vec::new(); compiled.objects().len()];

        for binding in bindings {
            let object_index = compiled
                .object_index(binding.object)
                .expect("reactive graph was validated against the compiled execution domain")
                as usize;
            let target_index = targets.len();
            targets.push(ReactiveTarget {
                signal: binding.signal,
                object_index,
                property: binding.property,
            });
            target_lookup.insert(binding_key(binding.object, binding.property), target_index);
            targets_by_object[object_index].push(target_index);
        }

        Self {
            state: program.instantiate(),
            targets,
            target_lookup,
            targets_by_object,
        }
    }

    fn target(&self, object: ObjectId, property: Property) -> ReactiveTarget {
        let index = *self
            .target_lookup
            .get(&binding_key(object, property))
            .expect("reactive update must reference a lowered binding");
        self.targets[index]
    }

    pub(crate) fn prepare_input_batch(
        &mut self,
        inputs: &[(SignalId, ReactiveValue)],
    ) -> Result<PreparedReactiveRuntimeUpdate, ReactiveError> {
        let compute = self.state.prepare_input_batch(inputs)?;
        let mut property_changes = Vec::new();
        let mut stats = ReactiveRuntimeStats::default();
        let update = compute.update();
        let evaluation = update.stats();
        stats.derived_signals_evaluated = evaluation.derived_signals_evaluated;
        stats.bindings_invalidated = evaluation.bindings_invalidated;
        for change in update.property_changes() {
            let target = self.target(change.object, change.property);
            property_changes.push((target.object_index, target.property, change.value.clone()));
        }
        stats.dense_targets_applied = property_changes.len();
        Ok(PreparedReactiveRuntimeUpdate {
            compute,
            property_changes,
            stats,
        })
    }

    pub(crate) fn prepared_value(
        &self,
        prepared: &PreparedReactiveRuntimeUpdate,
        signal: noon_core::SignalId,
    ) -> Option<ReactiveValue> {
        self.state.prepared_value(&prepared.compute, signal)
    }

    pub(crate) fn prepare_signal_enrollment(
        &self,
        signal: Option<noon_core::SignalId>,
        value: ReactiveValue,
    ) -> Result<PreparedReactiveSignalEnrollment, ReactiveError> {
        Ok(PreparedReactiveSignalEnrollment {
            compute: self.state.prepare_input_enrollment(signal, value)?,
        })
    }

    pub(crate) fn commit_signal_enrollment(
        &mut self,
        prepared: PreparedReactiveSignalEnrollment,
        signal: noon_core::SignalId,
    ) {
        self.state
            .commit_input_enrollment(prepared.compute, signal)
            .expect("reactive enrollment commits immediately under exclusive runtime ownership");
    }

    pub(crate) fn prepare_signal_enrollment_batch(
        &self,
        inputs: &[(SignalId, ReactiveValue)],
    ) -> Result<PreparedReactiveSignalEnrollmentBatch, ReactiveError> {
        Ok(PreparedReactiveSignalEnrollmentBatch {
            compute: self.state.prepare_input_enrollment_batch(inputs)?,
        })
    }

    pub(crate) fn commit_signal_enrollment_batch(
        &mut self,
        prepared: PreparedReactiveSignalEnrollmentBatch,
    ) {
        self.state
            .commit_input_enrollment_batch(prepared.compute)
            .expect(
                "reactive enrollment batch commits immediately under exclusive runtime ownership",
            );
    }

    pub(crate) fn commit_prepared_input_batch(
        &mut self,
        prepared: PreparedReactiveRuntimeUpdate,
    ) -> ReactiveRuntimeStats {
        self.state
            .commit_prepared_input_batch(prepared.compute)
            .expect("runtime commits a prepared batch only after its owning frame preflight");
        prepared.stats
    }

    fn rebind_object(&mut self, object: ObjectId, object_index: usize) {
        const PROPERTIES: [Property; 11] = [
            Property::Presence,
            Property::Transform,
            Property::Position,
            Property::Rotation,
            Property::Scale,
            Property::Fill,
            Property::Stroke,
            Property::Opacity,
            Property::Appearance,
            Property::Reveal,
            Property::Morph,
        ];
        for property in PROPERTIES {
            let Some(&target_index) = self.target_lookup.get(&binding_key(object, property)) else {
                continue;
            };
            let old_index = self.targets[target_index].object_index;
            if old_index == object_index {
                continue;
            }
            if let Some(old_targets) = self.targets_by_object.get_mut(old_index) {
                if let Some(position) = old_targets.iter().position(|&index| index == target_index)
                {
                    old_targets.swap_remove(position);
                }
            }
            if self.targets_by_object.len() <= object_index {
                self.targets_by_object
                    .resize_with(object_index + 1, Vec::new);
            }
            self.targets[target_index].object_index = object_index;
            self.targets_by_object[object_index].push(target_index);
        }
    }
}

impl SceneInstance {
    /// Preflight one input-only reactive slot append. `None` reserves a slot for
    /// a semantic identity that will be allocated by the owning transaction.
    pub fn prepare_reactive_signal_enrollment(
        &self,
        signal: Option<noon_core::SignalId>,
        value: ReactiveValue,
    ) -> Result<PreparedReactiveSignalEnrollment, ReactiveError> {
        self.reactive
            .as_ref()
            .expect("semantic execution sessions always install the reactive runtime")
            .prepare_signal_enrollment(signal, value)
    }

    pub fn commit_reactive_signal_enrollment(
        &mut self,
        prepared: PreparedReactiveSignalEnrollment,
        signal: noon_core::SignalId,
    ) {
        self.reactive
            .as_mut()
            .expect("semantic execution sessions always install the reactive runtime")
            .commit_signal_enrollment(prepared, signal);
    }

    /// Preflight exact-ID sparse reactive input enrollment as one atomic batch.
    pub fn prepare_reactive_signal_enrollment_batch(
        &self,
        inputs: &[(SignalId, ReactiveValue)],
    ) -> Result<PreparedReactiveSignalEnrollmentBatch, ReactiveError> {
        self.reactive
            .as_ref()
            .expect("semantic execution sessions always install the reactive runtime")
            .prepare_signal_enrollment_batch(inputs)
    }

    pub fn commit_reactive_signal_enrollment_batch(
        &mut self,
        prepared: PreparedReactiveSignalEnrollmentBatch,
    ) {
        self.reactive
            .as_mut()
            .expect("semantic execution sessions always install the reactive runtime")
            .commit_signal_enrollment_batch(prepared);
    }

    /// Build runtime state directly from the canonical semantic execution lowering
    /// handoff produced by `noon-compile`.
    ///
    /// The compiled scene and compute program were validated together by lowering,
    /// so runtime only binds the already-lowered reactive targets to dense frame
    /// indices and instantiates the existing compute VM. No authored scene is
    /// reconstructed or recompiled at this boundary.
    pub fn from_semantic_execution(lowered: SemanticExecutionLoweringOutput) -> Self {
        let publication = lowered.publication_context();
        let (compiled, reactive_projection, program) = lowered.into_execution_parts();
        let reactive =
            ReactiveRuntime::new(&compiled, reactive_projection.graph().bindings(), program);
        let mut instance = Self::new(compiled);
        instance.publication = publication;
        instance.reactive = Some(reactive);
        instance.reapply_reactive();
        instance
    }

    /// Compile a high-level semantic scene once and attach its validated native
    /// reactive program to this runtime instance.
    ///
    /// Reactive expressions are flattened into typed compute kernels and bindings
    /// are lowered to dense frame object indices here. Later input updates therefore
    /// do not recurse through authoring ASTs, rebuild `SceneDefinition`, recompile
    /// the timeline, or scan unrelated objects.
    pub fn from_semantic(scene: &SemanticScene) -> Result<Self, SceneBuildError> {
        let compiled = CompiledScene::compile(scene.definition())?;
        let program = ReactiveProgram::compile_for_execution_domain(
            compiled
                .objects()
                .iter()
                .filter(|object| object.live)
                .map(|object| object.id),
            compiled.tracks_iter().map(|track| {
                let object = compiled
                    .object_id_at_slot(track.object_index)
                    .expect("compiled timeline track must reference a live object slot");
                (object, track.property)
            }),
            scene.reactive(),
        )?
        .into_compute()?;
        let reactive = ReactiveRuntime::new(&compiled, scene.reactive().bindings(), program);
        let mut instance = Self::new(compiled);
        instance.reactive = Some(reactive);
        instance.reapply_reactive();
        Ok(instance)
    }

    /// Exact authored/executable/effective publication context of this runtime view.
    pub const fn publication_context(&self) -> PublicationContext {
        self.publication
    }

    /// Publish a coherent effective-only frame change.
    pub(crate) fn publish_effective_change(&mut self) {
        let next = self
            .publication
            .frame_epoch()
            .checked_next()
            .expect("Noon frame epoch space exhausted");
        self.publication = self.publication.with_frame_epoch(next);
    }

    /// Publish one coherent executable-projection change and the corresponding new
    /// effective frame context. Authored scene revision remains pinned to the
    /// semantic snapshot from which this session was built.
    pub(crate) fn publish_execution_change(&mut self) {
        let execution = self
            .publication
            .execution_revision()
            .checked_next()
            .expect("Noon execution revision space exhausted");
        let frame = self
            .publication
            .frame_epoch()
            .checked_next()
            .expect("Noon frame epoch space exhausted");
        self.publication =
            PublicationContext::new(self.publication.scene_revision(), execution, frame);
    }

    pub const fn last_reactive_stats(&self) -> ReactiveRuntimeStats {
        self.last_reactive_stats
    }

    pub fn reactive_value(&self, signal: SignalId) -> Option<&ReactiveValue> {
        self.reactive.as_ref()?.state.value(signal)
    }

    /// Deterministically seek, evaluating a canonical signal-timeline input batch
    /// before reactive bindings are reapplied to the rebuilt frame.
    pub fn seek_with_reactive_inputs(
        &mut self,
        time: f64,
        inputs: &[(SignalId, ReactiveValue)],
    ) -> Result<&FrameState, crate::EvaluationError> {
        if !time.is_finite() {
            return Err(crate::EvaluationError::InvalidTime(time));
        }
        let previous_time = self.frame.time;
        let prepared = self
            .reactive
            .as_mut()
            .ok_or_else(|| {
                crate::EvaluationError::Reactive(ReactiveError::UnknownSignal(inputs[0].0))
            })?
            .prepare_input_batch(inputs)
            .map_err(crate::EvaluationError::Reactive)?;
        let stats = self
            .reactive
            .as_mut()
            .expect("prepared reactive inputs retain their runtime")
            .commit_prepared_input_batch(prepared);
        self.seek_unchecked(time);
        self.last_reactive_stats = stats;
        if self.frame.time != previous_time || stats.bindings_invalidated != 0 {
            self.publish_effective_change();
        }
        Ok(&self.frame)
    }

    /// Change one native input and apply only its invalidated bindings to the
    /// already-compiled dense frame state.
    pub fn set_reactive_input(
        &mut self,
        signal: SignalId,
        value: impl Into<ReactiveValue>,
    ) -> Result<&FrameState, ReactiveError> {
        let update = self
            .reactive
            .as_mut()
            .ok_or(ReactiveError::UnknownSignal(signal))?
            .state
            .set_input(signal, value)?;
        let effective_changed = !update.signal_changes().is_empty();
        let evaluation = update.stats();
        let mut applied_targets = 0;
        let mut changed_targets = 0;

        for change in update.property_changes() {
            let target = self
                .reactive
                .as_ref()
                .expect("reactive state exists while applying its update")
                .target(change.object, change.property);
            if !self
                .compiled
                .object_slot_is_live(target.object_index as u32)
            {
                continue;
            }
            applied_targets += 1;
            if apply_reactive_value(
                &mut self.frame,
                target.object_index,
                target.property,
                &change.value,
            ) {
                self.mark_changed(target.object_index);
                changed_targets += 1;
            }
        }

        self.last_reactive_stats = runtime_stats(evaluation, applied_targets, changed_targets);
        if effective_changed {
            self.publish_effective_change();
        }
        Ok(&self.frame)
    }

    pub(crate) fn reapply_reactive(&mut self) {
        let Self {
            compiled,
            reactive,
            frame,
            ..
        } = self;
        let Some(reactive) = reactive.as_ref() else {
            return;
        };
        for target in &reactive.targets {
            if !compiled.object_slot_is_live(target.object_index as u32) {
                continue;
            }
            let value = reactive
                .state
                .value(target.signal)
                .expect("lowered reactive target references a valid signal");
            apply_reactive_value(frame, target.object_index, target.property, value);
        }
    }

    pub(crate) fn rebind_reactive_object(&mut self, object: ObjectId, object_index: usize) {
        let Some(reactive) = self.reactive.as_mut() else {
            return;
        };
        reactive.rebind_object(object, object_index);
    }

    pub(crate) fn reapply_reactive_for_object(&mut self, object_index: usize) {
        if !self.object_slot_is_live(object_index) {
            return;
        }
        let Self {
            reactive, frame, ..
        } = self;
        let Some(reactive) = reactive.as_ref() else {
            return;
        };
        let Some(target_indices) = reactive.targets_by_object.get(object_index) else {
            return;
        };
        for target_index in target_indices {
            let target = reactive.targets[*target_index];
            let value = reactive
                .state
                .value(target.signal)
                .expect("lowered reactive target references a valid signal");
            apply_reactive_value(frame, target.object_index, target.property, value);
        }
    }

    pub(crate) fn reapply_reactive_to_row(&self, object_index: usize, mut row: FrameRowMut<'_>) {
        if !self.object_slot_is_live(object_index) {
            return;
        }
        let Some(reactive) = self.reactive.as_ref() else {
            return;
        };
        let Some(target_indices) = reactive.targets_by_object.get(object_index) else {
            return;
        };
        for target_index in target_indices {
            let target = reactive.targets[*target_index];
            let value = reactive
                .state
                .value(target.signal)
                .expect("lowered reactive target references a valid signal");
            apply_reactive_value_to_row(&mut row, target.property, value);
        }
    }
}

fn runtime_stats(
    evaluation: ReactiveEvaluationStats,
    dense_targets_applied: usize,
    dense_targets_changed: usize,
) -> ReactiveRuntimeStats {
    ReactiveRuntimeStats {
        derived_signals_evaluated: evaluation.derived_signals_evaluated,
        bindings_invalidated: evaluation.bindings_invalidated,
        dense_targets_applied,
        dense_targets_changed,
    }
}

fn binding_key(object: ObjectId, property: Property) -> (u64, u8) {
    (object.get(), property_slot(property))
}

const fn property_slot(property: Property) -> u8 {
    match property {
        Property::Presence => 0,
        Property::Transform => 1,
        Property::Position => 2,
        Property::Rotation => 3,
        Property::Scale => 4,
        Property::Fill => 5,
        Property::Stroke => 6,
        Property::Opacity => 7,
        Property::Appearance => 8,
        Property::Reveal => 9,
        Property::Morph => 10,
    }
}

fn apply_reactive_value(
    frame: &mut FrameState,
    object_index: usize,
    property: Property,
    value: &ReactiveValue,
) -> bool {
    apply_reactive_value_to_row(&mut frame_row_mut(frame, object_index), property, value)
}

pub(crate) fn apply_reactive_value_to_row(
    row: &mut FrameRowMut<'_>,
    property: Property,
    value: &ReactiveValue,
) -> bool {
    match (property, value) {
        (Property::Presence, ReactiveValue::Bool(value)) => {
            let changed = *row.presence != *value;
            *row.presence = *value;
            changed
        }
        (Property::Position, ReactiveValue::Vec2(value)) => {
            let render_changed = crate::release_render_transform(
                row.render_geometry,
                row.render_transform,
                *row.transform,
            );
            let changed = row.transform.translation != *value;
            row.transform.translation = *value;
            changed || render_changed
        }
        (Property::Rotation, ReactiveValue::Scalar(value)) => {
            let render_changed = crate::release_render_transform(
                row.render_geometry,
                row.render_transform,
                *row.transform,
            );
            let changed = row.transform.rotation != *value;
            row.transform.rotation = *value;
            changed || render_changed
        }
        (Property::Scale, ReactiveValue::Vec2(value)) => {
            let render_changed = crate::release_render_transform(
                row.render_geometry,
                row.render_transform,
                *row.transform,
            );
            let changed = row.transform.scale != *value;
            row.transform.scale = *value;
            changed || render_changed
        }
        (Property::Opacity, ReactiveValue::Scalar(value)) => {
            let changed = row.style.opacity != *value;
            row.style.opacity = *value;
            changed
        }
        (Property::Appearance, ReactiveValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = *row.appearance != value;
            *row.appearance = value;
            changed
        }
        (Property::Reveal, ReactiveValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = *row.reveal != value;
            *row.reveal = value;
            changed
        }
        (Property::Morph, ReactiveValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = *row.morph != value;
            *row.morph = value;
            changed
        }
        (Property::Transform | Property::Fill | Property::Stroke, _) => {
            unreachable!("reactive values cannot drive object-snapshot or paint-color properties")
        }
        _ => unreachable!("validated reactive binding value type must match its property"),
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, RateFunction, ReactiveExpr, TrackTiming, Vec2};

    use super::*;

    #[test]
    fn initial_reactive_values_are_lowered_into_frame_state() {
        let mut scene = SemanticScene::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let position = scene.add_input(Vec2::new(3.0, -2.0));
        scene.bind(position, object, Property::Position);

        let instance = SceneInstance::from_semantic(&scene).expect("semantic scene must compile");
        assert_eq!(
            instance.frame().objects[0].transform.translation,
            Vec2::new(3.0, -2.0)
        );
    }

    #[test]
    fn reactive_scale_updates_transform_without_touching_geometry() {
        let mut scene = SemanticScene::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let scale = scene.add_input(Vec2::ONE);
        scene.bind(scale, object, Property::Scale);

        let mut instance =
            SceneInstance::from_semantic(&scene).expect("semantic scene must compile");
        assert_eq!(instance.frame().objects[0].transform.scale, Vec2::ONE);
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(1.0))
        );

        instance
            .set_reactive_input(scale, Vec2::new(0.25, 2.0))
            .expect("scale input update must work");
        assert_eq!(
            instance.frame().objects[0].transform.scale,
            Vec2::new(0.25, 2.0)
        );
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(1.0))
        );
    }

    #[test]
    fn input_update_mutates_only_lowered_dense_target() {
        let mut scene = SemanticScene::new();
        let untouched = scene.add(GeometryRef::circle(1.0));
        let target = scene.add(GeometryRef::circle(1.0));
        let input = scene.add_input(1.0_f32);
        let doubled = scene.add_derived(ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(input)),
            Box::new(ReactiveExpr::scalar(2.0)),
        ));
        scene.bind(doubled, target, Property::Rotation);

        let mut instance =
            SceneInstance::from_semantic(&scene).expect("semantic scene must compile");
        instance.take_frame_changes();
        instance
            .set_reactive_input(input, 2.0_f32)
            .expect("input update must work");

        assert_eq!(instance.frame().objects[0].id, untouched);
        assert_eq!(instance.frame().objects[0].transform.rotation, 0.0);
        assert_eq!(instance.frame().objects[1].id, target);
        assert_eq!(instance.frame().objects[1].transform.rotation, 4.0);
        assert_eq!(instance.take_frame_changes().object_indices(), &[1]);
        assert_eq!(
            instance.last_reactive_stats(),
            ReactiveRuntimeStats {
                derived_signals_evaluated: 1,
                bindings_invalidated: 1,
                dense_targets_applied: 1,
                dense_targets_changed: 1,
            }
        );
    }

    #[test]
    fn signal_only_reactive_update_publishes_frame_epoch_without_frame_dirtiness() {
        let mut scene = SemanticScene::new();
        scene.add(GeometryRef::circle(1.0));
        let input = scene.add_input(1.0_f32);
        let mut instance =
            SceneInstance::from_semantic(&scene).expect("semantic scene must compile");
        instance.take_frame_changes();
        let before = instance.publication_context();

        instance
            .set_reactive_input(input, 2.0_f32)
            .expect("input update must work");
        let after = instance.publication_context();
        assert_eq!(after.scene_revision(), before.scene_revision());
        assert_eq!(after.execution_revision(), before.execution_revision());
        assert_eq!(
            after.frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
        assert_eq!(
            instance.reactive_value(input),
            Some(&ReactiveValue::Scalar(2.0))
        );
        assert!(instance.take_frame_changes().is_empty());

        instance
            .set_reactive_input(input, 2.0_f32)
            .expect("same input is a valid no-op");
        assert_eq!(instance.publication_context(), after);
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn seeks_reapply_reactive_values_after_timeline_evaluation() {
        let mut scene = SemanticScene::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .definition_mut()
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            )
            .expect("timeline must be valid");
        let rotation = scene.add_input(0.25_f32);
        scene.bind(rotation, object, Property::Rotation);

        let mut instance =
            SceneInstance::from_semantic(&scene).expect("semantic scene must compile");
        instance
            .set_reactive_input(rotation, 1.25_f32)
            .expect("input update must work");
        instance.seek(1.0).expect("seek must work");

        assert_eq!(
            instance.frame().objects[0].transform.translation,
            Vec2::new(5.0, 0.0)
        );
        assert_eq!(instance.frame().objects[0].transform.rotation, 1.25);
    }

    #[test]
    fn reactive_update_cost_does_not_scale_with_static_object_count() {
        let mut scene = SemanticScene::new();
        for _ in 0..50_000 {
            scene.add(GeometryRef::circle(1.0));
        }
        let target = scene.add(GeometryRef::circle(1.0));
        let input = scene.add_input(1.0_f32);
        let doubled = scene.add_derived(ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(input)),
            Box::new(ReactiveExpr::scalar(2.0)),
        ));
        let shifted = scene.add_derived(ReactiveExpr::Add(
            Box::new(ReactiveExpr::signal(doubled)),
            Box::new(ReactiveExpr::scalar(1.0)),
        ));
        scene.bind(shifted, target, Property::Rotation);

        let mut instance =
            SceneInstance::from_semantic(&scene).expect("semantic scene must compile");
        instance.take_frame_changes();
        let timeline_stats_before = instance.last_stats();
        instance
            .set_reactive_input(input, 3.0_f32)
            .expect("input update must work");

        assert_eq!(instance.frame().objects[50_000].id, target);
        assert_eq!(instance.frame().objects[50_000].transform.rotation, 7.0);
        assert_eq!(instance.take_frame_changes().object_indices(), &[50_000]);
        assert_eq!(instance.last_stats(), timeline_stats_before);
        assert_eq!(
            instance.last_reactive_stats(),
            ReactiveRuntimeStats {
                derived_signals_evaluated: 2,
                bindings_invalidated: 1,
                dense_targets_applied: 1,
                dense_targets_changed: 1,
            }
        );
    }

    #[test]
    fn removed_reactive_target_stays_hidden_and_rebinds_on_same_id_recreate() {
        let mut scene = SemanticScene::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let visible = scene.add_input(true);
        scene.bind(visible, object, Property::Presence);

        let mut instance =
            SceneInstance::from_semantic(&scene).expect("semantic scene must compile");
        instance
            .apply_patch(&noon_core::ScenePatch::RemoveObject(object))
            .expect("remove must compile");
        assert!(!instance.frame().presences[0]);

        instance
            .set_reactive_input(visible, false)
            .expect("removed target update is still a valid signal update");
        instance
            .set_reactive_input(visible, true)
            .expect("removed target update is still a valid signal update");
        assert!(!instance.frame().presences[0]);
        assert_eq!(instance.last_reactive_stats().dense_targets_applied, 0);

        instance
            .apply_patch(&noon_core::ScenePatch::CreateObject(
                noon_core::ObjectDefinition::new(object, GeometryRef::rectangle(2.0, 1.0)),
            ))
            .expect("same identity may be recreated after removal");
        assert_eq!(instance.frame().objects.len(), 2);
        assert!(!instance.frame().presences[0]);
        assert_eq!(instance.frame().objects[1].id, object);
        assert!(instance.frame().presences[1]);

        instance
            .set_reactive_input(visible, false)
            .expect("rebound target update must apply");
        assert!(!instance.frame().presences[0]);
        assert!(!instance.frame().presences[1]);
        assert_eq!(instance.last_reactive_stats().dense_targets_applied, 1);
    }
}
