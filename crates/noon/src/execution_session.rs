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
    next_activation_track_id: Option<u64>,
}

impl ExecutionSession {
    /// Lower one authoritative semantic snapshot and instantiate the existing runtime.
    pub fn from_semantic_store(
        store: &SemanticStore,
    ) -> Result<Self, SemanticExecutionLoweringError> {
        let mut execution_index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(store, &mut execution_index)?;
        let next_activation_track_id = lowered
            .compiled()
            .tracks_iter()
            .map(|track| track.id.get())
            .max()
            .map_or(Some(0), |id| id.checked_add(1));
        let reactive_projection = lowered.reactive().clone();
        let runtime = SceneInstance::from_semantic_execution(lowered);
        Ok(Self {
            execution_index,
            reactive_projection,
            runtime,
            next_activation_track_id,
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

    fn linear_second() -> AnimationOptions {
        AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
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
