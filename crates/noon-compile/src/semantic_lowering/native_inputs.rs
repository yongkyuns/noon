use noon_core::{
    NativeInputDefinition, NativeInputError, SemanticNativeInputBinding,
    SemanticNativeInputDefinition, SemanticNativeInputError, SemanticNodeId, SemanticStore,
};

use super::SemanticReactiveProjection;

/// Failure while translating authoritative semantic native-input declarations into
/// the existing execution router vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticNativeInputLoweringError {
    Definition(SemanticNativeInputError),
    SignalNotLowered(SemanticNodeId),
    Execution(NativeInputError),
}

impl std::fmt::Display for SemanticNativeInputLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Definition(error) => error.fmt(formatter),
            Self::SignalNotLowered(signal) => write!(
                formatter,
                "semantic native input signal {}:{} is not reachable in this execution projection",
                signal.slot(),
                signal.generation()
            ),
            Self::Execution(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SemanticNativeInputLoweringError {}

/// Resolve semantic native-input signal identity through the same reactive
/// projection used by the execution VM.
///
/// This adapter does not create another graph or signal namespace. It validates
/// the authoritative declaration against `SemanticStore`, then translates each
/// semantic signal to the existing execution `SignalId` already allocated by
/// `SemanticReactiveProjection`.
pub fn lower_semantic_native_inputs(
    store: &SemanticStore,
    inputs: &SemanticNativeInputDefinition,
    projection: &SemanticReactiveProjection,
) -> Result<NativeInputDefinition, SemanticNativeInputLoweringError> {
    inputs
        .validate(store)
        .map_err(SemanticNativeInputLoweringError::Definition)?;

    let mut lowered = NativeInputDefinition::new();
    for binding in inputs.bindings() {
        let signal = projection.execution_signal_id(binding.signal()).ok_or(
            SemanticNativeInputLoweringError::SignalNotLowered(binding.signal()),
        )?;
        match binding {
            SemanticNativeInputBinding::State { source, .. } => {
                lowered.bind_state(source.clone(), signal);
            }
            SemanticNativeInputBinding::Event { source, .. } => {
                lowered.bind_event(source.clone(), signal);
            }
        }
    }
    lowered
        .validate(projection.graph())
        .map_err(SemanticNativeInputLoweringError::Execution)?;
    Ok(lowered)
}

#[cfg(test)]
mod tests {
    use noon_core::{
        NativeEventSource, NativeInputBinding, NativeStateSource, SemanticObjectProperty,
        SemanticObjectState, SemanticVec3, StoredGeometry,
    };

    use super::*;
    use crate::{lower_semantic_execution, SemanticExecutionIndex};

    #[test]
    fn semantic_native_input_identity_lowers_through_existing_reactive_projection() {
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
            .bind_semantic_signal(pointer, object, SemanticObjectProperty::Translation)
            .unwrap();
        store
            .bind_semantic_signal(clicks, object, SemanticObjectProperty::RotationZ)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let execution = lower_semantic_execution(&store, &mut index).unwrap();
        let mut inputs = SemanticNativeInputDefinition::new();
        inputs
            .bind_state(NativeStateSource::PointerPosition, pointer)
            .bind_event(NativeEventSource::PointerDown { button: 0 }, clicks);

        let lowered = lower_semantic_native_inputs(&store, &inputs, execution.reactive()).unwrap();
        assert_eq!(lowered.bindings().len(), 2);
        assert!(matches!(
            &lowered.bindings()[0],
            NativeInputBinding::State {
                source: NativeStateSource::PointerPosition,
                signal,
            } if *signal == execution.reactive().execution_signal_id(pointer).unwrap()
        ));
    }

    #[test]
    fn unreachable_semantic_native_input_fails_closed() {
        let mut store = SemanticStore::new();
        let unused = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let mut index = SemanticExecutionIndex::new();
        let execution = lower_semantic_execution(&store, &mut index).unwrap();
        let mut inputs = SemanticNativeInputDefinition::new();
        inputs.bind_event(NativeEventSource::Wheel, unused);

        assert_eq!(
            lower_semantic_native_inputs(&store, &inputs, execution.reactive()),
            Err(SemanticNativeInputLoweringError::SignalNotLowered(unused))
        );
    }
}
