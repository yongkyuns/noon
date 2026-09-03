use std::collections::HashSet;

use super::semantic_store::SemanticRemoveNodeEffect;
use super::{
    SemanticNodeId, SemanticNodeKind, SemanticObjectContent, SemanticObjectProperty,
    SemanticObjectState, SemanticSceneOperationError, SemanticSignalBinding, SemanticSignalError,
    SemanticSignalSource, SemanticSignalValue, SemanticSignalValueKind, SemanticStore,
    SemanticStoreError,
};

mod family_edges;
use family_edges::FamilyEdgePreflight;

/// One mutation in the authoritative Semantic Scene transaction vocabulary.
///
/// Signal values, object properties, authored content, family membership,
/// reactive subscriptions, and structural deletion share the same transaction so
/// frontends, editors, and host integrations cannot invent subsystem-specific
/// patch paths. Dependency-expression rewiring remains authored declaration
/// topology rather than being conflated with a value update.
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
    AddMember {
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    RemoveMember {
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    RemoveNode {
        node: SemanticNodeId,
    },
}

impl SemanticMutation {
    pub const fn target(&self) -> SemanticNodeId {
        match self {
            Self::SetSignal { signal, .. } => *signal,
            Self::SetProperty { object, .. }
            | Self::ReplaceContent { object, .. }
            | Self::ChangeSubscription { object, .. } => *object,
            Self::AddMember { family, .. } | Self::RemoveMember { family, .. } => *family,
            Self::RemoveNode { node } => *node,
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
            Self::AddMember { family, member } | Self::RemoveMember { family, member } => {
                SemanticMutationKey::FamilyEdge {
                    family: *family,
                    member: *member,
                }
            }
            Self::RemoveNode { node } => SemanticMutationKey::NodeRemoval(*node),
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
    FamilyEdge {
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    NodeRemoval(SemanticNodeId),
}

/// Locality classification emitted by committed semantic mutations.
///
/// Lowering/runtime consumers can use this without re-interpreting the mutation
/// payload. Structural cleanup emits impacts for every semantic declaration it
/// actually invalidates; no frontend- or subsystem-specific patch classification
/// is introduced.
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
    FamilyMemberAdded {
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    FamilyMemberRemoved {
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    NodeRemoved {
        node: SemanticNodeId,
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

    /// Add one direct ordered family edge through the authoritative transaction.
    pub fn add_member(&mut self, family: SemanticNodeId, member: SemanticNodeId) -> &mut Self {
        self.mutations
            .push(SemanticMutation::AddMember { family, member });
        self
    }

    /// Remove one direct ordered family edge through the authoritative transaction.
    pub fn remove_member(&mut self, family: SemanticNodeId, member: SemanticNodeId) -> &mut Self {
        self.mutations
            .push(SemanticMutation::RemoveMember { family, member });
        self
    }

    /// Delete one semantic identity and atomically clean declarations that cannot
    /// remain valid without it.
    ///
    /// Signal bindings are unbound. Derived signals and animation declarations
    /// that reference the removed identity are themselves removed, recursively.
    /// Structural removals are terminal within a transaction: once the first
    /// `RemoveNode` is authored, no later non-removal mutation is accepted. This
    /// keeps complete preflight valid through commit without staging a second scene.
    pub fn remove_node(&mut self, node: SemanticNodeId) -> &mut Self {
        self.mutations.push(SemanticMutation::RemoveNode { node });
        self
    }

    pub fn mutations(&self) -> &[SemanticMutation] {
        &self.mutations
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    /// Preflight the complete transaction, then commit every changed mutation.
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
                SemanticMutation::AddMember { family, member } => {
                    store.add_member(family, member).expect(
                        "preflighted family add must remain valid while transaction owns the semantic store",
                    );
                    written_slots.insert(family);
                    written_slots.insert(member);
                    impacts.push(SemanticMutationImpact::FamilyMemberAdded { family, member });
                }
                SemanticMutation::RemoveMember { family, member } => {
                    let removed = store.remove_member(family, member).expect(
                        "preflighted family removal must remain valid while transaction owns the semantic store",
                    );
                    debug_assert!(removed);
                    written_slots.insert(family);
                    written_slots.insert(member);
                    impacts.push(SemanticMutationImpact::FamilyMemberRemoved { family, member });
                }
                SemanticMutation::RemoveNode { node } => {
                    // An earlier explicit removal may have cascade-removed this
                    // node already. Preflight proved the handle was live at the
                    // transaction boundary; a cascade therefore satisfies this
                    // later structural mutation without duplicate impacts.
                    if store.node(node).is_none() {
                        continue;
                    }
                    let outcome = store
                        .remove_node_with_reverse_cleanup(node)
                        .expect("preflighted node removal must remain valid while transaction owns the store");
                    written_slots.extend(outcome.written_slots().iter().copied());
                    for effect in outcome.effects() {
                        match effect {
                            SemanticRemoveNodeEffect::NodeRemoved(node) => {
                                impacts.push(SemanticMutationImpact::NodeRemoved { node: *node });
                            }
                            SemanticRemoveNodeEffect::SubscriptionRemoved { object, property } => {
                                impacts.push(SemanticMutationImpact::Subscription {
                                    object: *object,
                                    property: *property,
                                });
                            }
                        }
                    }
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
        let mut removed_nodes = HashSet::new();
        let mut removal_started = false;
        for (index, mutation) in self.mutations.iter().enumerate() {
            match mutation {
                SemanticMutation::RemoveNode { node } => {
                    removal_started = true;
                    removed_nodes.insert(*node);
                }
                _ if removal_started => {
                    return Err(SemanticMutationTransactionError::MutationAfterRemove { index });
                }
                _ => {}
            }
        }
        let removed_nodes = store.semantic_removal_closure(&removed_nodes);

        let mut targets = HashSet::with_capacity(self.mutations.len());
        let mut changed = Vec::with_capacity(self.mutations.len());
        let mut family_edges = FamilyEdgePreflight::default();

        for (index, mutation) in self.mutations.iter().enumerate() {
            if !matches!(mutation, SemanticMutation::RemoveNode { .. })
                && removed_nodes.contains(&mutation.target())
            {
                return Err(SemanticMutationTransactionError::TargetRemoved {
                    index,
                    target: mutation.target(),
                });
            }
            if let SemanticMutation::ChangeSubscription {
                object,
                property,
                signal: Some(signal),
            } = mutation
            {
                if removed_nodes.contains(signal) {
                    return Err(
                        SemanticMutationTransactionError::SubscriptionUsesRemovedSignal {
                            index,
                            object: *object,
                            property: *property,
                            signal: *signal,
                        },
                    );
                }
            }
            if let SemanticMutation::AddMember { family, member }
            | SemanticMutation::RemoveMember { family, member } = mutation
            {
                if removed_nodes.contains(member) {
                    return Err(
                        SemanticMutationTransactionError::FamilyEdgeUsesRemovedNode {
                            index,
                            family: *family,
                            member: *member,
                        },
                    );
                }
            }

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
                    SemanticMutationKey::FamilyEdge { family, member } => {
                        SemanticMutationTransactionError::DuplicateFamilyEdge {
                            index,
                            family,
                            member,
                        }
                    }
                    SemanticMutationKey::NodeRemoval(node) => {
                        SemanticMutationTransactionError::DuplicateNodeRemoval { index, node }
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
                        changed.push(existing.is_some());
                    }
                }
                SemanticMutation::AddMember { family, member } => {
                    changed.push(family_edges.add(store, *family, *member).map_err(|error| {
                        SemanticMutationTransactionError::Family { index, error }
                    })?);
                }
                SemanticMutation::RemoveMember { family, member } => {
                    changed.push(family_edges.remove(store, *family, *member).map_err(
                        |error| SemanticMutationTransactionError::Family { index, error },
                    )?);
                }
                SemanticMutation::RemoveNode { node } => {
                    if store.node(*node).is_none() {
                        return Err(SemanticMutationTransactionError::Node {
                            index,
                            error: SemanticStoreError::UnknownNode(*node),
                        });
                    }
                    changed.push(true);
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
    store.unregister_semantic_references_for_owner(object);
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
    store.register_semantic_references_for_owner(object);
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
    DuplicateFamilyEdge {
        index: usize,
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    DuplicateNodeRemoval {
        index: usize,
        node: SemanticNodeId,
    },
    MutationAfterRemove {
        index: usize,
    },
    TargetRemoved {
        index: usize,
        target: SemanticNodeId,
    },
    SubscriptionUsesRemovedSignal {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
        signal: SemanticNodeId,
    },
    FamilyEdgeUsesRemovedNode {
        index: usize,
        family: SemanticNodeId,
        member: SemanticNodeId,
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
    Family {
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
    Node {
        index: usize,
        error: SemanticStoreError,
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
            Self::DuplicateFamilyEdge {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats family edge {}:{} -> {}:{}",
                family.slot(),
                family.generation(),
                member.slot(),
                member.generation()
            ),
            Self::DuplicateNodeRemoval { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} repeats removal of node {}:{}",
                node.slot(),
                node.generation()
            ),
            Self::MutationAfterRemove { index } => write!(
                formatter,
                "semantic transaction mutation {index} follows structural removal; RemoveNode mutations must be terminal"
            ),
            Self::TargetRemoved { index, target } => write!(
                formatter,
                "semantic transaction mutation {index} targets node {}:{} that is removed by the same transaction",
                target.slot(),
                target.generation()
            ),
            Self::SubscriptionUsesRemovedSignal {
                index,
                object,
                property,
                signal,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot bind signal {}:{} scheduled for removal to property {:?} on object {}:{}",
                signal.slot(),
                signal.generation(),
                property,
                object.slot(),
                object.generation()
            ),
            Self::FamilyEdgeUsesRemovedNode {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot change family edge {}:{} -> {}:{} because the member is removed by the same transaction",
                family.slot(),
                family.generation(),
                member.slot(),
                member.generation()
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
            Self::Object { index, error } | Self::Family { index, error } => {
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
            Self::Node { index, error } => {
                write!(formatter, "semantic transaction mutation {index}: {error}")
            }
        }
    }
}

impl std::error::Error for SemanticMutationTransactionError {}

#[cfg(test)]
mod base_tests;

#[cfg(test)]
mod content_tests;

#[cfg(test)]
mod subscription_tests;

#[cfg(test)]
mod family_edge_tests;

#[cfg(test)]
mod remove_node_tests;
