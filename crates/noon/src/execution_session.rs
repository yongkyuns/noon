use noon_compile::{
    lower_semantic_affine_animation_tracks, lower_semantic_animation_schedule,
    lower_semantic_execution, lower_semantic_native_inputs, CompilePatchError,
    SemanticAffineAnimationTrackError, SemanticAnimationScheduleError, SemanticExecutionIndex,
    SemanticExecutionLoweringError, SemanticExecutionLoweringOutput,
    SemanticNativeInputLoweringError, SemanticReactiveProjection,
};
use noon_core::{
    AnimationOptions, Camera2DState, MutationTransaction, NativeEventSource, NativeStateUpdate,
    ObjectId, ReactiveError, ReactiveValue, ScenePatch, SemanticNativeInputDefinition,
    SemanticNodeId, SemanticStore, TrackId,
};
use noon_runtime::{
    EvaluationError, FrameChanges, FrameState, NativeInputRouter, NativeInputStats, SceneInstance,
};

/// Error produced while constructing a session with semantic native-input declarations.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionBuildError {
    Execution(SemanticExecutionLoweringError),
    NativeInput(SemanticNativeInputLoweringError),
}

impl std::fmt::Display for ExecutionSessionBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execution(error) => error.fmt(formatter),
            Self::NativeInput(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionSessionBuildError {}

impl From<SemanticExecutionLoweringError> for ExecutionSessionBuildError {
    fn from(value: SemanticExecutionLoweringError) -> Self {
        Self::Execution(value)
    }
}

impl From<SemanticNativeInputLoweringError> for ExecutionSessionBuildError {
    fn from(value: SemanticNativeInputLoweringError) -> Self {
        Self::NativeInput(value)
    }
}

/// Error produced when a semantic reactive input cannot be applied to this execution session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionSessionInputError {
    UnknownSemanticSignal(SemanticNodeId),
    Reactive(ReactiveError),
}

impl std::fmt::Display for ExecutionSessionInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSemanticSignal(signal) => write!(
                formatter,
                "semantic signal {}:{} is not present in this execution session",
                signal.slot(),
                signal.generation()
            ),
            Self::Reactive(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionSessionInputError {}

impl From<ReactiveError> for ExecutionSessionInputError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

/// Error produced when the canonical semantic camera cannot be derived from the
/// current effective runtime frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionSessionCameraError {
    object: ObjectId,
}

impl ExecutionSessionCameraError {
    pub const fn object(self) -> ObjectId {
        self.object
    }
}

impl std::fmt::Display for ExecutionSessionCameraError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "execution camera object {} is missing or not a valid 2D frame",
            self.object.get()
        )
    }
}

impl std::error::Error for ExecutionSessionCameraError {}

/// Error produced while activating one authoritative semantic animation declaration.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionAnimationError {
    Schedule(SemanticAnimationScheduleError),
    Payload(SemanticAffineAnimationTrackError),
    Publication(CompilePatchError),
    TrackIdExhausted,
}

impl std::fmt::Display for ExecutionSessionAnimationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Schedule(error) => {
                write!(formatter, "semantic animation scheduling failed: {error}")
            }
            Self::Payload(error) => write!(
                formatter,
                "semantic animation payload lowering failed: {error}"
            ),
            Self::Publication(error) => {
                write!(formatter, "semantic animation publication failed: {error}")
            }
            Self::TrackIdExhausted => {
                formatter.write_str("execution animation track ID space exhausted")
            }
        }
    }
}

impl std::error::Error for ExecutionSessionAnimationError {}

impl From<SemanticAnimationScheduleError> for ExecutionSessionAnimationError {
    fn from(value: SemanticAnimationScheduleError) -> Self {
        Self::Schedule(value)
    }
}

impl From<SemanticAffineAnimationTrackError> for ExecutionSessionAnimationError {
    fn from(value: SemanticAffineAnimationTrackError) -> Self {
        Self::Payload(value)
    }
}

impl From<CompilePatchError> for ExecutionSessionAnimationError {
    fn from(value: CompilePatchError) -> Self {
        Self::Publication(value)
    }
}

