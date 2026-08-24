use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{ReactiveGraphDefinition, SignalId, SignalSource, ValueKind};

/// Sampled browser/runtime state that can feed Noon's native reactive graph.
///
/// Pointer positions are expressed in scene/world coordinates. Viewport size and
/// wheel/gesture deltas use canvas CSS-pixel units. Controls are named scalar
/// values supplied by the embedding UI.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeStateSource {
    PointerPosition,
    PointerButton { button: u8 },
    Key { code: String },
    ViewportSize,
    WheelDelta,
    GestureDelta { name: String },
    Control { name: String },
}

impl NativeStateSource {
    pub const fn value_kind(&self) -> ValueKind {
        match self {
            Self::PointerPosition
            | Self::ViewportSize
            | Self::WheelDelta
            | Self::GestureDelta { .. } => ValueKind::Vec2,
            Self::PointerButton { .. } | Self::Key { .. } => ValueKind::Bool,
            Self::Control { .. } => ValueKind::Scalar,
        }
    }
}

/// Discrete native events.
///
/// Each event drives a scalar event-sequence signal. Runtime dispatch increments
/// that signal (with a bounded wrap) for every event, so identical repeated events
/// still invalidate their dependency closure without requiring a host callback.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeEventSource {
    PointerDown { button: u8 },
    PointerUp { button: u8 },
    KeyPress { code: String },
    KeyRelease { code: String },
    Wheel,
    Gesture { name: String },
    ControlCommit { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeInputBinding {
    State {
        source: NativeStateSource,
        signal: SignalId,
    },
    Event {
        source: NativeEventSource,
        signal: SignalId,
    },
}

impl NativeInputBinding {
    pub const fn signal(&self) -> SignalId {
        match self {
            Self::State { signal, .. } | Self::Event { signal, .. } => *signal,
        }
    }
}

/// Declarative mapping from browser/native input sources to reactive input signals.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeInputDefinition {
    bindings: Vec<NativeInputBinding>,
}

impl NativeInputDefinition {
    pub const fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub fn from_parts(
        graph: &ReactiveGraphDefinition,
        bindings: Vec<NativeInputBinding>,
    ) -> Result<Self, NativeInputError> {
        let result = Self { bindings };
        result.validate(graph)?;
        Ok(result)
    }

    pub fn bind_state(&mut self, source: NativeStateSource, signal: SignalId) -> &mut Self {
        self.bindings
            .push(NativeInputBinding::State { source, signal });
        self
    }

    pub fn bind_event(&mut self, source: NativeEventSource, signal: SignalId) -> &mut Self {
        self.bindings
            .push(NativeInputBinding::Event { source, signal });
        self
    }

    pub fn bindings(&self) -> &[NativeInputBinding] {
        &self.bindings
    }

