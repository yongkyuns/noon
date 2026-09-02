use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{NativeEventSource, NativeStateSource, ValueKind, Vec2};

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

/// Accounting for one pending native-input batch.
///
/// `state_received` counts every sampled update accepted by the batch while
/// `state_coalesced` counts samples superseded by a newer value for the same
/// exact source. Discrete events are never coalesced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeInputBatchStats {
    pub state_received: u64,
    pub state_coalesced: u64,
    pub events_received: u64,
}

/// Pending normalized native input awaiting one runtime dispatch boundary.
///
/// Sampled state is stored once per exact source so high-frequency pointer,
/// viewport, wheel, gesture, and control updates have memory proportional to the
/// number of active sources rather than the number of browser samples. Discrete
/// events remain an insertion-ordered `Vec` because press/release/commit
/// occurrences must not disappear. This type deliberately does not define when
/// state versus events are applied relative to timeline/reactive/commit phases;
/// the runtime dispatcher owns that ordering contract.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeInputBatch {
    states: BTreeMap<NativeStateSource, NativeStateUpdate>,
    events: Vec<NativeEventOccurrence>,
    stats: NativeInputBatchStats,
}

impl NativeInputBatch {
    pub const fn new() -> Self {
        Self {
            states: BTreeMap::new(),
            events: Vec::new(),
            stats: NativeInputBatchStats {
                state_received: 0,
                state_coalesced: 0,
                events_received: 0,
            },
        }
    }

    /// Retains the newest sampled value for this exact source.
    ///
    /// Returns `true` when an older pending sample was coalesced away.
    pub fn push_state(&mut self, update: NativeStateUpdate) -> bool {
        self.stats.state_received = self.stats.state_received.saturating_add(1);
        let replaced = self.states.insert(update.source.clone(), update).is_some();
        if replaced {
            self.stats.state_coalesced = self.stats.state_coalesced.saturating_add(1);
        }
        replaced
    }

    /// Appends a discrete occurrence without coalescing or reordering it.
    pub fn push_event(&mut self, occurrence: NativeEventOccurrence) {
        self.stats.events_received = self.stats.events_received.saturating_add(1);
        self.events.push(occurrence);
    }

    pub fn states(&self) -> impl ExactSizeIterator<Item = &NativeStateUpdate> {
        self.states.values()
    }

    pub fn events(&self) -> &[NativeEventOccurrence] {
        &self.events
    }

    pub const fn stats(&self) -> NativeInputBatchStats {
        self.stats
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty() && self.events.is_empty()
    }

    pub fn clear(&mut self) {
        self.states.clear();
        self.events.clear();
        self.stats = NativeInputBatchStats::default();
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
    fn discrete_occurrences_preserve_explicit_ingress_order() {
        let down = NativeEventOccurrence::new(11, NativeEventSource::PointerDown { button: 0 });
        let up = NativeEventOccurrence::new(12, NativeEventSource::PointerUp { button: 0 });

        assert!(down.sequence < up.sequence);
        assert_ne!(down.source, up.source);
    }

    #[test]
    fn burst_batch_coalesces_sampled_state_to_latest_exact_source() {
        let mut batch = NativeInputBatch::new();
        for sample in 0..10_000 {
            let update = NativeStateUpdate::new(
                NativeStateSource::PointerPosition,
                NativeInputValue::Vec2(Vec2::new(sample as f32, -(sample as f32))),
            )
            .unwrap();
            batch.push_state(update);
        }
        batch.push_state(
            NativeStateUpdate::new(
                NativeStateSource::Key {
                    code: "Space".to_owned(),
                },
                NativeInputValue::Bool(true),
            )
            .unwrap(),
        );

        let states: Vec<_> = batch.states().collect();
        assert_eq!(states.len(), 2);
        let pointer = states
            .iter()
            .find(|update| update.source == NativeStateSource::PointerPosition)
            .unwrap();
        assert_eq!(
            pointer.value,
            NativeInputValue::Vec2(Vec2::new(9_999.0, -9_999.0))
        );
        assert_eq!(
            batch.stats(),
            NativeInputBatchStats {
                state_received: 10_001,
                state_coalesced: 9_999,
                events_received: 0,
            }
        );
    }

    #[test]
    fn burst_batch_never_coalesces_or_reorders_discrete_events() {
        let mut batch = NativeInputBatch::new();
        for sequence in 0..4_096 {
            let source = if sequence % 2 == 0 {
                NativeEventSource::PointerDown { button: 0 }
            } else {
                NativeEventSource::PointerUp { button: 0 }
            };
            batch.push_event(NativeEventOccurrence::new(sequence, source));
        }

        assert_eq!(batch.events().len(), 4_096);
        assert!(batch
            .events()
            .iter()
            .enumerate()
            .all(|(index, event)| event.sequence == index as u64));
        assert_eq!(batch.stats().events_received, 4_096);
    }

    #[test]
    fn clearing_batch_resets_pending_input_and_accounting() {
        let mut batch = NativeInputBatch::new();
        batch.push_event(NativeEventOccurrence::new(
            3,
            NativeEventSource::ControlCommit {
                name: "gain".to_owned(),
            },
        ));
        batch.push_state(
            NativeStateUpdate::new(
                NativeStateSource::Control {
                    name: "gain".to_owned(),
                },
                NativeInputValue::Scalar(0.5),
            )
            .unwrap(),
        );

        batch.clear();
        assert!(batch.is_empty());
        assert_eq!(batch.stats(), NativeInputBatchStats::default());
    }
}
