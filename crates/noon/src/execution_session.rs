use noon_compile::{
    lower_semantic_execution, SemanticExecutionIndex, SemanticExecutionLoweringError,
    SemanticReactiveProjection,
};
use noon_core::{ObjectId, SemanticNodeId, SemanticStore, SignalId};
use noon_runtime::{FrameChanges, FrameState, SceneInstance};

/// Thin typed orchestration from authoritative semantic state into Noon's existing runtime.
///
/// The session does not own or mirror the [`SemanticStore`]. It retains only the
/// compiler-owned semantic/execution identity mappings needed by hosts plus the
/// existing [`SceneInstance`] runtime. Renderer and platform lifecycle remain outside
/// this type.
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

    pub fn runtime(&self) -> &SceneInstance {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut SceneInstance {
        &mut self.runtime
    }

    /// Resolve an authoritative semantic object identity to its current execution key.
    pub fn execution_object_id(&self, node: SemanticNodeId) -> Option<ObjectId> {
        self.execution_index.execution_object_id(node)
    }

    /// Resolve an authoritative semantic signal identity to the existing reactive VM key.
    pub fn execution_signal_id(&self, node: SemanticNodeId) -> Option<SignalId> {
        self.reactive_projection.execution_signal_id(node)
    }

    pub fn into_runtime(self) -> SceneInstance {
        self.runtime
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
        let object = store.insert_semantic_object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 2.0 },
        ));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let execution_object = session.execution_object_id(object).unwrap();
        let execution_signal = session.execution_signal_id(signal).unwrap();

        assert_eq!(session.frame().objects.len(), 1);
        assert_eq!(session.frame().objects[0].id, execution_object);
        assert_eq!(session.frame().objects[0].style.opacity, 0.4);

        session.take_frame_changes();
        session
            .runtime_mut()
            .set_reactive_input(execution_signal, 0.7_f32)
            .unwrap();

        assert_eq!(session.frame().objects[0].style.opacity, 0.7);
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);
    }
}