/// Thin typed orchestration from authoritative semantic state into Noon's existing runtime.
///
/// The session does not own or mirror the [`SemanticStore`]. It retains only the
/// compiler-owned semantic/execution identity mappings needed by hosts plus the
/// existing [`SceneInstance`] runtime. Renderer and platform lifecycle remain outside
/// this type. The canonical camera is retained only as the execution identity of its
/// ordinary semantic frame object; its value is always derived from the effective
/// runtime frame.
///
/// Runtime mutation is intentionally not exposed as a mutable escape hatch here:
/// authored/live structural mutation remains owned by semantic transactions and
/// incremental lowering rather than the migration-era runtime patch surface.
#[derive(Clone, Debug)]
pub struct ExecutionSession {
    execution_index: SemanticExecutionIndex,
    reactive_projection: SemanticReactiveProjection,
    runtime: SceneInstance,
    native_inputs: NativeInputRouter,
    camera_object: Option<ObjectId>,
    next_activation_track_id: Option<u64>,
}

impl ExecutionSession {
    /// Lower one authoritative semantic snapshot and instantiate the existing runtime.
    pub fn from_semantic_store(
        store: &SemanticStore,
    ) -> Result<Self, SemanticExecutionLoweringError> {
        let mut execution_index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(store, &mut execution_index)?;
        Ok(Self::from_lowered(
            execution_index,
            lowered,
            NativeInputRouter::default(),
        ))
    }

    /// Lower one authoritative semantic snapshot plus native-input declarations.
    ///
    /// Scene/reactive/input lowering occurs under the same immutable store borrow, so
    /// a caller cannot validate native input identity against a later semantic mutation
    /// while retaining an older execution projection.
    pub fn from_semantic_store_with_native_inputs(
        store: &SemanticStore,
        inputs: &SemanticNativeInputDefinition,
    ) -> Result<Self, ExecutionSessionBuildError> {
        let mut execution_index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(store, &mut execution_index)?;
        let native_definition = lower_semantic_native_inputs(store, inputs, lowered.reactive())?;
        let native_inputs =
            NativeInputRouter::from_definition(lowered.reactive().graph(), &native_definition)
                .map_err(|error| {
                    ExecutionSessionBuildError::NativeInput(
                        SemanticNativeInputLoweringError::Execution(error),
                    )
                })?;
        Ok(Self::from_lowered(execution_index, lowered, native_inputs))
    }

    fn from_lowered(
        execution_index: SemanticExecutionIndex,
        lowered: SemanticExecutionLoweringOutput,
        native_inputs: NativeInputRouter,
    ) -> Self {
        let camera_object = lowered.camera_object();
        let next_activation_track_id = lowered
            .compiled()
            .tracks_iter()
            .map(|track| track.id.get())
            .max()
            .map_or(Some(0), |id| id.checked_add(1));
        let reactive_projection = lowered.reactive().clone();
        let runtime = SceneInstance::from_semantic_execution(lowered);
        Self {
            execution_index,
            reactive_projection,
            runtime,
            native_inputs,
            camera_object,
            next_activation_track_id,
        }
    }

    /// Current renderer-facing runtime frame.
    pub fn frame(&self) -> &FrameState {
        self.runtime.frame()
    }

    /// Current canonical 2D camera derived from the effective runtime frame object.
    ///
    /// Scenes without an authored camera use the shared Manim-compatible default.
    /// An authored camera never falls back silently: if its effective frame becomes
    /// invalid, the host receives an error just as the transport encoder does.
    pub fn camera(&self) -> Result<Camera2DState, ExecutionSessionCameraError> {
        let Some(camera_object) = self.camera_object else {
            return Ok(Camera2DState::default());
        };
        let object = self
            .runtime
            .frame()
            .objects
            .iter()
            .find(|object| object.id == camera_object)
            .ok_or(ExecutionSessionCameraError {
                object: camera_object,
            })?;
        Camera2DState::from_frame_object(&object.geometry, object.transform).ok_or(
            ExecutionSessionCameraError {
                object: camera_object,
            },
        )
    }

