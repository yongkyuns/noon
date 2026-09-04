use std::collections::BTreeSet;

use crate::{
    NativeEventSource, NativeStateSource, SemanticNodeId, SemanticSignalError,
    SemanticSignalSource, SemanticSignalValueKind, SemanticStore,
};

/// Authoritative native-input binding keyed by semantic signal identity.
///
/// Platform source vocabulary is normalized and backend-neutral. Execution
/// `SignalId` values deliberately do not appear in this authored declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticNativeInputBinding {
    State {
        source: NativeStateSource,
        signal: SemanticNodeId,
    },
    Event {
        source: NativeEventSource,
        signal: SemanticNodeId,
    },
}

impl SemanticNativeInputBinding {
    pub const fn signal(&self) -> SemanticNodeId {
        match self {
            Self::State { signal, .. } | Self::Event { signal, .. } => *signal,
        }
    }
}

/// Backend-neutral native-input declarations for the authoritative semantic scene.
///
/// The declaration remains separate from platform collection. Native and browser
/// hosts translate their platform events into [`NativeStateSource`] / [`NativeEventSource`]
/// and the execution session resolves semantic signal identity through canonical
/// lowering before dispatch reaches the existing native-reactive VM.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticNativeInputDefinition {
    bindings: Vec<SemanticNativeInputBinding>,
}

impl SemanticNativeInputDefinition {
    pub const fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub fn bind_state(&mut self, source: NativeStateSource, signal: SemanticNodeId) -> &mut Self {
        self.bindings
            .push(SemanticNativeInputBinding::State { source, signal });
        self
    }

    pub fn bind_event(&mut self, source: NativeEventSource, signal: SemanticNodeId) -> &mut Self {
        self.bindings
            .push(SemanticNativeInputBinding::Event { source, signal });
        self
    }

    pub fn bindings(&self) -> &[SemanticNativeInputBinding] {
        &self.bindings
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn validate(&self, store: &SemanticStore) -> Result<(), SemanticNativeInputError> {
        let mut driven_signals = BTreeSet::new();
        for binding in &self.bindings {
            let signal = binding.signal();
            if !driven_signals.insert(signal) {
                return Err(SemanticNativeInputError::DuplicateSignalDriver(signal));
            }
            let state = store
                .semantic_signal_state(signal)
                .map_err(SemanticNativeInputError::Signal)?;
            if !matches!(state.source(), SemanticSignalSource::Input(_)) {
                return Err(SemanticNativeInputError::NotInputSignal(signal));
            }

            let expected = match binding {
                SemanticNativeInputBinding::State { source, .. } => {
                    validate_state_source(source)?;
                    semantic_kind_for_state(source)
                }
                SemanticNativeInputBinding::Event { source, .. } => {
                    validate_event_source(source)?;
                    SemanticSignalValueKind::Scalar
                }
            };
            let actual = state.value_kind();
            if actual != expected {
                return Err(SemanticNativeInputError::TypeMismatch {
                    signal,
                    expected,
                    actual,
                });
            }
        }
        Ok(())
    }
}

fn semantic_kind_for_state(source: &NativeStateSource) -> SemanticSignalValueKind {
    match source {
        NativeStateSource::PointerPosition
        | NativeStateSource::ViewportSize
        | NativeStateSource::WheelDelta
        | NativeStateSource::GestureDelta { .. } => SemanticSignalValueKind::Vec3,
        NativeStateSource::PointerButton { .. } | NativeStateSource::Key { .. } => {
            SemanticSignalValueKind::Bool
        }
        NativeStateSource::Control { .. } => SemanticSignalValueKind::Scalar,
    }
}

fn validate_state_source(source: &NativeStateSource) -> Result<(), SemanticNativeInputError> {
    match source {
        NativeStateSource::Key { code } => validate_name("key code", code),
        NativeStateSource::GestureDelta { name } => validate_name("gesture name", name),
        NativeStateSource::Control { name } => validate_name("control name", name),
        _ => Ok(()),
    }
}

fn validate_event_source(source: &NativeEventSource) -> Result<(), SemanticNativeInputError> {
    match source {
        NativeEventSource::KeyPress { code } | NativeEventSource::KeyRelease { code } => {
            validate_name("key code", code)
        }
        NativeEventSource::Gesture { name } => validate_name("gesture name", name),
        NativeEventSource::ControlCommit { name } => validate_name("control name", name),
        _ => Ok(()),
    }
}

fn validate_name(kind: &'static str, value: &str) -> Result<(), SemanticNativeInputError> {
    if value.trim().is_empty() {
        Err(SemanticNativeInputError::EmptyName(kind))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticNativeInputError {
    Signal(SemanticSignalError),
    NotInputSignal(SemanticNodeId),
    DuplicateSignalDriver(SemanticNodeId),
    TypeMismatch {
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    EmptyName(&'static str),
}

impl std::fmt::Display for SemanticNativeInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signal(error) => error.fmt(formatter),
            Self::NotInputSignal(signal) => write!(
                formatter,
                "semantic native input signal {}:{} is derived and cannot be externally driven",
                signal.slot(),
                signal.generation()
            ),
            Self::DuplicateSignalDriver(signal) => write!(
                formatter,
                "semantic signal {}:{} has more than one native input driver",
                signal.slot(),
                signal.generation()
            ),
            Self::TypeMismatch {
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic native input signal {}:{} type mismatch: expected {expected}, got {actual}",
                signal.slot(),
                signal.generation()
            ),
            Self::EmptyName(kind) => write!(formatter, "semantic native input {kind} must not be empty"),
        }
    }
}

impl std::error::Error for SemanticNativeInputError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticSignalExpr, SemanticVec3};

