use std::collections::HashSet;

use super::{
    SemanticNodeId, SemanticObjectContent, SemanticObjectProperty, SemanticObjectState,
    SemanticSceneOperationError, SemanticSignalBinding, SemanticSignalError, SemanticSignalSource,
    SemanticSignalValue, SemanticSignalValueKind, SemanticStore,
};

/// One mutation in the authoritative Semantic Scene transaction vocabulary.
///
/// Signal values, object properties, authored content, and reactive subscriptions
/// share the same transaction so frontends, editors, and host integrations cannot
/// invent subsystem-specific patch paths. Dependency-expression rewiring remains
/// authored declaration topology rather than being conflated with a value update.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticMutation {
    SetSignal {
        signal: SemanticNodeId,
        value: SemanticSignalValue,
    },
    SetProperty {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
        value: SemanticSignalValue,
    },
    ReplaceContent {
        object: SemanticNodeId,
        content: SemanticObjectContent,
    },
    ChangeSubscription {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
        signal: Option<SemanticNodeId>,
    },
}

impl SemanticMutation {
    pub const fn target(&self) -> SemanticNodeId {
        match self {
            Self::SetSignal { signal, .. } => *signal,
            Self::SetProperty { object, .. }
            | Self::ReplaceContent { object, .. }
            | Self::ChangeSubscription { object, .. } => *object,
        }
    }