    /// Consume renderer-facing invalidation state accumulated by the runtime.
    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.runtime.take_frame_changes()
    }

    /// Evaluate deterministically at an absolute time.
    pub fn evaluate(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.runtime.evaluate(time)
    }

    /// Seek deterministically to an absolute time.
    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.runtime.seek(time)
    }

    /// Advance to an absolute time, falling back to deterministic seek when time moves backward.
    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.runtime.advance_to(time)
    }

    /// Activate one semantic animation root at the session's current deterministic time.
    ///
    /// The authoritative semantic store remains caller-owned. Scheduling reads the
    /// current declaration, affine payload lowering captures each target's effective
    /// runtime transform at most once, and the session attaches execution-local track
    /// identity. All emitted tracks are then preflighted and published as one existing
    /// [`MutationTransaction`], so a failed activation cannot expose a partial timeline.
    pub fn activate_animation(
        &mut self,
        store: &SemanticStore,
        root: SemanticNodeId,
        play_options: AnimationOptions,
    ) -> Result<&FrameState, ExecutionSessionAnimationError> {
        let schedule = lower_semantic_animation_schedule(
            store,
            &self.execution_index,
            root,
            self.runtime.frame().time,
            play_options,
        )?;
        let tracks = lower_semantic_affine_animation_tracks(store, &schedule, |object| {
            self.runtime.effective_transform(object)
        })?;
        if tracks.is_empty() {
            return Ok(self.runtime.frame());
        }

        let mut next_track_id = self.next_activation_track_id;
        let mut mutations = Vec::with_capacity(tracks.len());
        for track in tracks.tracks() {
            let raw_id = next_track_id.ok_or(ExecutionSessionAnimationError::TrackIdExhausted)?;
            let definition = track.with_track_id(TrackId::new(raw_id))?;
            mutations.push(ScenePatch::AddTrack(definition));
            next_track_id = raw_id.checked_add(1);
        }

        let transaction = MutationTransaction::from_mutations(mutations);
        self.runtime.apply_transaction(&transaction)?;
        self.next_activation_track_id = next_track_id;
        Ok(self.runtime.frame())
    }

    /// Apply one native-reactive input using authoritative semantic signal identity.
    ///
    /// The execution VM key remains an internal lowering detail. Current native
    /// reactive values use the compact execution domain (`bool`, `f32`, `Vec2`);
    /// semantic structural/property mutation remains outside this method.
    pub fn set_reactive_input(
        &mut self,
        signal: SemanticNodeId,
        value: impl Into<ReactiveValue>,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        let execution_signal = self
            .reactive_projection
            .execution_signal_id(signal)
            .ok_or(ExecutionSessionInputError::UnknownSemanticSignal(signal))?;
        Ok(self.runtime.set_reactive_input(execution_signal, value)?)
    }

    /// Dispatch one validated sampled native input through the shared runtime router.
    pub fn dispatch_native_state(
        &mut self,
        update: NativeStateUpdate,
    ) -> Result<bool, ExecutionSessionInputError> {
        Ok(self.native_inputs.dispatch_state_scene(
            &mut self.runtime,
            &update.source,
            update.value,
        )?)
    }

    /// Dispatch one normalized discrete native event through the shared runtime router.
    pub fn emit_native_event(
        &mut self,
        source: &NativeEventSource,
    ) -> Result<bool, ExecutionSessionInputError> {
        Ok(self
            .native_inputs
            .emit_event_scene(&mut self.runtime, source)?)
    }

    /// Work counters from the shared native-input router.
    pub const fn native_input_stats(&self) -> NativeInputStats {
        self.native_inputs.stats()
    }

    /// Resolve an authoritative semantic object identity to its current execution key.
    pub fn execution_object_id(&self, node: SemanticNodeId) -> Option<ObjectId> {
        self.execution_index.execution_object_id(node)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, NativeEventSource, NativeInputValue, NativeStateSource, RateFunction,
        SemanticNativeInputDefinition, SemanticObjectProperty, SemanticObjectRole,
        SemanticObjectState, SemanticVec3, StoredGeometry, Vec2,
    };

    use super::*;

    fn linear_second() -> AnimationOptions {
        AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
    }

    fn camera_state(center: Vec2, height: f32) -> SemanticObjectState {
        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(height * 16.0 / 9.0, height),
        });
        state.transform.translation = SemanticVec3::new(center.x as f64, center.y as f64, 0.0);
        state.set_role(SemanticObjectRole::Camera2D);
        state
    }

    #[test]
    fn semantic_store_runs_through_typed_session_into_renderer_facing_changes() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 2.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let execution_object = session.execution_object_id(object).unwrap();

        assert_eq!(session.frame().objects.len(), 1);
        assert_eq!(session.frame().objects[0].id, execution_object);
        assert_eq!(session.frame().objects[0].style.opacity, 0.4);
        assert_eq!(session.camera().unwrap(), Camera2DState::default());

        session.take_frame_changes();
        session.set_reactive_input(signal, 0.7_f32).unwrap();

        assert_eq!(session.frame().objects[0].style.opacity, 0.7);
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);

        session.seek(1.25).unwrap();
        assert_eq!(session.frame().time, 1.25);
        assert_eq!(session.frame().objects[0].style.opacity, 0.7);
    }

    #[test]
    fn canonical_camera_is_derived_from_effective_runtime_transform() {
        let mut store = SemanticStore::new();
        let camera = store.insert_semantic_object(camera_state(Vec2::new(1.0, 2.0), 8.0));
        store.attach_to_scene(camera).unwrap();

        let mut target_state = store.semantic_object_state_checked(camera).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(5.0, -2.0, 0.0);
        target_state.transform.scale = SemanticVec3::new(1.0, 0.5, 1.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(camera, target, AnimationOptions::new())
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        assert_eq!(
            session.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(1.0, 2.0),
                height: 8.0,
            }
        );

        session
            .activate_animation(&store, animation, linear_second())
            .unwrap();
        session.seek(0.5).unwrap();
        assert_eq!(
            session.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(3.0, 0.0),
                height: 6.0,
            }
        );
    }

    #[test]
    fn execution_vm_signal_identity_does_not_escape_the_session_surface() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        assert_eq!(
            session.set_reactive_input(object, 1.0_f32),
            Err(ExecutionSessionInputError::UnknownSemanticSignal(object))
        );
    }

    #[test]
    fn semantic_native_inputs_use_shared_router_without_exposing_execution_signal_ids() {
        let mut store = SemanticStore::new();
        let pointer = store
            .insert_semantic_input_signal(SemanticVec3::new(0.0, 0.0, 0.0))
            .unwrap();
        let clicks = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(pointer, object, SemanticObjectProperty::Position)
            .unwrap();
        store
            .bind_semantic_signal(clicks, object, SemanticObjectProperty::RotationZ)
            .unwrap();

        let mut inputs = SemanticNativeInputDefinition::new();
        inputs
            .bind_state(NativeStateSource::PointerPosition, pointer)
            .bind_event(NativeEventSource::PointerDown { button: 0 }, clicks);

        let mut session =
            ExecutionSession::from_semantic_store_with_native_inputs(&store, &inputs).unwrap();
        session.take_frame_changes();

        assert!(session
            .dispatch_native_state(
                NativeStateUpdate::new(
                    NativeStateSource::PointerPosition,
                    NativeInputValue::Vec2(Vec2::new(2.0, -1.0)),
                )
                .unwrap(),
            )
            .unwrap());
        assert_eq!(
            session.frame().objects[0].transform.translation,
            Vec2::new(2.0, -1.0)
        );
        assert!(!session
            .dispatch_native_state(
                NativeStateUpdate::new(
                    NativeStateSource::PointerPosition,
                    NativeInputValue::Vec2(Vec2::new(2.0, -1.0)),
                )
                .unwrap(),
            )
            .unwrap());

        let click = NativeEventSource::PointerDown { button: 0 };
        assert!(session.emit_native_event(&click).unwrap());
        assert!(session.emit_native_event(&click).unwrap());
        assert_eq!(session.frame().objects[0].transform.rotation, 2.0);
        assert_eq!(session.native_input_stats().state_samples_coalesced, 1);
        assert_eq!(session.native_input_stats().events_received, 2);
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);
    }

    #[test]
    fn chained_activation_starts_from_current_effective_affine_state() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();

        let mut first_state = store.semantic_object_state_checked(object).unwrap().clone();
        first_state.transform.translation = SemanticVec3::new(4.0, 0.0, 0.0);
        first_state.transform.rotation_z = 0.5;
        first_state.transform.scale = SemanticVec3::new(2.0, 2.0, 1.0);
        let first_state = store.insert_semantic_object(first_state);
        let first = store
            .insert_semantic_transform_animation(object, first_state, AnimationOptions::new())
            .unwrap();

        let mut second_state = store.semantic_object_state_checked(object).unwrap().clone();
        second_state.transform.translation = SemanticVec3::new(10.0, 0.0, 0.0);
        second_state.transform.rotation_z = 1.5;
        second_state.transform.scale = SemanticVec3::new(4.0, 1.0, 1.0);
        let second_state = store.insert_semantic_object(second_state);
        let second = store
            .insert_semantic_transform_animation(object, second_state, AnimationOptions::new())
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session
            .activate_animation(&store, first, linear_second())
            .unwrap();
        session.seek(1.0).unwrap();
        assert_eq!(
            session.frame().objects[0].transform,
            noon_core::Transform2D {
                translation: Vec2::new(4.0, 0.0),
                rotation: 0.5,
                scale: Vec2::new(2.0, 2.0),
            }
        );

        session
            .activate_animation(&store, second, linear_second())
            .unwrap();
        session.seek(1.5).unwrap();

        assert_eq!(
            session.frame().objects[0].transform,
            noon_core::Transform2D {
                translation: Vec2::new(7.0, 0.0),
                rotation: 1.0,
                scale: Vec2::new(3.0, 1.5),
            }
        );
    }
}