    pub fn drives(&self, signal: SignalId) -> bool {
        self.bindings.iter().any(|binding| binding.signal() == signal)
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn validate(&self, graph: &ReactiveGraphDefinition) -> Result<(), NativeInputError> {
        let mut driven_signals = BTreeSet::new();
        for binding in &self.bindings {
            let signal = binding.signal();
            if !driven_signals.insert(signal) {
                return Err(NativeInputError::DuplicateSignalDriver(signal));
            }
            let definition = graph
                .signals()
                .iter()
                .find(|definition| definition.id == signal)
                .ok_or(NativeInputError::UnknownSignal(signal))?;
            let SignalSource::Input(initial) = &definition.source else {
                return Err(NativeInputError::NotInputSignal(signal));
            };
            let expected = match binding {
                NativeInputBinding::State { source, .. } => {
                    validate_state_name(source)?;
                    source.value_kind()
                }
                NativeInputBinding::Event { source, .. } => {
                    validate_event_name(source)?;
                    ValueKind::Scalar
                }
            };
            let actual = initial.value_kind();
            if actual != expected {
                return Err(NativeInputError::TypeMismatch {
                    signal,
                    expected,
                    actual,
                });
            }
        }
        Ok(())
    }
}

fn validate_state_name(source: &NativeStateSource) -> Result<(), NativeInputError> {
    match source {
        NativeStateSource::Key { code } => validate_name("key code", code),
        NativeStateSource::GestureDelta { name } => validate_name("gesture name", name),
        NativeStateSource::Control { name } => validate_name("control name", name),
        _ => Ok(()),
    }
}

fn validate_event_name(source: &NativeEventSource) -> Result<(), NativeInputError> {
    match source {
        NativeEventSource::KeyPress { code } | NativeEventSource::KeyRelease { code } => {
            validate_name("key code", code)
        }
        NativeEventSource::Gesture { name } => validate_name("gesture name", name),
        NativeEventSource::ControlCommit { name } => validate_name("control name", name),
        _ => Ok(()),
    }
}

fn validate_name(kind: &'static str, value: &str) -> Result<(), NativeInputError> {
    if value.trim().is_empty() {
        Err(NativeInputError::EmptyName(kind))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeInputError {
    UnknownSignal(SignalId),
    NotInputSignal(SignalId),
    DuplicateSignalDriver(SignalId),
    TypeMismatch {
        signal: SignalId,
        expected: ValueKind,
        actual: ValueKind,
    },
    EmptyName(&'static str),
}

impl std::fmt::Display for NativeInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSignal(signal) => {
                write!(formatter, "native input references unknown signal {}", signal.get())
            }
            Self::NotInputSignal(signal) => write!(
                formatter,
                "native input signal {} is derived and cannot be externally driven",
                signal.get()
            ),
            Self::DuplicateSignalDriver(signal) => write!(
                formatter,
                "signal {} has more than one native input driver",
                signal.get()
            ),
            Self::TypeMismatch {
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "native input signal {} type mismatch: expected {expected:?}, got {actual:?}",
                signal.get()
            ),
            Self::EmptyName(kind) => write!(formatter, "native input {kind} must not be empty"),
        }
    }
}

impl std::error::Error for NativeInputError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_state_and_event_bindings_validate_signal_types() {
        let mut graph = ReactiveGraphDefinition::new();
        let pointer = graph.add_input(crate::Vec2::ZERO);
        let pressed = graph.add_input(false);
        let clicks = graph.add_input(0.0_f32);
        let mut inputs = NativeInputDefinition::new();
        inputs
            .bind_state(NativeStateSource::PointerPosition, pointer)
            .bind_state(
                NativeStateSource::Key {
                    code: "Space".to_owned(),
                },
                pressed,
            )
            .bind_event(NativeEventSource::PointerDown { button: 0 }, clicks);
        inputs.validate(&graph).unwrap();
    }

    #[test]
    fn native_input_rejects_wrong_or_duplicate_signal_drivers() {
        let mut graph = ReactiveGraphDefinition::new();
        let scalar = graph.add_input(0.0_f32);
        let mut wrong = NativeInputDefinition::new();
        wrong.bind_state(NativeStateSource::PointerPosition, scalar);
        assert!(matches!(
            wrong.validate(&graph),
            Err(NativeInputError::TypeMismatch { .. })
        ));

        let mut duplicate = NativeInputDefinition::new();
        duplicate
            .bind_state(
                NativeStateSource::Control {
                    name: "x".to_owned(),
                },
                scalar,
            )
            .bind_event(NativeEventSource::Wheel, scalar);
        assert!(matches!(
            duplicate.validate(&graph),
            Err(NativeInputError::DuplicateSignalDriver(_))
        ));
    }

    #[test]
    fn native_input_rejects_derived_signals() {
        let mut graph = ReactiveGraphDefinition::new();
        let input = graph.add_input(1.0_f32);
        let derived = graph.add_derived(crate::ReactiveExpr::signal(input));
        let mut inputs = NativeInputDefinition::new();
        inputs.bind_event(NativeEventSource::Wheel, derived);
        assert!(matches!(
            inputs.validate(&graph),
            Err(NativeInputError::NotInputSignal(signal)) if signal == derived
        ));
    }
}