    const fn key(&self) -> SemanticMutationKey {
        match self {
            Self::SetSignal { signal, .. } => SemanticMutationKey::Signal(*signal),
            Self::SetProperty {
                object, property, ..
            } => SemanticMutationKey::ObjectProperty {
                object: *object,
                property: *property,
            },
            Self::ReplaceContent { object, .. } => SemanticMutationKey::ObjectContent(*object),
            Self::ChangeSubscription {
                object, property, ..
            } => SemanticMutationKey::Subscription {
                object: *object,
                property: *property,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SemanticMutationKey {
    Signal(SemanticNodeId),
    ObjectProperty {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    ObjectContent(SemanticNodeId),
    Subscription {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
}

/// Locality classification emitted by committed semantic mutations.
///
/// Lowering/runtime consumers can use this without re-interpreting the mutation
/// payload. Future A1.5 mutation kinds extend this enum rather than introducing
/// frontend- or subsystem-specific patch classifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticMutationImpact {
    SignalValue {
        signal: SemanticNodeId,
    },
    ObjectProperty {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    ObjectContent {
        object: SemanticNodeId,
    },
    Subscription {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
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

    /// Set one node-owned authored object property in the same atomic mutation
    /// vocabulary as signal values.
    ///
    /// `SemanticSignalValue` is reused here as the existing high-precision typed
    /// scalar/vector/bool value vocabulary; property-kind validation rejects the
    /// bool case for current object properties instead of creating a duplicate
    /// property-value type.
    pub fn set_property(
        &mut self,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
        value: impl Into<SemanticSignalValue>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetProperty {
            object,
            property,
            value: value.into(),
        });
        self
    }

    /// Replace only the authored content reference/value of one semantic object.
    ///
    /// Transform, style, painter metadata, signal bindings, identity, family
    /// relationships, and scene lifecycle are intentionally left untouched.
    pub fn replace_content(
        &mut self,
        object: SemanticNodeId,
        content: impl Into<SemanticObjectContent>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ReplaceContent {
            object,
            content: content.into(),
        });
        self
    }

    /// Change the authored signal driver for one object property.
    ///
    /// `Some(signal)` binds or rebinds the property and `None` removes its
    /// authored driver. Rebinding is one semantic mutation rather than a visible
    /// remove/add pair, so the existing declaration position is preserved.
    pub fn change_subscription(
        &mut self,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
        signal: Option<SemanticNodeId>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ChangeSubscription {
            object,
            property,
            signal,
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
    /// target-kind, value-kind, finite-value, content, subscription, and
    /// duplicate-mutation validation. Once preflight succeeds, commit cannot
    /// observe external scene changes because the transaction owns
    /// `&mut SemanticStore` for the duration.
    pub fn apply(
        self,
        store: &mut SemanticStore,
    ) -> Result<SemanticMutationTransactionResult, SemanticMutationTransactionError> {
        store.set_last_mutation_writes(0);
        let preflight = self.preflight(store)?;

        let mut impacts = Vec::with_capacity(self.mutations.len());
        let mut written_slots = HashSet::with_capacity(self.mutations.len());
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
                    written_slots.insert(signal);
                    impacts.push(SemanticMutationImpact::SignalValue { signal });
                }
                SemanticMutation::SetProperty {
                    object,
                    property,
                    value,
                } => {
                    set_object_property(store, object, property, value);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectProperty { object, property });
                }
                SemanticMutation::ReplaceContent { object, content } => {
                    set_object_content(store, object, content);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectContent { object });
                }
                SemanticMutation::ChangeSubscription {
                    object,
                    property,
                    signal,
                } => {
                    set_object_subscription(store, object, property, signal);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::Subscription { object, property });
                }
            }
        }
        store.set_last_mutation_writes(written_slots.len());

        Ok(SemanticMutationTransactionResult { impacts })
    }

    fn preflight(
        &self,
        store: &SemanticStore,
    ) -> Result<Vec<bool>, SemanticMutationTransactionError> {
        let mut targets = HashSet::with_capacity(self.mutations.len());
        let mut changed = Vec::with_capacity(self.mutations.len());

        for (index, mutation) in self.mutations.iter().enumerate() {
            let key = mutation.key();
            if !targets.insert(key) {
                return Err(match key {
                    SemanticMutationKey::Signal(target) => {
                        SemanticMutationTransactionError::DuplicateTarget { index, target }
                    }
                    SemanticMutationKey::ObjectProperty { object, property } => {
                        SemanticMutationTransactionError::DuplicateProperty {
                            index,
                            object,
                            property,
                        }
                    }
                    SemanticMutationKey::ObjectContent(object) => {
                        SemanticMutationTransactionError::DuplicateContent { index, object }
                    }
                    SemanticMutationKey::Subscription { object, property } => {
                        SemanticMutationTransactionError::DuplicateSubscription {
                            index,
                            object,
                            property,
                        }
                    }
                });
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
                SemanticMutation::SetProperty {
                    object,
                    property,
                    value,
                } => {
                    let state = store
                        .semantic_object_state_checked(*object)
                        .map_err(|error| SemanticMutationTransactionError::Object {
                            index,
                            error,
                        })?;
                    if !value.is_finite() {
                        return Err(SemanticMutationTransactionError::NonFinitePropertyValue {
                            index,
                            object: *object,
                            property: *property,
                        });
                    }
                    let expected = property.value_kind();
                    let actual = value.value_kind();
                    if actual != expected {
                        return Err(SemanticMutationTransactionError::PropertyTypeMismatch {
                            index,
                            object: *object,
                            property: *property,
                            expected,
                            actual,
                        });
                    }
                    changed.push(object_property_value(state, *property) != *value);
                }
                SemanticMutation::ReplaceContent { object, content } => {
                    let state = store
                        .semantic_object_state_checked(*object)
                        .map_err(|error| SemanticMutationTransactionError::Object {
                            index,
                            error,
                        })?;
                    changed.push(state.content != *content);
                }
                SemanticMutation::ChangeSubscription {
                    object,
                    property,
                    signal,
                } => {
                    let state = store
                        .semantic_object_state_checked(*object)
                        .map_err(|error| SemanticMutationTransactionError::Object {
                            index,
                            error,
                        })?;
                    let existing = state
                        .signal_bindings()
                        .iter()
                        .find(|binding| binding.property() == *property)
                        .map(|binding| binding.signal());

                    if let Some(signal) = signal {
                        let actual =
                            store.semantic_signal_value_kind(*signal).map_err(|error| {
                                SemanticMutationTransactionError::Signal { index, error }
                            })?;
                        let expected = property.value_kind();
                        if actual != expected {
                            return Err(
                                SemanticMutationTransactionError::SubscriptionTypeMismatch {
                                    index,
                                    object: *object,
                                    property: *property,
                                    signal: *signal,
                                    expected,
                                    actual,
                                },
                            );
                        }
                        changed.push(existing != Some(*signal));
                    } else {
                        // Unbinding is intentionally keyed only by object/property.
                        // It must be able to remove a generation-safe stale source
                        // declaration left by the current low-level RemoveNode seam.
                        changed.push(existing.is_some());
                    }
                }
            }
        }

        Ok(changed)
    }
}

fn object_property_value(
    state: &SemanticObjectState,
    property: SemanticObjectProperty,
) -> SemanticSignalValue {
    match property {
        SemanticObjectProperty::Translation => {
            SemanticSignalValue::Vec3(state.transform.translation)
        }
        SemanticObjectProperty::Scale => SemanticSignalValue::Vec3(state.transform.scale),
        SemanticObjectProperty::RotationZ => {
            SemanticSignalValue::Scalar(state.transform.rotation_z)
        }
        SemanticObjectProperty::FillOpacity => {
            SemanticSignalValue::Scalar(state.style.fill_opacity)
        }
        SemanticObjectProperty::StrokeOpacity => {
            SemanticSignalValue::Scalar(state.style.stroke_opacity)
        }
        SemanticObjectProperty::StrokeWidth => {
            SemanticSignalValue::Scalar(state.style.stroke_width)
        }
        SemanticObjectProperty::ObjectOpacity => {
            SemanticSignalValue::Scalar(state.style.object_opacity)
        }
    }
}

fn set_object_property(
    store: &mut SemanticStore,
    object: SemanticNodeId,
    property: SemanticObjectProperty,
    value: SemanticSignalValue,
) {
    let state = store
        .node_mut(object)
        .and_then(|node| node.semantic_object_state_mut())
        .expect("preflighted semantic object must remain valid while transaction owns the store");

    match (property, value) {
        (SemanticObjectProperty::Translation, SemanticSignalValue::Vec3(value)) => {
            state.transform.translation = value;
        }
        (SemanticObjectProperty::Scale, SemanticSignalValue::Vec3(value)) => {
            state.transform.scale = value;
        }
        (SemanticObjectProperty::RotationZ, SemanticSignalValue::Scalar(value)) => {
            state.transform.rotation_z = value;
        }
        (SemanticObjectProperty::FillOpacity, SemanticSignalValue::Scalar(value)) => {
            state.style.fill_opacity = value;
        }
        (SemanticObjectProperty::StrokeOpacity, SemanticSignalValue::Scalar(value)) => {
            state.style.stroke_opacity = value;
        }
        (SemanticObjectProperty::StrokeWidth, SemanticSignalValue::Scalar(value)) => {
            state.style.stroke_width = value;
        }
        (SemanticObjectProperty::ObjectOpacity, SemanticSignalValue::Scalar(value)) => {
            state.style.object_opacity = value;
        }
        _ => {
            unreachable!("semantic property value kind was validated during transaction preflight")
        }
    }
}

fn set_object_content(
    store: &mut SemanticStore,
    object: SemanticNodeId,
    content: SemanticObjectContent,
) {
    store
        .node_mut(object)
        .and_then(|node| node.semantic_object_state_mut())
        .expect("preflighted semantic object must remain valid while transaction owns the store")
        .content = content;
}

fn set_object_subscription(
    store: &mut SemanticStore,
    object: SemanticNodeId,
    property: SemanticObjectProperty,
    signal: Option<SemanticNodeId>,
) {
    let bindings = store
        .node_mut(object)
        .and_then(|node| node.semantic_object_state_mut())
        .expect("preflighted semantic object must remain valid while transaction owns the store")
        .signal_bindings_mut();
    let position = bindings
        .iter()
        .position(|binding| binding.property() == property);

    match (position, signal) {
        (Some(position), Some(signal)) => {
            bindings[position] = SemanticSignalBinding::new(signal, property);
        }
        (None, Some(signal)) => bindings.push(SemanticSignalBinding::new(signal, property)),
        (Some(position), None) => {
            bindings.remove(position);
        }
        (None, None) => {
            unreachable!("unchanged missing subscription is filtered during transaction preflight")
        }
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticMutationTransactionError {
    DuplicateTarget {
        index: usize,
        target: SemanticNodeId,
    },
    DuplicateProperty {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    DuplicateContent {
        index: usize,
        object: SemanticNodeId,
    },
    DuplicateSubscription {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
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
    Object {
        index: usize,
        error: SemanticSceneOperationError,
    },
    NonFinitePropertyValue {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    PropertyTypeMismatch {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    SubscriptionTypeMismatch {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
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
            Self::DuplicateProperty {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats property {:?} on object {}:{}",
                property,
                object.slot(),
                object.generation()
            ),
            Self::DuplicateContent { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} repeats content replacement on object {}:{}",
                object.slot(),
                object.generation()
            ),
            Self::DuplicateSubscription {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats subscription for property {:?} on object {}:{}",
                property,
                object.slot(),
                object.generation()
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
            Self::Object { index, error } => {
                write!(formatter, "semantic transaction mutation {index}: {error}")
            }
            Self::NonFinitePropertyValue {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set property {:?} on object {}:{} to a non-finite value",
                property,
                object.slot(),
                object.generation()
            ),
            Self::PropertyTypeMismatch {
                index,
                object,
                property,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set property {:?} on object {}:{} of kind {expected} to {actual}",
                property,
                object.slot(),
                object.generation()
            ),
            Self::SubscriptionTypeMismatch {
                index,
                object,
                property,
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot bind {actual} signal {}:{} to property {:?} on object {}:{} requiring {expected}",
                signal.slot(),
                signal.generation(),
                property,
                object.slot(),
                object.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticMutationTransactionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SemanticObjectState, SemanticSignalBinding, SemanticSignalExpr, SemanticVec3,
        StoredGeometry,
    };

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    fn input_value(store: &SemanticStore, signal: SemanticNodeId) -> SemanticSignalValue {
        let SemanticSignalSource::Input(value) =
            store.semantic_signal_state(signal).unwrap().source()
        else {
            panic!("expected input signal")
        };
        value.clone()
    }

    fn property_value(
        store: &SemanticStore,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    ) -> SemanticSignalValue {
        object_property_value(
            store.semantic_object_state_checked(object).unwrap(),
            property,
        )
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

        assert_eq!(
            input_value(&store, scalar),
            SemanticSignalValue::Scalar(2.5)
        );
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
    fn mixed_signal_and_properties_commit_atomically_and_count_unique_slots() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let target = object(&mut store, 2.0);
        let translation = SemanticVec3::new(4.0, -2.0, 7.0);
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(signal, 3.0_f64)
            .set_property(target, SemanticObjectProperty::Translation, translation)
            .set_property(target, SemanticObjectProperty::ObjectOpacity, 0.4_f64);

        let result = transaction.apply(&mut store).unwrap();

        assert_eq!(
            input_value(&store, signal),
            SemanticSignalValue::Scalar(3.0)
        );
        assert_eq!(
            property_value(&store, target, SemanticObjectProperty::Translation),
            SemanticSignalValue::Vec3(translation)
        );
        assert_eq!(
            property_value(&store, target, SemanticObjectProperty::ObjectOpacity),
            SemanticSignalValue::Scalar(0.4)
        );
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert_eq!(
            result.impacts(),
            &[
                SemanticMutationImpact::SignalValue { signal },
                SemanticMutationImpact::ObjectProperty {
                    object: target,
                    property: SemanticObjectProperty::Translation,
                },
                SemanticMutationImpact::ObjectProperty {
                    object: target,
                    property: SemanticObjectProperty::ObjectOpacity,
                },
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
    fn invalid_late_property_prevents_earlier_signal_and_property_mutation() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let target = object(&mut store, 2.0);
        let signal_before = input_value(&store, signal);
        let translation_before =
            property_value(&store, target, SemanticObjectProperty::Translation);
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(signal, 5.0_f64)
            .set_property(
                target,
                SemanticObjectProperty::Translation,
                SemanticVec3::new(1.0, 2.0, 3.0),
            )
            .set_property(target, SemanticObjectProperty::StrokeWidth, f64::NAN);

        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::NonFinitePropertyValue {
                index: 2,
                object: target,
                property: SemanticObjectProperty::StrokeWidth,
            })
        );
        assert_eq!(input_value(&store, signal), signal_before);
        assert_eq!(
            property_value(&store, target, SemanticObjectProperty::Translation),
            translation_before
        );
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
    fn stale_property_target_prevents_earlier_valid_mutation() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let stale = object(&mut store, 2.0);
        store.remove_node(stale).unwrap();
        let replacement = object(&mut store, 3.0);
        assert_eq!(stale.slot(), replacement.slot());
        assert_ne!(stale.generation(), replacement.generation());
        let signal_before = input_value(&store, signal);
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_signal(signal, 2.0_f64).set_property(
            stale,
            SemanticObjectProperty::RotationZ,
            0.5_f64,
        );

        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::Object {
                index: 1,
                error: SemanticSceneOperationError::UnknownNode(stale),
            })
        );
        assert_eq!(input_value(&store, signal), signal_before);
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
    fn duplicate_property_is_rejected_but_distinct_properties_share_one_object() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let mut duplicate = SemanticMutationTransaction::new();
        duplicate
            .set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64)
            .set_property(target, SemanticObjectProperty::RotationZ, 1.0_f64);

        assert_eq!(
            duplicate.apply(&mut store),
            Err(SemanticMutationTransactionError::DuplicateProperty {
                index: 1,
                object: target,
                property: SemanticObjectProperty::RotationZ,
            })
        );
        assert_eq!(
            property_value(&store, target, SemanticObjectProperty::RotationZ),
            SemanticSignalValue::Scalar(0.0)
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        let mut distinct = SemanticMutationTransaction::new();
        distinct
            .set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64)
            .set_property(target, SemanticObjectProperty::StrokeWidth, 3.0_f64);
        let result = distinct.apply(&mut store).unwrap();
        assert_eq!(result.impacts().len(), 2);
        assert_eq!(store.last_mutation_stats().slots_written, 1);
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
    fn property_type_and_target_kind_are_validated_before_mutation() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let family = store.insert_family();
        let mut mismatch = SemanticMutationTransaction::new();
        mismatch.set_property(target, SemanticObjectProperty::Scale, 2.0_f64);

        assert_eq!(
            mismatch.apply(&mut store),
            Err(SemanticMutationTransactionError::PropertyTypeMismatch {
                index: 0,
                object: target,
                property: SemanticObjectProperty::Scale,
                expected: SemanticSignalValueKind::Vec3,
                actual: SemanticSignalValueKind::Scalar,
            })
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        let mut wrong_target = SemanticMutationTransaction::new();
        wrong_target.set_property(family, SemanticObjectProperty::RotationZ, 1.0_f64);
        assert_eq!(
            wrong_target.apply(&mut store),
            Err(SemanticMutationTransactionError::Object {
                index: 0,
                error: SemanticSceneOperationError::NotSemanticObject(family),
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
    fn unchanged_property_is_a_noop_with_no_impact() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(
            target,
            SemanticObjectProperty::Translation,
            SemanticVec3::ZERO,
        );

        let result = transaction.apply(&mut store).unwrap();

        assert!(result.impacts().is_empty());
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn set_property_preserves_signal_binding_declarations() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
        let target = object(&mut store, 1.0);
        store
            .bind_semantic_signal(signal, target, SemanticObjectProperty::ObjectOpacity)
            .unwrap();
        let binding = SemanticSignalBinding::new(signal, SemanticObjectProperty::ObjectOpacity);
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(target, SemanticObjectProperty::ObjectOpacity, 0.25_f64);

        let result = transaction.apply(&mut store).unwrap();

        assert_eq!(
            property_value(&store, target, SemanticObjectProperty::ObjectOpacity),
            SemanticSignalValue::Scalar(0.25)
        );
        assert_eq!(
            store.semantic_object_signal_bindings(target).unwrap(),
            &[binding]
        );
        assert_eq!(
            result.impacts(),
            &[SemanticMutationImpact::ObjectProperty {
                object: target,
                property: SemanticObjectProperty::ObjectOpacity,
            }]
        );
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

    #[test]
    fn property_transaction_writes_only_target_slot_with_large_unrelated_scene() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let target = object(&mut store, 0.5);
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_property(target, SemanticObjectProperty::RotationZ, 0.75_f64)
            .set_property(target, SemanticObjectProperty::StrokeWidth, 4.0_f64);

        let result = transaction.apply(&mut store).unwrap();

        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(result.impacts().len(), 2);
    }
}

#[cfg(test)]
mod content_tests;

#[cfg(test)]
mod subscription_tests;
