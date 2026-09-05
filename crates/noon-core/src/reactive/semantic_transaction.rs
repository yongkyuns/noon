use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use super::semantic_store::SemanticRemoveNodeEffect;
use super::{
    SemanticAnimationState, SemanticNodeId, SemanticNodeKind, SemanticObjectContent,
    SemanticObjectProperty, SemanticObjectState, SemanticSceneOperationError,
    SemanticSignalBinding, SemanticSignalError, SemanticSignalSource, SemanticSignalValue,
    SemanticSignalValueKind, SemanticStore, SemanticStoreError,
};

mod animation_addition;
use animation_addition::{commit_add_animation, preflight_add_animation};

mod family_edges;
use family_edges::FamilyEdgePreflight;

mod node_addition;
pub use node_addition::SemanticNodeCreation;
use node_addition::{commit_add_node, preflight_add_node};

static NEXT_SEMANTIC_TRANSACTION_SCOPE: AtomicU64 = AtomicU64::new(1);

fn next_semantic_transaction_scope() -> u64 {
    NEXT_SEMANTIC_TRANSACTION_SCOPE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |scope| {
            scope.checked_add(1)
        })
        .expect("Noon semantic transaction scope space exhausted")
}

/// Opaque transaction-local reference to a node that has not received permanent
/// semantic identity yet.
///
/// Tokens are scoped to the transaction that produced them and are never valid
/// `SemanticNodeId`s. They resolve to permanent generational identity only after a
/// successful commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticTransactionNodeToken {
    scope: u64,
    mutation_index: u32,
}

impl SemanticTransactionNodeToken {
    pub const fn local_index(self) -> u32 {
        self.mutation_index
    }
}

/// Reference accepted by transaction-local structural operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticTransactionNodeRef {
    Existing(SemanticNodeId),
    Pending(SemanticTransactionNodeToken),
}

impl SemanticTransactionNodeRef {
    pub const fn existing(self) -> Option<SemanticNodeId> {
        match self {
            Self::Existing(node) => Some(node),
            Self::Pending(_) => None,
        }
    }
}

impl From<SemanticNodeId> for SemanticTransactionNodeRef {
    fn from(value: SemanticNodeId) -> Self {
        Self::Existing(value)
    }
}

