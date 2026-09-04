use noon_compile::{
    lower_semantic_affine_animation_tracks, lower_semantic_animation_schedule,
    lower_semantic_execution, CompilePatchError, SemanticAffineAnimationTrackError,
    SemanticAnimationScheduleError, SemanticExecutionIndex, SemanticExecutionLoweringError,
    SemanticReactiveProjection,
};
use noon_core::{
    AnimationOptions, MutationTransaction, ObjectId, ReactiveError, ReactiveValue, ScenePatch,
    SemanticNodeId, SemanticStore, TrackId,
};
use noon_runtime::{EvaluationError, FrameChanges, FrameState, SceneInstance};

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

/// Error produced when a semantic animation cannot be activated into this execution session.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionAnimationError {
    Schedule(SemanticAnimationScheduleError),
    AffinePayload(SemanticAffineAnimationTrackError),
    TrackIdExhausted,
    Publication(CompilePatchError),
}

impl std::fmt::Display for ExecutionSessionAnimationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Schedule(error) => error.fmt(formatter),
            Self::AffinePayload(error) => error.fmt(formatter),
            Self::TrackIdExhausted => {
                formatter.write_str("execution animation TrackId space exhausted")
            }
            Self::Publication(error) => {
                write!(formatter, "execution animation publication failed: {error}")
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
        Self::AffinePayload(value)
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
/// this type.
///
/// Runtime mutation is intentionally not exposed as a mutable escape hatch here:
/// authored/live structural mutation remains owned by semantic transactions and
/// incremental lowering rather than the migration-era runtime patch surface.
#[derive(Clone, Debug)]
pub struct ExecutionSession {
    execution_index: SemanticExecutionIndex,
    reactive_projection: SemanticReactiveProjection,
    runtime: SceneInstance,
    next_execution_track_id: Option<u64>,
}

impl ExecutionSession {
    /// Lower one authoritative semantic snapshot and instantiate the existing runtime.
    pub fn from_semantic_store(
        store: &SemanticStore,
    ) -> Result<Self, SemanticExecutionLoweringError> {
        let mut execution_index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(store, &mut execution_index)?;
        let reactive_projection = lowered.reactive().clone();
        let runtime = SceneInstance::from_semantic_execution(lowered);
        Ok(Self {
            execution_index,
            reactive_projection,
            runtime,
            // Canonical initial semantic lowering currently emits no execution tracks.
            // Activation IDs are session-owned and advance only after atomic publication.
            next_execution_track_id: Some(0),
        })
    }

    /// Current renderer-facing runtime frame.
    pub fn frame(&self) -> &FrameState {
        self.runtime.frame()
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

    /// Activate one authoritative semantic animation at the current session time.
    ///
    /// Scheduling and payload lowering remain compiler-owned. The runtime contributes only
    /// activation-time effective transforms and atomically publishes the resulting existing
    /// `ScenePatch::AddTrack` operations. Execution TrackIds are session-local and are not
    /// committed until publication succeeds.
    pub fn activate_animation(
        &mut self,
        store: &SemanticStore,
        animation: SemanticNodeId,
        play_options: AnimationOptions,
    ) -> Result<&FrameState, ExecutionSessionAnimationError> {
        let schedule = lower_semantic_animation_schedule(
            store,
            &self.execution_index,
            animation,
            self.runtime.frame().time,
            play_options,
        )?;
        let tracks = lower_semantic_affine_animation_tracks(store, &schedule, |object| {
            self.runtime.effective_transform(object)
        })?;

        let mut next_execution_track_id = self.next_execution_track_id;
        let mut transaction = MutationTransaction::new();
        for track in tracks.tracks() {
            let raw = next_execution_track_id
                .ok_or(ExecutionSessionAnimationError::TrackIdExhausted)?;
            transaction.push(ScenePatch::AddTrack(track.with_track_id(TrackId::new(raw))?));
            next_execution_track_id = raw.checked_add(1);
        }

        self.runtime.apply_transaction(&transaction)?;
        self.next_execution_track_id = next_execution_track_id;
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

    /// Resolve an authoritative semantic object identity to its current execution key.
    pub fn execution_object_id(&self, node: SemanticNodeId) -> Option<ObjectId> {
        self.execution_index.execution_object_id(node)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, RateFunction, SemanticObjectProperty, SemanticObjectState, SemanticVec3,
        StoredGeometry, Vec2,
    };

    use super::*;

    fn visible_circle(store: &mut SemanticStore) -> SemanticNodeId {
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        object
    }

    fn transform_state(
        store: &mut SemanticStore,
        source: SemanticNodeId,
        translation_x: f64,
        rotation_z: f64,
        scale_x: f64,
        scale_y: f64,
    ) -> SemanticNodeId {
        let mut state = store.semantic_object_state_checked(source).unwrap().clone();
        state.transform.translation = SemanticVec3::new(translation_x, 0.0, 0.0);
        state.transform.rotation_z = rotation_z;
        state.transform.scale = SemanticVec3::new(scale_x, scale_y, 1.0);
        store.insert_semantic_object(state)
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

        session.take_frame_changes();
        session.set_reactive_input(signal, 0.7_f32).unwrap();

        assert_eq!(session.frame().objects[0].style.opacity, 0.7);
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);

        session.seek(1.25).unwrap();
        assert_eq!(session.frame().time, 1.25);
        assert_eq!(session.frame().objects[0].style.opacity, 0.7);
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
    fn animation_activation_chains_from_effective_state_at_current_session_time() {
        let mut store = SemanticStore::new();
        let object = visible_circle(&mut store);
        let first_state = transform_state(&mut store, object, 4.0, 0.4, 2.0, 2.0);
        let second_state = transform_state(&mut store, object, 10.0, 1.0, 4.0, 3.0);
        let first = store
            .insert_semantic_transform_animation(
                object,
                first_state,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let second = store
            .insert_semantic_transform_animation(
                object,
                second_state,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        session
            .activate_animation(&store, first, AnimationOptions::new())
            .unwrap();
        session.seek(1.0).unwrap();
        let captured = session.frame().objects[0].transform;
        assert_eq!(captured.translation, Vec2::new(2.0, 0.0));
        assert!((captured.rotation - 0.2).abs() < 1.0e-6);
        assert_eq!(captured.scale, Vec2::new(1.5, 1.5));

        session
            .activate_animation(&store, second, AnimationOptions::new())
            .unwrap();
        assert_eq!(session.frame().objects[0].transform, captured);

        session.seek(2.0).unwrap();
        let halfway = session.frame().objects[0].transform;
        assert_eq!(halfway.translation, Vec2::new(6.0, 0.0));
        assert!((halfway.rotation - 0.6).abs() < 1.0e-6);
        assert_eq!(halfway.scale, Vec2::new(2.75, 2.25));
    }

    #[test]
    fn execution_track_id_commits_only_after_successful_publication() {
        let mut store = SemanticStore::new();
        let object = visible_circle(&mut store);
        let first_state = transform_state(&mut store, object, 4.0, 0.0, 1.0, 1.0);
        let second_state = transform_state(&mut store, object, 8.0, 0.0, 1.0, 1.0);
        let first = store
            .insert_semantic_transform_animation(
                object,
                first_state,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let second = store
            .insert_semantic_transform_animation(
                object,
                second_state,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        session
            .activate_animation(&store, first, AnimationOptions::new())
            .unwrap();
        assert_eq!(session.next_execution_track_id, Some(1));

        session.next_execution_track_id = Some(0);
        assert_eq!(
            session.activate_animation(&store, second, AnimationOptions::new()),
            Err(ExecutionSessionAnimationError::Publication(
                CompilePatchError::DuplicateTrack(TrackId::new(0))
            ))
        );
        assert_eq!(session.next_execution_track_id, Some(0));

        session.seek(1.0).unwrap();
        assert_eq!(
            session.frame().objects[0].transform.translation,
            Vec2::new(2.0, 0.0)
        );
    }
}
