use serde::{Deserialize, Serialize};

use crate::{NativeEventSource, NativeStateSource, ReactiveValue, ValueKind, Vec2};

/// Normalized value carried by a sampled native input update.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeInputValue {
    Scalar(f32),
    Bool(bool),
    Vec2(Vec2),
}

impl NativeInputValue {
    pub const fn value_kind(self) -> ValueKind {
        match self {
            Self::Scalar(_) => ValueKind::Scalar,
            Self::Bool(_) => ValueKind::Bool,
            Self::Vec2(_) => ValueKind::Vec2,
        }
    }

    pub fn is_finite(self) -> bool {
        match self {
            Self::Scalar(value) => value.is_finite(),
            Self::Bool(_) => true,
            Self::Vec2(value) => value.x.is_finite() && value.y.is_finite(),
        }
    }
}

impl From<NativeInputValue> for ReactiveValue {
    fn from(value: NativeInputValue) -> Self {
        match value {
            NativeInputValue::Scalar(value) => Self::Scalar(value),
            NativeInputValue::Bool(value) => Self::Bool(value),
            NativeInputValue::Vec2(value) => Self::Vec2(value),
        }
    }
}

/// Latest-value state update received from the browser/native host.
///
/// Runtimes may coalesce pending updates with the same `source`, retaining only
/// the newest value before evaluating the native reactive dependency closure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativeStateUpdate {
    pub source: NativeStateSource,
    pub value: NativeInputValue,
}

impl NativeStateUpdate {
    pub fn new(
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<Self, NativeInputRuntimeError> {
        let expected = source.value_kind();
        let actual = value.value_kind();
        if expected != actual {
            return Err(NativeInputRuntimeError::TypeMismatch { expected, actual });
        }
        if !value.is_finite() {
            return Err(NativeInputRuntimeError::NonFiniteValue);
        }
        Ok(Self { source, value })
    }

    /// Source identity is the coalescing key. No scene lookup is required.
    pub fn coalesces_with(&self, newer: &Self) -> bool {
        self.source == newer.source
    }
}

/// Ordered discrete occurrence received from the browser/native host.
///
/// Discrete occurrences are never coalesced. `sequence` is host-ingress order,
/// deliberately independent of authored timeline time so input can wake a paused
/// scene without advancing the timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeEventOccurrence {
    pub sequence: u64,
    pub source: NativeEventSource,
}

impl NativeEventOccurrence {
    pub const fn new(sequence: u64, source: NativeEventSource) -> Self {
        Self { sequence, source }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeInputRuntimeError {
    TypeMismatch {
        expected: ValueKind,
        actual: ValueKind,
    },
    NonFiniteValue,
}

impl std::fmt::Display for NativeInputRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TypeMismatch { expected, actual } => write!(
                formatter,
                "native input value type mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::NonFiniteValue => formatter.write_str("native input value must be finite"),
        }
    }
}

impl std::error::Error for NativeInputRuntimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampled_updates_validate_source_value_kind() {
        let pointer = NativeStateUpdate::new(
            NativeStateSource::PointerPosition,
            NativeInputValue::Vec2(Vec2::new(2.0, -1.0)),
        )
        .unwrap();
        assert_eq!(pointer.value.value_kind(), ValueKind::Vec2);

        assert!(matches!(
            NativeStateUpdate::new(
                NativeStateSource::PointerPosition,
                NativeInputValue::Scalar(1.0),
            ),
            Err(NativeInputRuntimeError::TypeMismatch { .. })
        ));
    }

    #[test]
    fn sampled_updates_fail_closed_on_non_finite_values() {
        assert_eq!(
            NativeStateUpdate::new(
                NativeStateSource::WheelDelta,
                NativeInputValue::Vec2(Vec2::new(f32::NAN, 1.0)),
            ),
            Err(NativeInputRuntimeError::NonFiniteValue)
        );
        assert_eq!(
            NativeStateUpdate::new(
                NativeStateSource::Control {
                    name: "zoom".to_owned(),
                },
                NativeInputValue::Scalar(f32::INFINITY),
            ),
            Err(NativeInputRuntimeError::NonFiniteValue)
        );
    }

    #[test]
    fn sampled_state_coalesces_only_by_exact_source_identity() {
        let old = NativeStateUpdate::new(
            NativeStateSource::Key {
                code: "KeyA".to_owned(),
            },
            NativeInputValue::Bool(false),
        )
        .unwrap();
        let newer = NativeStateUpdate::new(
            NativeStateSource::Key {
                code: "KeyA".to_owned(),
            },
            NativeInputValue::Bool(true),
        )
        .unwrap();
        let other = NativeStateUpdate::new(
            NativeStateSource::Key {
                code: "KeyB".to_owned(),
            },
            NativeInputValue::Bool(true),
        )
        .unwrap();

        assert!(old.coalesces_with(&newer));
        assert!(!old.coalesces_with(&other));
    }

    #[test]
    fn normalized_input_value_uses_existing_reactive_execution_domain() {
        assert_eq!(
            ReactiveValue::from(NativeInputValue::Vec2(Vec2::new(2.0, -1.0))),
            ReactiveValue::Vec2(Vec2::new(2.0, -1.0))
        );
    }

    #[test]
    fn discrete_occurrences_preserve_explicit_ingress_order() {
        let down = NativeEventOccurrence::new(11, NativeEventSource::PointerDown { button: 0 });
        let up = NativeEventOccurrence::new(12, NativeEventSource::PointerUp { button: 0 });

        assert!(down.sequence < up.sequence);
        assert_ne!(down.source, up.source);
    }
}