impl From<SemanticTransactionNodeToken> for SemanticTransactionNodeRef {
    fn from(value: SemanticTransactionNodeToken) -> Self {
        Self::Pending(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PendingNodeKind {
    Object,
    Family,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticPendingNodeError {
    UnknownToken,
    NotObject,
    NonFiniteProperty(SemanticObjectProperty),
    PropertyTypeMismatch {
        property: SemanticObjectProperty,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    SubscriptionTypeMismatch {
        property: SemanticObjectProperty,
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticPendingFamilyError {
    UnknownToken(SemanticTransactionNodeToken),
    NotFamily(SemanticTransactionNodeToken),
    Cycle {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    NotFamilyMember {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
}

/// One mutation in the authoritative Semantic Scene transaction vocabulary.
///
/// Signal values, object properties, authored content, scene-node allocation,
/// family membership/order, reactive subscriptions, animation declarations, and
/// structural deletion share the same transaction so frontends, editors, and host
/// integrations cannot invent subsystem-specific patch paths. Dependency-expression
/// rewiring remains authored declaration topology rather than being conflated with a
/// value update.
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
    SetPendingProperty {
        object: SemanticTransactionNodeToken,
        property: SemanticObjectProperty,
        value: SemanticSignalValue,
    },
    ReplaceContent {
        object: SemanticNodeId,
        content: SemanticObjectContent,
    },
    ReplacePendingContent {
        object: SemanticTransactionNodeToken,
        content: SemanticObjectContent,
    },
    ChangeSubscription {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
        signal: Option<SemanticNodeId>,
    },
    ChangePendingSubscription {
        object: SemanticTransactionNodeToken,
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
    ReorderMember {
        family: SemanticNodeId,
        member: SemanticNodeId,
        before: Option<SemanticNodeId>,
    },
    AddMemberRef {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    RemoveMemberRef {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    ReorderMemberRef {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
        before: Option<SemanticTransactionNodeRef>,
    },
    AddNode {
        token: SemanticTransactionNodeToken,
        creation: SemanticNodeCreation,
    },
    AddAnimation {
        state: SemanticAnimationState,
    },
    RemoveAnimation {
        animation: SemanticNodeId,
    },
    RemoveNode {
        node: SemanticNodeId,
    },
}

impl SemanticMutation {
    /// Existing semantic identity directly targeted by this mutation.
    ///
    /// Allocation mutations do not have an identity until commit and therefore
    /// return `None`. This keeps creation out of the existing-target conflict key
    /// space without reserving or manufacturing semantic identity before preflight.
    pub const fn target(&self) -> Option<SemanticNodeId> {
        match self {
            Self::SetSignal { signal, .. } => Some(*signal),
            Self::SetProperty { object, .. }
            | Self::ReplaceContent { object, .. }
            | Self::ChangeSubscription { object, .. } => Some(*object),
            Self::SetPendingProperty { .. }
            | Self::ReplacePendingContent { .. }
            | Self::ChangePendingSubscription { .. } => None,
            Self::AddMember { family, .. }
            | Self::RemoveMember { family, .. }
            | Self::ReorderMember { family, .. } => Some(*family),
            Self::AddMemberRef { family, .. }
            | Self::RemoveMemberRef { family, .. }
            | Self::ReorderMemberRef { family, .. } => family.existing(),
            Self::AddNode { .. } | Self::AddAnimation { .. } => None,
            Self::RemoveAnimation { animation } => Some(*animation),
            Self::RemoveNode { node } => Some(*node),
        }
    }

    const fn key(&self) -> Option<SemanticMutationKey> {
        match self {
            Self::SetSignal { signal, .. } => Some(SemanticMutationKey::Signal(*signal)),
            Self::SetProperty {
                object, property, ..
            } => Some(SemanticMutationKey::ObjectProperty {
                object: *object,
                property: *property,
            }),
            Self::SetPendingProperty {
                object, property, ..
            } => Some(SemanticMutationKey::PendingObjectProperty {
                object: *object,
                property: *property,
            }),
            Self::ReplaceContent { object, .. } => {
                Some(SemanticMutationKey::ObjectContent(*object))
            }
            Self::ReplacePendingContent { object, .. } => {
                Some(SemanticMutationKey::PendingObjectContent(*object))
            }
            Self::ChangeSubscription {
                object, property, ..
            } => Some(SemanticMutationKey::Subscription {
                object: *object,
                property: *property,
            }),
            Self::ChangePendingSubscription {
                object, property, ..
            } => Some(SemanticMutationKey::PendingSubscription {
                object: *object,
                property: *property,
            }),
            Self::AddMember { family, member } | Self::RemoveMember { family, member } => {
                Some(SemanticMutationKey::FamilyEdge {
                    family: (*family).into(),
                    member: (*member).into(),
                })
            }
            Self::AddMemberRef { family, member } | Self::RemoveMemberRef { family, member } => {
                Some(SemanticMutationKey::FamilyEdge {
                    family: *family,
                    member: *member,
                })
            }
            Self::ReorderMember { family, member, .. } => Some(SemanticMutationKey::FamilyOrder {
                family: (*family).into(),
                member: (*member).into(),
            }),
            Self::ReorderMemberRef { family, member, .. } => {
                Some(SemanticMutationKey::FamilyOrder {
                    family: *family,
                    member: *member,
                })
            }
            Self::AddNode { .. } | Self::AddAnimation { .. } => None,
            Self::RemoveAnimation { animation } => {
                Some(SemanticMutationKey::NodeRemoval(*animation))
            }
            Self::RemoveNode { node } => Some(SemanticMutationKey::NodeRemoval(*node)),
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
    PendingObjectProperty {
        object: SemanticTransactionNodeToken,
        property: SemanticObjectProperty,
    },
    ObjectContent(SemanticNodeId),
    PendingObjectContent(SemanticTransactionNodeToken),
    Subscription {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    PendingSubscription {
        object: SemanticTransactionNodeToken,
        property: SemanticObjectProperty,
    },
    FamilyEdge {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    FamilyOrder {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
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
    FamilyMemberReordered {
        family: SemanticNodeId,
        member: SemanticNodeId,
        before: Option<SemanticNodeId>,
    },
    NodeAdded {
        node: SemanticNodeId,
    },
    AnimationAdded {
        animation: SemanticNodeId,
    },
    NodeRemoved {
        node: SemanticNodeId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticMutationTransaction {
    scope: u64,
    mutations: Vec<SemanticMutation>,
}

impl Default for SemanticMutationTransaction {
    fn default() -> Self {
        Self {
            scope: next_semantic_transaction_scope(),
            mutations: Vec::new(),
        }
    }
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

    /// Set one property on a transaction-local pending object.
    pub fn set_pending_property(
        &mut self,
        object: SemanticTransactionNodeToken,
        property: SemanticObjectProperty,
        value: impl Into<SemanticSignalValue>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetPendingProperty {
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

    pub fn replace_pending_content(
        &mut self,
        object: SemanticTransactionNodeToken,
        content: impl Into<SemanticObjectContent>,
    ) -> &mut Self {
        self.mutations
            .push(SemanticMutation::ReplacePendingContent {
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

    pub fn change_pending_subscription(
        &mut self,
        object: SemanticTransactionNodeToken,
        property: SemanticObjectProperty,
        signal: Option<SemanticNodeId>,
    ) -> &mut Self {
        self.mutations
            .push(SemanticMutation::ChangePendingSubscription {
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

    /// Move one direct family member before another direct member, or to the tail.
    ///
    /// `before=None` means tail. Reordering preserves membership and parent edges;
    /// only the family's authoritative order is mutated.
    pub fn reorder_member(
        &mut self,
        family: SemanticNodeId,
        member: SemanticNodeId,
        before: Option<SemanticNodeId>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ReorderMember {
            family,
            member,
            before,
        });
        self
    }

    /// Add an edge where either endpoint may be transaction-local pending identity.
    pub fn add_member_ref(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::AddMemberRef {
            family: family.into(),
            member: member.into(),
        });
        self
    }

    pub fn remove_member_ref(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::RemoveMemberRef {
            family: family.into(),
            member: member.into(),
        });
        self
    }

    pub fn reorder_member_ref(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
        before: Option<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ReorderMemberRef {
            family: family.into(),
            member: member.into(),
            before,
        });
        self
    }

    /// Allocate one new detached semantic scene node after complete transaction
    /// preflight.
    ///
    /// Object/family creation uses the same scene-global generational allocator as
    /// all other semantic entities. The real identity is reported by `NodeAdded`;
    /// no provisional semantic ID is reserved before commit. Optional source
    /// identity is assigned atomically after terminal removals, allowing hot-reload
    /// replacement to transfer one stable source key from an old node to its new
    /// identity without creating a second identity model.
    pub fn add_node(&mut self, creation: SemanticNodeCreation) -> &mut Self {
        let _ = self.create_node(creation);
        self
    }

    /// Stage one new node and return its transaction-local token.
    pub fn create_node(&mut self, creation: SemanticNodeCreation) -> SemanticTransactionNodeToken {
        let mutation_index = u32::try_from(self.mutations.len())
            .expect("Noon semantic transaction mutation space exhausted");
        let token = SemanticTransactionNodeToken {
            scope: self.scope,
            mutation_index,
        };
        self.mutations
            .push(SemanticMutation::AddNode { token, creation });
        token
    }

    /// Read the effective staged state of a pending object without publishing it.
    pub fn pending_object_state(
        &self,
        token: SemanticTransactionNodeToken,
    ) -> Option<SemanticObjectState> {
        let mut state = match self.pending_creation(token)? {
            SemanticNodeCreation::Object { state, .. } => (**state).clone(),
            SemanticNodeCreation::Family { .. } => return None,
        };
        for mutation in self
            .mutations
            .iter()
            .skip(token.mutation_index as usize + 1)
        {
            match mutation {
                SemanticMutation::SetPendingProperty {
                    object,
                    property,
                    value,
                } if *object == token => {
                    if !value.is_finite() || value.value_kind() != property.value_kind() {
                        return None;
                    }
                    set_object_property_state(&mut state, *property, value.clone());
                }
                SemanticMutation::ReplacePendingContent { object, content } if *object == token => {
                    state.content = content.clone();
                }
                SemanticMutation::ChangePendingSubscription {
                    object,
                    property,
                    signal,
                } if *object == token => {
                    set_object_subscription_state(&mut state, *property, *signal);
                }
                _ => {}
            }
        }
        Some(state)
    }

    fn pending_creation(
        &self,
        token: SemanticTransactionNodeToken,
    ) -> Option<&SemanticNodeCreation> {
        if token.scope != self.scope {
            return None;
        }
        match self.mutations.get(token.mutation_index as usize)? {
            SemanticMutation::AddNode {
                token: stored,
                creation,
            } if *stored == token => Some(creation),
            _ => None,
        }
    }

    fn pending_object_creation(
        &self,
        token: SemanticTransactionNodeToken,
    ) -> Result<&SemanticObjectState, SemanticPendingNodeError> {
        match self.pending_creation(token) {
            Some(SemanticNodeCreation::Object { state, .. }) => Ok(state),
            Some(SemanticNodeCreation::Family { .. }) => Err(SemanticPendingNodeError::NotObject),
            None => Err(SemanticPendingNodeError::UnknownToken),
        }
    }

    fn pending_node_kinds(&self) -> HashMap<SemanticTransactionNodeToken, PendingNodeKind> {
        self.mutations
            .iter()
            .filter_map(|mutation| match mutation {
                SemanticMutation::AddNode { token, creation } => Some((
                    *token,
                    match creation {
                        SemanticNodeCreation::Object { .. } => PendingNodeKind::Object,
                        SemanticNodeCreation::Family { .. } => PendingNodeKind::Family,
                    },
                )),
                _ => None,
            })
            .collect()
    }

    /// Add one authored animation declaration after complete transaction preflight.
    ///
    /// The declaration references existing scene-global semantic identities. The
    /// newly allocated animation identity is reported by `AnimationAdded` in the
    /// transaction result, rather than allocating semantic identity before commit.
    pub fn add_animation(&mut self, state: SemanticAnimationState) -> &mut Self {
        self.mutations
            .push(SemanticMutation::AddAnimation { state });
        self
    }

    /// Delete one authored animation declaration through the same structural
    /// transaction path as node deletion.
    ///
    /// The target must be a live animation at the transaction boundary. Removing a
    /// child animation also removes parent compositions that can no longer remain
    /// valid; composition children are references and are not owned/deleted when a
    /// composition itself is removed.
    pub fn remove_animation(&mut self, animation: SemanticNodeId) -> &mut Self {
        self.mutations
            .push(SemanticMutation::RemoveAnimation { animation });
        self
    }

    /// Delete one semantic identity and atomically clean declarations that cannot
    /// remain valid without it.
    ///
    /// Signal bindings are unbound. Derived signals and animation declarations
    /// that reference the removed identity are themselves removed, recursively.
    /// Structural removals are terminal within a transaction: once the first
    /// structural removal is authored, no later non-removal mutation is accepted.
    /// This keeps complete preflight valid through commit without staging a second
    /// scene.
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
        let mut pending_source_assignments = Vec::new();
        let mut committed_nodes = HashMap::new();
        let mut committed_node_order = Vec::new();
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
                SemanticMutation::SetPendingProperty {
                    object,
                    property,
                    value,
                } => {
                    let object = *committed_nodes
                        .get(&object)
                        .expect("preflighted pending object must have committed before use");
                    set_object_property(store, object, property, value);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectProperty { object, property });
                }
                SemanticMutation::SetPendingProperty {
                    object,
                    property,
                    value,
                } => {
                    let state = self.pending_object_creation(*object).map_err(|error| {
                        SemanticMutationTransactionError::PendingNode {
                            index,
                            token: *object,
                            error,
                        }
                    })?;
                    if !value.is_finite() {
                        return Err(SemanticMutationTransactionError::PendingNode {
                            index,
                            token: *object,
                            error: SemanticPendingNodeError::NonFiniteProperty(*property),
                        });
                    }
                    let expected = property.value_kind();
                    let actual = value.value_kind();
                    if actual != expected {
                        return Err(SemanticMutationTransactionError::PendingNode {
                            index,
                            token: *object,
                            error: SemanticPendingNodeError::PropertyTypeMismatch {
                                property: *property,
                                expected,
                                actual,
                            },
                        });
                    }
                    changed.push(object_property_value(state, *property) != *value);
                }
                SemanticMutation::ReplaceContent { object, content } => {
                    set_object_content(store, object, content);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectContent { object });
                }
                SemanticMutation::ReplacePendingContent { object, content } => {
                    let object = *committed_nodes
                        .get(&object)
                        .expect("preflighted pending object must have committed before use");
                    set_object_content(store, object, content);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectContent { object });
                }
                SemanticMutation::ReplacePendingContent { object, content } => {
                    let state = self.pending_object_creation(*object).map_err(|error| {
                        SemanticMutationTransactionError::PendingNode {
                            index,
                            token: *object,
                            error,
                        }
                    })?;
                    changed.push(state.content != *content);
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
                SemanticMutation::ChangePendingSubscription {
                    object,
                    property,
                    signal,
                } => {
                    let object = *committed_nodes
                        .get(&object)
                        .expect("preflighted pending object must have committed before use");
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
                SemanticMutation::ReorderMember {
                    family,
                    member,
                    before,
                } => {
                    let reordered = store.reorder_member(family, member, before).expect(
                        "preflighted family reorder must remain valid while transaction owns the semantic store",
                    );
                    if !reordered {
                        continue;
                    }
                    written_slots.insert(family);
                    impacts.push(SemanticMutationImpact::FamilyMemberReordered {
                        family,
                        member,
                        before,
                    });
                }
                SemanticMutation::AddMemberRef { family, member } => {
                    let family = resolve_transaction_ref(family, &committed_nodes);
                    let member = resolve_transaction_ref(member, &committed_nodes);
                    store.add_member(family, member).expect(
                        "preflighted referenced family add must remain valid while transaction owns the semantic store",
                    );
                    written_slots.insert(family);
                    written_slots.insert(member);
                    impacts.push(SemanticMutationImpact::FamilyMemberAdded { family, member });
                }
                SemanticMutation::RemoveMemberRef { family, member } => {
                    let family = resolve_transaction_ref(family, &committed_nodes);
                    let member = resolve_transaction_ref(member, &committed_nodes);
                    let removed = store.remove_member(family, member).expect(
                        "preflighted referenced family removal must remain valid while transaction owns the semantic store",
                    );
                    debug_assert!(removed);
                    written_slots.insert(family);
                    written_slots.insert(member);
                    impacts.push(SemanticMutationImpact::FamilyMemberRemoved { family, member });
                }
                SemanticMutation::ReorderMemberRef {
                    family,
                    member,
                    before,
                } => {
                    let family = resolve_transaction_ref(family, &committed_nodes);
                    let member = resolve_transaction_ref(member, &committed_nodes);
                    let before = before.map(|node| resolve_transaction_ref(node, &committed_nodes));
                    let reordered = store.reorder_member(family, member, before).expect(
                        "preflighted referenced family reorder must remain valid while transaction owns the semantic store",
                    );
                    if !reordered {
                        continue;
                    }
                    written_slots.insert(family);
                    impacts.push(SemanticMutationImpact::FamilyMemberReordered {
                        family,
                        member,
                        before,
                    });
                }
                SemanticMutation::AddNode { token, creation } => {
                    let (node, source_identity) = commit_add_node(store, creation);
                    committed_nodes.insert(token, node);
                    committed_node_order.push((token, node));
                    written_slots.insert(node);
                    if let Some(source_identity) = source_identity {
                        pending_source_assignments.push((node, source_identity));
                    }
                    impacts.push(SemanticMutationImpact::NodeAdded { node });
                }
                SemanticMutation::AddAnimation { state } => {
                    let animation = commit_add_animation(store, &state);
                    written_slots.insert(animation);
                    impacts.push(SemanticMutationImpact::AnimationAdded { animation });
                }
                SemanticMutation::RemoveAnimation { animation }
                | SemanticMutation::RemoveNode { node: animation } => {
                    // An earlier explicit removal may have cascade-removed this
                    // node already. Preflight proved the handle was live at the
                    // transaction boundary; a cascade therefore satisfies this
                    // later structural mutation without duplicate impacts.
                    if store.node(animation).is_none() {
                        continue;
                    }
                    let outcome = store
                        .remove_node_with_reverse_cleanup(animation)
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

        for (node, source_identity) in pending_source_assignments {
            store
                .set_source_identity(node, Some(source_identity))
                .expect("preflighted source identity must be available after terminal removals");
        }
        store.set_last_mutation_writes(written_slots.len());

        Ok(SemanticMutationTransactionResult {
            impacts,
            committed_nodes: committed_node_order,
        })
    }

    fn preflight(
        &self,
        store: &SemanticStore,
    ) -> Result<Vec<bool>, SemanticMutationTransactionError> {
        let mut removed_nodes = HashSet::new();
        let mut removal_started = false;
        for (index, mutation) in self.mutations.iter().enumerate() {
            match mutation {
                SemanticMutation::RemoveAnimation { animation } => {
                    let Some(node) = store.node(*animation) else {
                        return Err(SemanticMutationTransactionError::UnknownAnimation {
                            index,
                            animation: *animation,
                        });
                    };
                    if !matches!(node.kind(), SemanticNodeKind::Animation(_)) {
                        return Err(SemanticMutationTransactionError::NotAnimation {
                            index,
                            animation: *animation,
                        });
                    }
                    removal_started = true;
                    removed_nodes.insert(*animation);
                }
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

        let pending_nodes = self.pending_node_kinds();
        let mut targets = HashSet::with_capacity(self.mutations.len());
        let mut changed = Vec::with_capacity(self.mutations.len());
        let mut family_edges = FamilyEdgePreflight::default();
        let mut pending_sources = HashSet::new();

        for (index, mutation) in self.mutations.iter().enumerate() {
            if !matches!(
                mutation,
                SemanticMutation::RemoveAnimation { .. } | SemanticMutation::RemoveNode { .. }
            ) {
                if let Some(target) = mutation.target() {
                    if removed_nodes.contains(&target) {
                        return Err(SemanticMutationTransactionError::TargetRemoved {
                            index,
                            target,
                        });
                    }
                }
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
            if let SemanticMutation::AddMemberRef { family, member }
            | SemanticMutation::RemoveMemberRef { family, member } = mutation
            {
                if let Some(member) = member
                    .existing()
                    .filter(|member| removed_nodes.contains(member))
                {
                    return Err(
                        SemanticMutationTransactionError::ReferencedFamilyUsesRemovedNode {
                            index,
                            family: *family,
                            node: member,
                        },
                    );
                }
            }
            if let SemanticMutation::ReorderMember {
                family,
                member,
                before,
            } = mutation
            {
                if removed_nodes.contains(member) {
                    return Err(
                        SemanticMutationTransactionError::FamilyOrderUsesRemovedNode {
                            index,
                            family: *family,
                            node: *member,
                        },
                    );
                }
                if let Some(anchor) = before {
                    if removed_nodes.contains(anchor) {
                        return Err(
                            SemanticMutationTransactionError::FamilyOrderUsesRemovedNode {
                                index,
                                family: *family,
                                node: *anchor,
                            },
                        );
                    }
                }
            }

            if let SemanticMutation::ReorderMemberRef {
                family,
                member,
                before,
            } = mutation
            {
                for node in std::iter::once(*member).chain(before.iter().copied()) {
                    if let Some(node) = node.existing().filter(|node| removed_nodes.contains(node))
                    {
                        return Err(
                            SemanticMutationTransactionError::ReferencedFamilyUsesRemovedNode {
                                index,
                                family: *family,
                                node,
                            },
                        );
                    }
                }
            }

            if let Some(key) = mutation.key() {
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
                        SemanticMutationKey::PendingObjectProperty { object, property } => {
                            SemanticMutationTransactionError::DuplicatePendingProperty {
                                index,
                                object,
                                property,
                            }
                        }
                        SemanticMutationKey::ObjectContent(object) => {
                            SemanticMutationTransactionError::DuplicateContent { index, object }
                        }
                        SemanticMutationKey::PendingObjectContent(object) => {
                            SemanticMutationTransactionError::DuplicatePendingContent {
                                index,
                                object,
                            }
                        }
                        SemanticMutationKey::Subscription { object, property } => {
                            SemanticMutationTransactionError::DuplicateSubscription {
                                index,
                                object,
                                property,
                            }
                        }
                        SemanticMutationKey::PendingSubscription { object, property } => {
                            SemanticMutationTransactionError::DuplicatePendingSubscription {
                                index,
                                object,
                                property,
                            }
                        }
                        SemanticMutationKey::FamilyEdge { family, member } => match (family, member)
                        {
                            (
                                SemanticTransactionNodeRef::Existing(family),
                                SemanticTransactionNodeRef::Existing(member),
                            ) => SemanticMutationTransactionError::DuplicateFamilyEdge {
                                index,
                                family,
                                member,
                            },
                            (family, member) => {
                                SemanticMutationTransactionError::DuplicateReferencedFamilyEdge {
                                    index,
                                    family,
                                    member,
                                }
                            }
                        },
                        SemanticMutationKey::FamilyOrder { family, member } => match (
                            family, member,
                        ) {
                            (
                                SemanticTransactionNodeRef::Existing(family),
                                SemanticTransactionNodeRef::Existing(member),
                            ) => SemanticMutationTransactionError::DuplicateFamilyOrder {
                                index,
                                family,
                                member,
                            },
                            (family, member) => {
                                SemanticMutationTransactionError::DuplicateReferencedFamilyOrder {
                                    index,
                                    family,
                                    member,
                                }
                            }
                        },
                        SemanticMutationKey::NodeRemoval(node) => {
                            SemanticMutationTransactionError::DuplicateNodeRemoval { index, node }
                        }
                    });
                }
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
                SemanticMutation::ChangePendingSubscription {
                    object,
                    property,
                    signal,
                } => {
                    let state = self.pending_object_creation(*object).map_err(|error| {
                        SemanticMutationTransactionError::PendingNode {
                            index,
                            token: *object,
                            error,
                        }
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
                            return Err(SemanticMutationTransactionError::PendingNode {
                                index,
                                token: *object,
                                error: SemanticPendingNodeError::SubscriptionTypeMismatch {
                                    property: *property,
                                    signal: *signal,
                                    expected,
                                    actual,
                                },
                            });
                        }
                        changed.push(existing != Some(*signal));
                    } else {
                        changed.push(existing.is_some());
                    }
                }
                SemanticMutation::AddMember { family, member } => {
                    changed.push(
                        family_edges
                            .add(store, &pending_nodes, (*family).into(), (*member).into())
                            .map_err(|error| map_family_preflight_error(index, error))?,
                    );
                }
                SemanticMutation::RemoveMember { family, member } => {
                    changed.push(
                        family_edges
                            .remove(store, &pending_nodes, (*family).into(), (*member).into())
                            .map_err(|error| map_family_preflight_error(index, error))?,
                    );
                }
                SemanticMutation::ReorderMember {
                    family,
                    member,
                    before,
                } => {
                    changed.push(
                        family_edges
                            .reorder(
                                store,
                                &pending_nodes,
                                (*family).into(),
                                (*member).into(),
                                before.map(Into::into),
                            )
                            .map_err(|error| map_family_preflight_error(index, error))?,
                    );
                }
                SemanticMutation::AddMemberRef { family, member } => {
                    changed.push(
                        family_edges
                            .add(store, &pending_nodes, *family, *member)
                            .map_err(|error| map_family_preflight_error(index, error))?,
                    );
                }
                SemanticMutation::RemoveMemberRef { family, member } => {
                    changed.push(
                        family_edges
                            .remove(store, &pending_nodes, *family, *member)
                            .map_err(|error| map_family_preflight_error(index, error))?,
                    );
                }
                SemanticMutation::ReorderMemberRef {
                    family,
                    member,
                    before,
                } => {
                    changed.push(
                        family_edges
                            .reorder(store, &pending_nodes, *family, *member, *before)
                            .map_err(|error| map_family_preflight_error(index, error))?,
                    );
                }
                SemanticMutation::AddNode { creation, .. } => {
                    preflight_add_node(
                        store,
                        creation,
                        &removed_nodes,
                        &mut pending_sources,
                        index,
                    )?;
                    changed.push(true);
                }
                SemanticMutation::AddAnimation { state } => {
                    preflight_add_animation(store, state, &removed_nodes, index)?;
                    changed.push(true);
                }
                SemanticMutation::RemoveAnimation { .. } => {
                    changed.push(true);
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

fn resolve_transaction_ref(
    node: SemanticTransactionNodeRef,
    committed_nodes: &HashMap<SemanticTransactionNodeToken, SemanticNodeId>,
) -> SemanticNodeId {
    match node {
        SemanticTransactionNodeRef::Existing(node) => node,
        SemanticTransactionNodeRef::Pending(token) => *committed_nodes
            .get(&token)
            .expect("preflighted pending semantic reference must resolve after creation"),
    }
}

fn map_family_preflight_error(
    index: usize,
    error: family_edges::FamilyEdgePreflightError,
) -> SemanticMutationTransactionError {
    match error {
        family_edges::FamilyEdgePreflightError::Existing(error) => {
            SemanticMutationTransactionError::Family { index, error }
        }
        family_edges::FamilyEdgePreflightError::Pending(error) => {
            SemanticMutationTransactionError::PendingFamily { index, error }
        }
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
    set_object_property_state(state, property, value);
}

fn set_object_property_state(
    state: &mut SemanticObjectState,
    property: SemanticObjectProperty,
    value: SemanticSignalValue,
) {
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
    let state = store
        .node_mut(object)
        .and_then(|node| node.semantic_object_state_mut())
        .expect("preflighted semantic object must remain valid while transaction owns the store");
    set_object_subscription_state(state, property, signal);
    store.register_semantic_references_for_owner(object);
}

fn set_object_subscription_state(
    state: &mut SemanticObjectState,
    property: SemanticObjectProperty,
    signal: Option<SemanticNodeId>,
) {
    let bindings = state.signal_bindings_mut();
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
    committed_nodes: Vec<(SemanticTransactionNodeToken, SemanticNodeId)>,
}

impl SemanticMutationTransactionResult {
    pub fn impacts(&self) -> &[SemanticMutationImpact] {
        &self.impacts
    }

    pub fn committed_node(&self, token: SemanticTransactionNodeToken) -> Option<SemanticNodeId> {
        self.committed_nodes
            .iter()
            .find_map(|(candidate, node)| (*candidate == token).then_some(*node))
    }

    pub fn committed_nodes(&self) -> &[(SemanticTransactionNodeToken, SemanticNodeId)] {
        &self.committed_nodes
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
    DuplicatePendingProperty {
        index: usize,
        object: SemanticTransactionNodeToken,
        property: SemanticObjectProperty,
    },
    DuplicateContent {
        index: usize,
        object: SemanticNodeId,
    },
    DuplicatePendingContent {
        index: usize,
        object: SemanticTransactionNodeToken,
    },
    DuplicateSubscription {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    DuplicatePendingSubscription {
        index: usize,
        object: SemanticTransactionNodeToken,
        property: SemanticObjectProperty,
    },
    DuplicateFamilyEdge {
        index: usize,
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    DuplicateFamilyOrder {
        index: usize,
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    DuplicateReferencedFamilyEdge {
        index: usize,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    DuplicateReferencedFamilyOrder {
        index: usize,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
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
    FamilyOrderUsesRemovedNode {
        index: usize,
        family: SemanticNodeId,
        node: SemanticNodeId,
    },
    ReferencedFamilyUsesRemovedNode {
        index: usize,
        family: SemanticTransactionNodeRef,
        node: SemanticNodeId,
    },
    NodeCreationUsesRemovedNode {
        index: usize,
        node: SemanticNodeId,
    },
    AnimationUsesRemovedNode {
        index: usize,
        node: SemanticNodeId,
    },
    InvalidNodeObjectState {
        index: usize,
    },
    NodeCreationBindingTypeMismatch {
        index: usize,
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
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
    PendingNode {
        index: usize,
        token: SemanticTransactionNodeToken,
        error: SemanticPendingNodeError,
    },
    PendingFamily {
        index: usize,
        error: SemanticPendingFamilyError,
    },
    Family {
        index: usize,
        error: SemanticSceneOperationError,
    },
    AnimationTarget {
        index: usize,
        error: SemanticSceneOperationError,
    },
    UnknownAnimation {
        index: usize,
        animation: SemanticNodeId,
    },
    NotAnimation {
        index: usize,
        animation: SemanticNodeId,
    },
    EmptyAnimationComposition {
        index: usize,
    },
    SameAnimationTargetAndTargetState {
        index: usize,
        node: SemanticNodeId,
    },
    InvalidAnimationRunTime {
        index: usize,
    },
    InvalidAnimationLagRatio {
        index: usize,
    },
    InvalidAnimationPathArc {
        index: usize,
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
            Self::DuplicatePendingProperty {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats property {:?} on pending node {}",
                property,
                object.local_index()
            ),
            Self::DuplicateContent { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} repeats content replacement on object {}:{}",
                object.slot(),
                object.generation()
            ),
            Self::DuplicatePendingContent { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} repeats content replacement on pending node {}",
                object.local_index()
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
            Self::DuplicatePendingSubscription {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats subscription for property {:?} on pending node {}",
                property,
                object.local_index()
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
            Self::DuplicateFamilyOrder {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats family reorder for member {}:{} in family {}:{}",
                member.slot(),
                member.generation(),
                family.slot(),
                family.generation()
            ),
            Self::DuplicateReferencedFamilyEdge {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats referenced family edge {family:?} -> {member:?}"
            ),
            Self::DuplicateReferencedFamilyOrder {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats referenced family reorder {family:?} / {member:?}"
            ),
            Self::DuplicateNodeRemoval { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} repeats removal of node {}:{}",
                node.slot(),
                node.generation()
            ),
            Self::MutationAfterRemove { index } => write!(
                formatter,
                "semantic transaction mutation {index} follows structural removal; structural removals must be terminal"
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
            Self::FamilyOrderUsesRemovedNode {
                index,
                family,
                node,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot reorder family {}:{} using node {}:{} because that node is removed by the same transaction",
                family.slot(),
                family.generation(),
                node.slot(),
                node.generation()
            ),
            Self::ReferencedFamilyUsesRemovedNode {
                index,
                family,
                node,
            } => write!(
                formatter,
                "semantic transaction mutation {index} uses removed node {}:{} in referenced family {family:?}",
                node.slot(),
                node.generation()
            ),
            Self::NodeCreationUsesRemovedNode { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add a node referencing semantic node {}:{} because that node is removed by the same transaction",
                node.slot(),
                node.generation()
            ),
            Self::AnimationUsesRemovedNode { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an animation referencing node {}:{} because that node is removed by the same transaction",
                node.slot(),
                node.generation()
            ),
            Self::InvalidNodeObjectState { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an object with non-finite authored transform/style values"
            ),
            Self::NodeCreationBindingTypeMismatch {
                index,
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an object binding {actual} signal {}:{} to a property requiring {expected}",
                signal.slot(),
                signal.generation()
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
            Self::PendingNode {
                index,
                token,
                error,
            } => write!(
                formatter,
                "semantic transaction mutation {index} pending node {}: {error:?}",
                token.local_index()
            ),
            Self::PendingFamily { index, error } => write!(
                formatter,
                "semantic transaction mutation {index} pending family reference: {error:?}"
            ),
            Self::AnimationTarget { index, error } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add animation: {error}"
            ),
            Self::UnknownAnimation { index, animation } => write!(
                formatter,
                "semantic transaction mutation {index}: unknown semantic animation {}:{}",
                animation.slot(),
                animation.generation()
            ),
            Self::NotAnimation { index, animation } => write!(
                formatter,
                "semantic transaction mutation {index}: semantic node {}:{} is not an animation",
                animation.slot(),
                animation.generation()
            ),
            Self::EmptyAnimationComposition { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an empty animation composition"
            ),
            Self::SameAnimationTargetAndTargetState { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add animation with identical target and target-state node {}:{}",
                node.slot(),
                node.generation()
            ),
            Self::InvalidAnimationRunTime { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add animation with non-finite or non-positive run_time"
            ),
            Self::InvalidAnimationLagRatio { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add animation with non-finite or negative lag_ratio"
            ),
            Self::InvalidAnimationPathArc { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add animation with non-finite path_arc"
            ),
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
mod reorder_member_tests;

#[cfg(test)]
mod add_node_tests;

#[cfg(test)]
mod pending_node_tests;

#[cfg(test)]
mod add_animation_tests;

#[cfg(test)]
mod remove_animation_tests;

#[cfg(test)]
mod remove_node_tests;
