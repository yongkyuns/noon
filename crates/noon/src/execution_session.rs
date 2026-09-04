use noon_compile::{
    lower_semantic_execution, SemanticExecutionIndex, SemanticExecutionLoweringError,
    SemanticReactiveProjection,
};
use noon_core::{ObjectId, ReactiveError, ReactiveValue, SemanticNodeId, SemanticStore};
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
    use noon_core::{SemanticObjectProperty, SemanticObjectState, StoredGeometry};

    use super::*;

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
}