    #[test]
    fn semantic_native_inputs_validate_authoritative_signal_identity_and_kind() {
        let mut store = SemanticStore::new();
        let pointer = store
            .insert_semantic_input_signal(SemanticVec3::new(0.0, 0.0, 0.0))
            .unwrap();
        let pressed = store.insert_semantic_input_signal(false).unwrap();
        let clicks = store.insert_semantic_input_signal(0.0_f64).unwrap();

        let mut inputs = SemanticNativeInputDefinition::new();
        inputs
            .bind_state(NativeStateSource::PointerPosition, pointer)
            .bind_state(
                NativeStateSource::Key {
                    code: "Space".to_owned(),
                },
                pressed,
            )
            .bind_event(NativeEventSource::PointerDown { button: 0 }, clicks);
        inputs.validate(&store).unwrap();
    }

    #[test]
    fn semantic_native_inputs_reject_derived_wrong_kind_and_duplicate_drivers() {
        let mut store = SemanticStore::new();
        let scalar = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(scalar))
            .unwrap();

        let mut derived_input = SemanticNativeInputDefinition::new();
        derived_input.bind_event(NativeEventSource::Wheel, derived);
        assert!(matches!(
            derived_input.validate(&store),
            Err(SemanticNativeInputError::NotInputSignal(signal)) if signal == derived
        ));

        let mut wrong_kind = SemanticNativeInputDefinition::new();
        wrong_kind.bind_state(NativeStateSource::PointerPosition, scalar);
        assert!(matches!(
            wrong_kind.validate(&store),
            Err(SemanticNativeInputError::TypeMismatch { .. })
        ));

        let mut duplicate = SemanticNativeInputDefinition::new();
        duplicate
            .bind_state(
                NativeStateSource::Control {
                    name: "gain".to_owned(),
                },
                scalar,
            )
            .bind_event(NativeEventSource::Wheel, scalar);
        assert!(matches!(
            duplicate.validate(&store),
            Err(SemanticNativeInputError::DuplicateSignalDriver(signal)) if signal == scalar
        ));
    }
}
