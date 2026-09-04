use noon_core::{ComputeProgram, ReactiveProgram, SemanticStore};

use crate::CompiledScene;

use super::{
    lower_semantic_reactive_projection, SemanticCompiledSceneError, SemanticExecutionIndex,
    SemanticLoweringError, SemanticReactiveLoweringError, SemanticReactiveProjection,
};

/// One typed compiler handoff from the authoritative semantic scene into Noon's
/// existing execution representations.
///
/// This is a composition boundary, not a second runtime scene model: object/timeline
/// storage remains `CompiledScene`, native reactive execution remains the existing
/// compute VM, and durable runtime identity remains owned by `ExecutionSlotTable`.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticExecutionLoweringOutput {
    compiled: CompiledScene,
    reactive: SemanticReactiveProjection,
    compute: ComputeProgram,
}

impl SemanticExecutionLoweringOutput {
    pub fn compiled(&self) -> &CompiledScene {
        &self.compiled
    }

    pub fn reactive(&self) -> &SemanticReactiveProjection {
        &self.reactive
    }

    pub fn compute(&self) -> &ComputeProgram {
        &self.compute
    }

    /// Compatibility decomposition retained while callers migrate to consuming the
    /// complete canonical execution handoff.
    pub fn into_parts(self) -> (CompiledScene, SemanticReactiveProjection) {
        (self.compiled, self.reactive)
    }

    pub fn into_execution_parts(
        self,
    ) -> (CompiledScene, SemanticReactiveProjection, ComputeProgram) {
        (self.compiled, self.reactive, self.compute)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticExecutionLoweringError {
    Object(SemanticLoweringError),
    Reactive(SemanticReactiveLoweringError),
    Compiled(SemanticCompiledSceneError),
}

impl From<SemanticLoweringError> for SemanticExecutionLoweringError {
    fn from(value: SemanticLoweringError) -> Self {
        Self::Object(value)
    }
}

impl From<SemanticReactiveLoweringError> for SemanticExecutionLoweringError {
    fn from(value: SemanticReactiveLoweringError) -> Self {
        Self::Reactive(value)
    }
}

impl From<SemanticCompiledSceneError> for SemanticExecutionLoweringError {
    fn from(value: SemanticCompiledSceneError) -> Self {
        Self::Compiled(value)
    }
}

impl std::fmt::Display for SemanticExecutionLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Object(error) => write!(formatter, "semantic object lowering failed: {error}"),
            Self::Reactive(error) => {
                write!(formatter, "semantic reactive lowering failed: {error}")
            }
            Self::Compiled(error) => {
                write!(formatter, "compiled execution lowering failed: {error}")
            }
        }
    }
}

impl std::error::Error for SemanticExecutionLoweringError {}

/// Canonical A1.6 initial-scene lowering entry point.
///
/// Object values and active native-reactive bindings are lowered from the same
/// semantic snapshot. The reactive graph is validated against the already-lowered
/// execution object/timeline domain and lowered into the existing compute VM. The
/// identity index is staged and published only after all downstream lowering
/// succeeds, including compute-program construction, so an invalid execution
/// handoff cannot leave a partially admitted semantic-to-execution mapping.
///
/// Authored animation declarations remain semantic intent until an explicit
/// animation activation/composition root is supplied to its dedicated lowering
/// channel; detached declarations are not implicitly scheduled by initial-scene
/// lowering.
pub fn lower_semantic_execution(
    store: &SemanticStore,
    index: &mut SemanticExecutionIndex,
) -> Result<SemanticExecutionLoweringOutput, SemanticExecutionLoweringError> {
    let mut staged_index = index.clone();
    let projection = staged_index.lower_scene(store)?;
    let reactive = lower_semantic_reactive_projection(store, &projection)?;
    let compiled = CompiledScene::from_semantic_projection_after_reactive_lowering(&projection)?;
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
        reactive.graph(),
    )
    .map_err(SemanticReactiveLoweringError::from)?;
    let compute = program
        .into_compute()
        .map_err(SemanticReactiveLoweringError::from)?;

    *index = staged_index;
    Ok(SemanticExecutionLoweringOutput {
        compiled,
        reactive,
        compute,
    })
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Property, ReactiveValue, SemanticObjectProperty, SemanticObjectState, SemanticStore,
        StoredGeometry, TextResourceHandle, TextResourceId,
    };

    use super::*;

    fn circle(radius: f32) -> SemanticObjectState {
        SemanticObjectState::new(StoredGeometry::Circle { radius })
    }

    #[test]
    fn canonical_entry_lowers_object_values_and_reactivity_into_existing_compute_vm() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        let object = store.insert_semantic_object(circle(2.0));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&store, &mut index).unwrap();
        let execution_id = index.execution_object_id(object).unwrap();
        let execution_signal = lowered.reactive().execution_signal_id(signal).unwrap();

        assert_eq!(lowered.compiled().objects().len(), 1);
        assert_eq!(lowered.compiled().objects()[0].id, execution_id);
        assert_eq!(lowered.reactive().signal_count(), 1);
        assert_eq!(lowered.compute().signal_count(), 1);
        assert_eq!(
            lowered.reactive().graph().bindings()[0].object,
            execution_id
        );
        assert_eq!(
            lowered.reactive().graph().bindings()[0].property,
            Property::Opacity
        );
        assert_eq!(
            lowered.reactive().graph().bindings()[0].signal,
            execution_signal
        );

        let mut compute = lowered.compute().clone().instantiate();
        let update = compute.set_input(execution_signal, 0.7_f32).unwrap();
        assert_eq!(update.affected_objects(), vec![execution_id]);
        assert_eq!(
            update.property_changes(),
            &[noon_core::ReactivePropertyChange {
                object: execution_id,
                property: Property::Opacity,
                value: ReactiveValue::Scalar(0.7),
            }]
        );
    }

    #[test]
    fn reactive_failure_does_not_publish_staged_execution_identity() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        let object = store.insert_semantic_object(circle(1.0));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::FillOpacity)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        assert!(matches!(
            lower_semantic_execution(&store, &mut index),
            Err(SemanticExecutionLoweringError::Reactive(
                SemanticReactiveLoweringError::UnsupportedProperty {
                    target,
                    property: SemanticObjectProperty::FillOpacity,
                }
            )) if target == object
        ));
        assert!(index.is_empty());
    }

    #[test]
    fn compiled_payload_failure_does_not_publish_staged_execution_identity() {
        let mut store = SemanticStore::new();
        let object = store.insert_semantic_object(SemanticObjectState::new(TextResourceHandle {
            id: TextResourceId::new(9),
            version: 2,
        }));
        store.attach_to_scene(object).unwrap();

        let mut index = SemanticExecutionIndex::new();
        assert!(matches!(
            lower_semantic_execution(&store, &mut index),
            Err(SemanticExecutionLoweringError::Compiled(
                SemanticCompiledSceneError::UnsupportedText { node, .. }
            )) if node == object
        ));
        assert!(index.is_empty());
    }
}
