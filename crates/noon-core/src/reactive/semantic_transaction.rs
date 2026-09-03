use std::collections::HashSet;

use super::{
    SemanticNodeId, SemanticSignalError, SemanticSignalSource, SemanticSignalValue,
    SemanticSignalValueKind, SemanticStore,
};

/// One mutation in the authoritative Semantic Scene transaction vocabulary.
///
/// This first A1.5 slice intentionally starts with input-signal value mutation.
/// Dependency-expression rewiring remains an authored declaration operation rather
/// than being conflated with a frame/value update.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticMutation {
    SetSignal {
        signal: SemanticNodeId,
        value: SemanticSignalValue,
    },
}

impl SemanticMutation {
    pub const fn target(&self) -> SemanticNodeId {
        match self {
            Self::SetSignal { signal, .. } => *signal,
        }
    }
}

/// Locality classification emitted by committed semantic mutations.
///
/// Lowering/runtime consumers can use this without re-interpreting the mutation
/// payload. Future A1.5 mutation kinds extend this enum rather than introducing
/// frontend- or subsystem-specific patch classifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticMutationImpact {
    SignalValue { signal: SemanticNodeId },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticMutationTransaction {
    mutations: Vec<SemanticMutation>,
}

impl SemanticMutationTransaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_signal(
        &mut self,
        signal: SemanticNodeId,
        value: impl Into<SemanticSignalValue>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetSignal {
            signal,
            value: value.into(),
        });
        self
    }

    pub fn mutations(&self) -> &[SemanticMutation] {
        &self.mutations
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    /// Preflight the complete transaction, then commit every changed mutation.
    ///
    /// No semantic slot is written until all mutations have passed generation,
    /// target-kind, input-kind, value-kind, finite-value, and duplicate-target
    /// validation. Once preflight succeeds, commit cannot observe external scene
    /// changes because the transaction owns `&mut SemanticStore` for the duration.
    pub fn apply(
        self,
        store: &mut SemanticStore,
    ) -> Result<SemanticMutationTransactionResult, SemanticMutationTransactionError> {
        store.set_last_mutation_writes(0);
        let preflight = self.preflight(store)?;

        let mut impacts = Vec::with_capacity(self.mutations.len());
        let mut writes = 0;
        for (mutation, changed) in self.mutations.into_iter().zip(preflight) {
            if !changed {
                continue;
            }
            match mutation {
                SemanticMutation::SetSignal { signal, value } => {
                    let changed = store
                        .set_semantic_signal_source(signal, SemanticSignalSource::Input(value))
                        .expect(
                            "preflighted input signal update must remain valid while transaction owns the semantic store",
                        );
                    debug_assert!(changed);
                    writes += 1;
                    impacts.push(SemanticMutationImpact::SignalValue { signal });
                }
            }
        }
        store.set_last_mutation_writes(writes);

        Ok(SemanticMutationTransactionResult { impacts })
    }

    fn preflight(
        &self,
        store: &SemanticStore,
    ) -> Result<Vec<bool>, SemanticMutationTransactionError> {
        let mut targets = HashSet::with_capacity(self.mutations.len());
        let mut changed = Vec::with_capacity(self.mutations.len());

        for (index, mutation) in self.mutations.iter().enumerate() {
            let target = mutation.target();
            if !targets.insert(target) {
                return Err(SemanticMutationTransactionError::DuplicateTarget { index, target });
            }

            match mutation {
                SemanticMutation::SetSignal { signal, value } => {
                    let state = store.semantic_signal_state(*signal).map_err(|error| {
                        SemanticMutationTransactionError::Signal { index, error }
                    })?;
                    let SemanticSignalSource::Input(previous) = state.source() else {
                        return Err(SemanticMutationTransactionError::NotInputSignal {
                            index,
                            signal: *signal,
                        });
                    };
                    if !value.is_finite() {
                        return Err(SemanticMutationTransactionError::Signal {
                            index,
                            error: SemanticSignalError::NonFiniteValue,
                        });
                    }
                    let expected = state.value_kind();
                    let actual = value.value_kind();
                    if actual != expected {
                        return Err(SemanticMutationTransactionError::SignalTypeMismatch {
                            index,
                            signal: *signal,
                            expected,
                            actual,
                        });
                    }
                    changed.push(previous != value);
                }
            }
        }

        Ok(changed)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticMutationTransactionResult {
    impacts: Vec<SemanticMutationImpact>,
}

impl SemanticMutationTransactionResult {
    pub fn impacts(&self) -> &[SemanticMutationImpact] {
        &self.impacts
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticMutationTransactionError {
    DuplicateTarget {
        index: usize,
        target: SemanticNodeId,
    },
    Signal {
        index: usize,
        error: SemanticSignalError,
    },
    NotInputSignal {
        index: usize,
        signal: SemanticNodeId,
    },
    SignalTypeMismatch {
        index: usize,
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
}

impl std::fmt::Display for SemanticMutationTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateTarget { index, target } => write!(
                formatter,
                "semantic transaction mutation {index} repeats target {}:{}",
                target.slot(),
                target.generation()
            ),
            Self::Signal { index, error } => {
                write!(formatter, "semantic transaction mutation {index}: {error}")
            }
            Self::NotInputSignal { index, signal } => write!(
                formatter,
                "semantic transaction mutation {index} cannot SetSignal on derived signal {}:{}",
                signal.slot(),
                signal.generation()
            ),
            Self::SignalTypeMismatch {
                index,
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set signal {}:{} of kind {expected} to {actual}",
                signal.slot(),
                signal.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticMutationTransactionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticObjectState, SemanticSignalExpr, SemanticVec3, StoredGeometry};

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    fn input_value(store: &SemanticStore, signal: SemanticNodeId) -> SemanticSignalValue {
        let SemanticSignalSource::Input(value) = store.semantic_signal_state(signal).unwrap().source()
        else {
            panic!("expected input signal")
        };
        value.clone()
    }

    #[test]
    fn multiple_signal_values_commit_after_complete_preflight() {
        let mut store = SemanticStore::new();
        let scalar = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let vector = store
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        let before_len = store.len();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(scalar, 2.5_f64)
            .set_signal(vector, SemanticVec3::new(4.0, 5.0, 6.0));

        let result = transaction.apply(&mut store).unwrap();

        assert_eq!(input_value(&store, scalar), SemanticSignalValue::Scalar(2.5));
        assert_eq!(
            input_value(&store, vector),
            SemanticSignalValue::Vec3(SemanticVec3::new(4.0, 5.0, 6.0))
        );
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert_eq!(
            result.impacts(),
            &[
                SemanticMutationImpact::SignalValue { signal: scalar },
                SemanticMutationImpact::SignalValue { signal: vector },
            ]
        );
    }

    #[test]
    fn invalid_late_value_prevents_earlier_valid_mutation() {
        let mut store = SemanticStore::new();
        let first = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let second = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let first_before = input_value(&store, first);
        let second_before = input_value(&store, second);
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(first, 10.0_f64)
            .set_signal(second, f64::NAN);

        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::Signal {
                index: 1,
                error: SemanticSignalError::NonFiniteValue,
            })
        );
        assert_eq!(input_value(&store, first), first_before);
        assert_eq!(input_value(&store, second), second_before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn stale_late_target_prevents_earlier_valid_mutation() {
        let mut store = SemanticStore::new();
        let first = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let stale = store.insert_semantic_input_signal(2.0_f64).unwrap();
        store.remove_node(stale).unwrap();
        let replacement = object(&mut store, 3.0);
        assert_eq!(stale.slot(), replacement.slot());
        assert_ne!(stale.generation(), replacement.generation());
        let first_before = input_value(&store, first);
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(first, 10.0_f64)
            .set_signal(stale, 20.0_f64);

        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::Signal {
                index: 1,
                error: SemanticSignalError::UnknownSignal(stale),
            })
        );
        assert_eq!(input_value(&store, first), first_before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn duplicate_target_is_rejected_before_mutation() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let before = input_value(&store, signal);
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(signal, 2.0_f64)
            .set_signal(signal, 3.0_f64);

        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::DuplicateTarget {
                index: 1,
                target: signal,
            })
        );
        assert_eq!(input_value(&store, signal), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn type_mismatch_and_derived_signal_targets_fail_atomically() {
        let mut store = SemanticStore::new();
        let scalar = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(scalar))
            .unwrap();
        let scalar_before = input_value(&store, scalar);

        let mut mismatch = SemanticMutationTransaction::new();
        mismatch.set_signal(scalar, SemanticVec3::new(1.0, 2.0, 3.0));
        assert_eq!(
            mismatch.apply(&mut store),
            Err(SemanticMutationTransactionError::SignalTypeMismatch {
                index: 0,
                signal: scalar,
                expected: SemanticSignalValueKind::Scalar,
                actual: SemanticSignalValueKind::Vec3,
            })
        );
        assert_eq!(input_value(&store, scalar), scalar_before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        let mut derived_target = SemanticMutationTransaction::new();
        derived_target.set_signal(derived, 4.0_f64);
        assert_eq!(
            derived_target.apply(&mut store),
            Err(SemanticMutationTransactionError::NotInputSignal {
                index: 0,
                signal: derived,
            })
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn unchanged_signal_is_a_noop_with_no_impact() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(true).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_signal(signal, true);

        let result = transaction.apply(&mut store).unwrap();

        assert!(result.impacts().is_empty());
        assert_eq!(store.last_mutation_stats().slots_written, 0);
        assert_eq!(input_value(&store, signal), SemanticSignalValue::Bool(true));
    }

    #[test]
    fn transaction_writes_only_changed_signal_slots_with_large_unrelated_scene() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let first = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let second = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let unchanged = store.insert_semantic_input_signal(3.0_f64).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(first, 10.0_f64)
            .set_signal(second, 20.0_f64)
            .set_signal(unchanged, 3.0_f64);

        let result = transaction.apply(&mut store).unwrap();

        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert_eq!(result.impacts().len(), 2);
    }
}
