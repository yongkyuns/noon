use std::collections::{HashMap, HashSet};

use super::semantic_declarations::{
    close_all_updater_registrations, close_first_updater_registration, insert_updater_registration,
    UpdaterRegistrationEditError,
};
use super::semantic_store::SemanticRemoveNodeEffect;
use super::{
    AnimationOptions, HostCallbackId, SemanticAnimationCompositionKind, SemanticAnimationState,
    SemanticFadeDirection, SemanticNodeId, SemanticNodeKind, SemanticObjectContent,
    SemanticObjectProperty, SemanticObjectState, SemanticScalarSignalHold,
    SemanticScalarSignalTimelineEntry, SemanticScalarSignalTrack, SemanticScalarSignalTrackError,
    SemanticSceneOperationError, SemanticSignalBinding, SemanticSignalError, SemanticSignalSource,
    SemanticSignalValue, SemanticSignalValueKind, SemanticStore, SemanticStoreError, SemanticStyle,
    SemanticUpdaterRegistration, StoredGeometry,
};
use crate::TrackTiming;

mod prepared;
pub use prepared::{PreparedSemanticMutationTransaction, SemanticTransactionReadError};

mod animation_addition;
use animation_addition::{commit_add_animation, preflight_transaction_animation};
pub use animation_addition::{SemanticTransactionAnimation, SemanticTransactionAnimationIntent};

mod family_edges;
use family_edges::FamilyEdgePreflight;

mod node_addition;
pub use node_addition::SemanticNodeCreation;
use node_addition::{commit_add_node, preflight_add_node};

mod provisional;
use provisional::{
    conflicting_style_error, duplicate_mutation_error, invalid_style_error,
    non_finite_property_error, property_type_error, replace_object_binding,
    subscription_type_error,
};
use provisional::{next_transaction_id, TransactionNodeCatalog};
pub use provisional::{
    SemanticLocalNodeToken, SemanticPendingNodeKind, SemanticTransactionNodeRef,
};

/// One mutation in the authoritative Semantic Scene transaction vocabulary.
///
/// Signal values, object properties, authored content, scene-node allocation,
/// family membership/order, reactive subscriptions, ordered updater registrations,
/// animation declarations, and structural deletion share the same transaction so
/// frontends, editors, and host integrations cannot invent subsystem-specific patch
/// paths. Dependency-expression rewiring remains authored declaration topology
/// rather than being conflated with a value update.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticMutation {
    SetSignal {
        signal: SemanticNodeId,
        value: SemanticSignalValue,
    },
    AddScalarSignalTrack {
        signal: SemanticNodeId,
        from: f64,
        to: f64,
        timing: TrackTiming,
    },
    SetScalarSignalAt {
        signal: SemanticNodeId,
        value: f64,
        time: f64,
    },
    SetProperty {
        object: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
        value: SemanticSignalValue,
    },
    ReplaceContent {
        object: SemanticTransactionNodeRef,
        content: SemanticObjectContent,
    },
    ReplaceStyle {
        object: SemanticTransactionNodeRef,
        style: SemanticStyle,
    },
    ChangeSubscription {
        object: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
        signal: Option<SemanticNodeId>,
    },
    AddUpdater {
        target: SemanticTransactionNodeRef,
        callback: HostCallbackId,
        active_from: f64,
        position: Option<usize>,
    },
    RemoveUpdater {
        target: SemanticTransactionNodeRef,
        callback: HostCallbackId,
        inactive_from: f64,
    },
    ClearUpdaters {
        target: SemanticTransactionNodeRef,
        inactive_from: f64,
    },
    ScopeSignal {
        scope: SemanticTransactionNodeRef,
        signal: SemanticTransactionNodeRef,
    },
    AddMember {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    RemoveMember {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    ReorderMember {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
        before: Option<SemanticTransactionNodeRef>,
    },
    AddNode {
        token: SemanticLocalNodeToken,
        creation: SemanticNodeCreation,
    },
    AddAnimation {
        token: SemanticLocalNodeToken,
        animation: SemanticTransactionAnimation,
    },
    RemoveAnimation {
        animation: SemanticNodeId,
    },
    RemoveNode {
        node: SemanticTransactionNodeRef,
    },
}

impl SemanticMutation {
    fn node_references(&self) -> Vec<SemanticTransactionNodeRef> {
        match self {
            Self::SetProperty { object, .. }
            | Self::ReplaceContent { object, .. }
            | Self::ReplaceStyle { object, .. }
            | Self::ChangeSubscription { object, .. } => vec![*object],
            Self::AddUpdater { target, .. }
            | Self::RemoveUpdater { target, .. }
            | Self::ClearUpdaters { target, .. } => vec![*target],
            Self::ScopeSignal { scope, signal } => vec![*scope, *signal],
            Self::AddMember { family, member } | Self::RemoveMember { family, member } => {
                vec![*family, *member]
            }
            Self::ReorderMember {
                family,
                member,
                before,
            } => {
                let mut references = vec![*family, *member];
                references.extend(*before);
                references
            }
            Self::RemoveNode { node } => vec![*node],
            Self::AddAnimation { animation, .. } => animation.intent().node_references().collect(),
            Self::SetSignal { .. }
            | Self::AddScalarSignalTrack { .. }
            | Self::SetScalarSignalAt { .. }
            | Self::AddNode { .. }
            | Self::RemoveAnimation { .. } => Vec::new(),
        }
    }

    fn references_any_pending(&self, removed: &HashSet<SemanticLocalNodeToken>) -> bool {
        self.node_references().into_iter().any(
            |node| matches!(node, SemanticTransactionNodeRef::Pending(token) if removed.contains(&token)),
        ) || matches!(self, Self::AddNode { token, .. } | Self::AddAnimation { token, .. } if removed.contains(token))
    }

    /// Existing semantic identity directly targeted by this mutation.
    ///
    /// Allocation mutations do not have an identity until commit and therefore
    /// return `None`. This keeps creation out of the existing-target conflict key
    /// space without reserving or manufacturing semantic identity before preflight.
    pub const fn target(&self) -> Option<SemanticNodeId> {
        match self {
            Self::SetSignal { signal, .. } => Some(*signal),
            Self::AddScalarSignalTrack { signal, .. } => Some(*signal),
            Self::SetScalarSignalAt { signal, .. } => Some(*signal),
            Self::SetProperty { object, .. }
            | Self::ReplaceContent { object, .. }
            | Self::ReplaceStyle { object, .. }
            | Self::ChangeSubscription { object, .. } => object.existing(),
            Self::AddUpdater { target, .. }
            | Self::RemoveUpdater { target, .. }
            | Self::ClearUpdaters { target, .. } => target.existing(),
            Self::ScopeSignal { scope, .. } => scope.existing(),
            Self::AddMember { family, .. }
            | Self::RemoveMember { family, .. }
            | Self::ReorderMember { family, .. } => family.existing(),
            Self::AddNode { .. } | Self::AddAnimation { .. } => None,
            Self::RemoveAnimation { animation } => Some(*animation),
            Self::RemoveNode { node } => node.existing(),
        }
    }

    const fn key(&self) -> Option<SemanticMutationKey> {
        match self {
            Self::SetSignal { signal, .. } => Some(SemanticMutationKey::Signal(*signal)),
            Self::AddScalarSignalTrack { .. } | Self::SetScalarSignalAt { .. } => None,
            Self::SetProperty {
                object, property, ..
            } => Some(SemanticMutationKey::ObjectProperty {
                object: *object,
                property: *property,
            }),
            Self::ReplaceContent { object, .. } => {
                Some(SemanticMutationKey::ObjectContent(*object))
            }
            Self::ReplaceStyle { object, .. } => Some(SemanticMutationKey::ObjectStyle(*object)),
            Self::ChangeSubscription {
                object, property, ..
            } => Some(SemanticMutationKey::Subscription {
                object: *object,
                property: *property,
            }),
            Self::AddUpdater { .. }
            | Self::RemoveUpdater { .. }
            | Self::ClearUpdaters { .. }
            | Self::ScopeSignal { .. } => None,
            Self::AddMember { family, member } | Self::RemoveMember { family, member } => {
                Some(SemanticMutationKey::FamilyEdge {
                    family: *family,
                    member: *member,
                })
            }
            Self::ReorderMember { family, member, .. } => Some(SemanticMutationKey::FamilyOrder {
                family: *family,
                member: *member,
            }),
            Self::AddNode { .. } | Self::AddAnimation { .. } => None,
            Self::RemoveAnimation { animation } => Some(SemanticMutationKey::NodeRemoval(
                SemanticTransactionNodeRef::Existing(*animation),
            )),
            Self::RemoveNode { node } => Some(SemanticMutationKey::NodeRemoval(*node)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum SemanticMutationKey {
    Signal(SemanticNodeId),
    ObjectProperty {
        object: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
    },
    ObjectContent(SemanticTransactionNodeRef),
    ObjectStyle(SemanticTransactionNodeRef),
    Subscription {
        object: SemanticTransactionNodeRef,
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
    NodeRemoval(SemanticTransactionNodeRef),
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
    SignalTimeline {
        signal: SemanticNodeId,
    },
    ObjectProperty {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    ObjectContent {
        object: SemanticNodeId,
    },
    ObjectStyle {
        object: SemanticNodeId,
    },
    Subscription {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    UpdaterRegistrations {
        target: SemanticNodeId,
    },
    SignalScoped {
        scope: SemanticNodeId,
        signal: SemanticNodeId,
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

#[derive(Debug, PartialEq)]
pub struct SemanticMutationTransaction {
    id: u32,
    next_token: u32,
    mutations: Vec<SemanticMutation>,
}

pub(super) struct SemanticTransactionPreflight {
    changed: Vec<bool>,
    staged_objects: HashMap<SemanticTransactionNodeRef, SemanticObjectState>,
    staged_object_order: Vec<SemanticTransactionNodeRef>,
    family_edges: FamilyEdgePreflight,
    pending_creations: HashMap<SemanticLocalNodeToken, SemanticNodeCreation>,
    pending_animations: HashMap<SemanticLocalNodeToken, SemanticTransactionAnimation>,
    staged_signal_scope_additions: Vec<(SemanticTransactionNodeRef, SemanticTransactionNodeRef)>,
    removed_existing: HashSet<SemanticNodeId>,
    removed_pending: HashSet<SemanticLocalNodeToken>,
}

impl Default for SemanticMutationTransaction {
    fn default() -> Self {
        let id = next_transaction_id();
        Self {
            id,
            next_token: 0,
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

    pub fn add_scalar_signal_track(
        &mut self,
        signal: SemanticNodeId,
        from: f64,
        to: f64,
        timing: TrackTiming,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::AddScalarSignalTrack {
            signal,
            from,
            to,
            timing,
        });
        self
    }

    /// Persist one scalar input value from an explicit authored time onward.
    pub fn set_scalar_signal_at(
        &mut self,
        signal: SemanticNodeId,
        value: f64,
        time: f64,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetScalarSignalAt {
            signal,
            value,
            time,
        });
        self
    }

    /// Set one node-owned authored object property in the same atomic mutation
    /// vocabulary as signal values.
    pub fn set_property(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        property: SemanticObjectProperty,
        value: impl Into<SemanticSignalValue>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetProperty {
            object: object.into(),
            property,
            value: value.into(),
        });
        self
    }

    /// Replace only the authored content reference/value of one semantic object.
    pub fn replace_content(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        content: impl Into<SemanticObjectContent>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ReplaceContent {
            object: object.into(),
            content: content.into(),
        });
        self
    }

    /// Replace the complete authored style of one semantic object.
    ///
    /// This carries paints and discrete stroke topology through the same atomic
    /// mutation vocabulary as scalar style properties. A transaction cannot mix
    /// full-style replacement with scalar style writes for the same object.
    pub fn replace_style(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        style: SemanticStyle,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ReplaceStyle {
            object: object.into(),
            style,
        });
        self
    }

    /// Change the authored signal driver for one object property.
    pub fn change_subscription(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        property: SemanticObjectProperty,
        signal: Option<SemanticNodeId>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ChangeSubscription {
            object: object.into(),
            property,
            signal,
        });
        self
    }

    /// Register one ordered host-updater occurrence on an object or family.
    ///
    /// `active_from` is inclusive authored time. `position` indexes the occurrences
    /// active at that time; `None` appends after the last active occurrence.
    pub fn add_updater(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        callback: HostCallbackId,
        active_from: f64,
        position: Option<usize>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::AddUpdater {
            target: target.into(),
            callback,
            active_from,
            position,
        });
        self
    }

    /// Close the first occurrence of `callback` active at exclusive authored time.
    pub fn remove_updater(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        callback: HostCallbackId,
        inactive_from: f64,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::RemoveUpdater {
            target: target.into(),
            callback,
            inactive_from,
        });
        self
    }

    /// Close every updater occurrence active on a target at exclusive authored time.
    pub fn clear_updaters(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        inactive_from: f64,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ClearUpdaters {
            target: target.into(),
            inactive_from,
        });
        self
    }

    /// Associate a signal with one family execution scope without changing
    /// painter membership. Repeated association is an exact no-op.
    pub fn scope_signal(
        &mut self,
        scope: impl Into<SemanticTransactionNodeRef>,
        signal: impl Into<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ScopeSignal {
            scope: scope.into(),
            signal: signal.into(),
        });
        self
    }

    /// Add one direct ordered family edge through the authoritative transaction.
    pub fn add_member(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::AddMember {
            family: family.into(),
            member: member.into(),
        });
        self
    }

    /// Remove one direct ordered family edge through the authoritative transaction.
    pub fn remove_member(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::RemoveMember {
            family: family.into(),
            member: member.into(),
        });
        self
    }

    /// Move one direct family member before another direct member, or to the tail.
    ///
    /// `before=None` means tail. Reordering preserves membership and parent edges;
    /// only the family's authoritative order is mutated.
    pub fn reorder_member(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
        before: Option<SemanticNodeId>,
    ) -> &mut Self {
        self.reorder_member_ref(family, member, before.map(Into::into))
    }

    /// Move a direct family member using existing or transaction-local identities.
    pub fn reorder_member_ref(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
        before: Option<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ReorderMember {
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
        self.create_node(creation);
        self
    }

    /// Stage a node allocation and return its transaction-local reference token.
    pub fn create_node(&mut self, creation: SemanticNodeCreation) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations
            .push(SemanticMutation::AddNode { token, creation });
        token
    }

    /// Add one authored animation declaration after complete transaction preflight.
    ///
    /// The declaration references existing scene-global semantic identities. The
    /// newly allocated animation identity is reported by `AnimationAdded` in the
    /// transaction result, rather than allocating semantic identity before commit.
    pub fn add_animation(&mut self, state: SemanticAnimationState) -> &mut Self {
        self.create_animation(state);
        self
    }

    /// Stage an animation declaration and return its transaction-local node token.
    pub fn create_animation(&mut self, state: SemanticAnimationState) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::from_published(state),
        });
        token
    }

    /// Add a transform declaration that may reference objects staged in this batch.
    pub fn add_transform_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        target_state: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> &mut Self {
        self.create_transform_animation(target, target_state, options);
        self
    }

    /// Stage a transform declaration and return its transaction-local node token.
    pub fn create_transform_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        target_state: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::TransformTo {
                    target: target.into(),
                    target_state: target_state.into(),
                },
                options,
            ),
        });
        token
    }

    /// Stage a single-leaf fade declaration and return its transaction-local token.
    pub fn create_fade_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        direction: SemanticFadeDirection,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::Fade {
                    target: target.into(),
                    direction,
                },
                options,
            ),
        });
        token
    }

    /// Stage a single-leaf Create declaration and return its transaction-local token.
    pub fn create_create_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::Create {
                    target: target.into(),
                },
                options,
            ),
        });
        token
    }

    /// Stage an ordered animation composition whose children already exist or
    /// were staged earlier in this transaction.
    pub fn create_animation_composition<I, R>(
        &mut self,
        kind: SemanticAnimationCompositionKind,
        children: I,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken
    where
        I: IntoIterator<Item = R>,
        R: Into<SemanticTransactionNodeRef>,
    {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::Composition {
                    kind,
                    children: children.into_iter().map(Into::into).collect(),
                },
                options,
            ),
        });
        token
    }

    fn allocate_local_node_token(&mut self) -> SemanticLocalNodeToken {
        let ordinal = self.next_token;
        self.next_token = self
            .next_token
            .checked_add(1)
            .expect("Noon semantic transaction local-node space exhausted");
        SemanticLocalNodeToken::new(self.id, ordinal)
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
    /// scene. Removing a pending node cancels its allocation and any staged edge or
    /// animation declaration that references it; those mutations are still fully
    /// preflighted so cancellation cannot hide an invalid family, kind, or value.
    pub fn remove_node(&mut self, node: impl Into<SemanticTransactionNodeRef>) -> &mut Self {
        self.mutations
            .push(SemanticMutation::RemoveNode { node: node.into() });
        self
    }

    pub fn mutations(&self) -> &[SemanticMutation] {
        &self.mutations
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    /// Validate and reserve publication while holding the store exclusively.
    /// Dropping the returned proof discards the batch without changing the store.
    pub fn prepare(
        self,
        store: &mut SemanticStore,
    ) -> Result<PreparedSemanticMutationTransaction<'_>, SemanticMutationTransactionError> {
        PreparedSemanticMutationTransaction::new(self, store)
    }

    /// Preflight the complete transaction, then commit every changed mutation.
    pub fn apply(
        self,
        store: &mut SemanticStore,
    ) -> Result<SemanticMutationTransactionResult, SemanticMutationTransactionError> {
        store.set_last_mutation_writes(0);
        self.prepare(store)
            .map(PreparedSemanticMutationTransaction::commit)
    }

    fn preflight(
        &self,
        store: &SemanticStore,
    ) -> Result<SemanticTransactionPreflight, SemanticMutationTransactionError> {
        let catalog = TransactionNodeCatalog::new(self, store);
        let mut removed_nodes = HashSet::new();
        let mut removed_pending = HashSet::new();
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
                SemanticMutation::RemoveNode {
                    node: SemanticTransactionNodeRef::Existing(node),
                } => {
                    removal_started = true;
                    removed_nodes.insert(*node);
                }
                SemanticMutation::RemoveNode {
                    node: SemanticTransactionNodeRef::Pending(token),
                } => {
                    catalog.validate_pending((*token).into(), index)?;
                    removal_started = true;
                    removed_pending.insert(*token);
                }
                _ if removal_started => {
                    return Err(SemanticMutationTransactionError::MutationAfterRemove { index });
                }
                _ => {}
            }
        }
        // Pending animation declarations form an insertion-ordered DAG because
        // compositions can only reference earlier pending animations. Canceling
        // any pending dependency therefore invalidates its parent declarations
        // transitively in one forward pass.
        for mutation in &self.mutations {
            let SemanticMutation::AddAnimation { token, animation } = mutation else {
                continue;
            };
            if animation.intent().node_references().any(|reference| {
                matches!(reference, SemanticTransactionNodeRef::Pending(dependency) if removed_pending.contains(&dependency))
            }) {
                removed_pending.insert(*token);
            }
        }
        let removed_nodes = store.semantic_removal_closure(&removed_nodes);
        let pending_creations = catalog.cloned_creations();
        let pending_animations = catalog.cloned_animations();
        let surviving_object_creations = pending_creations
            .iter()
            .filter(|(token, creation)| {
                !removed_pending.contains(token)
                    && matches!(creation, SemanticNodeCreation::Object { .. })
            })
            .count();
        let surviving_object_creations = u64::try_from(surviving_object_creations)
            .map_err(|_| SemanticMutationTransactionError::InsertionOrderExhausted)?;
        if store
            .next_insertion_order()
            .checked_add(surviving_object_creations)
            .is_none()
        {
            return Err(SemanticMutationTransactionError::InsertionOrderExhausted);
        }

        let mut targets = HashSet::with_capacity(self.mutations.len());
        let mut style_replacements = HashSet::new();
        let mut style_property_writes = HashSet::new();
        let mut changed = Vec::with_capacity(self.mutations.len());
        let mut family_edges = FamilyEdgePreflight::default();
        let mut pending_sources = HashSet::new();
        let mut staged_objects = HashMap::new();
        let mut staged_object_order = Vec::new();
        let mut staged_updaters =
            HashMap::<SemanticTransactionNodeRef, Vec<SemanticUpdaterRegistration>>::new();
        let mut staged_signal_timeline =
            HashMap::<SemanticNodeId, Vec<SemanticScalarSignalTimelineEntry>>::new();
        let mut staged_signal_scope_additions = Vec::new();
        let mut staged_signal_scope_membership = HashSet::new();
        let mut available_pending_animations = HashSet::new();

        for (index, mutation) in self.mutations.iter().enumerate() {
            for node in mutation.node_references() {
                catalog.validate_pending(node, index)?;
            }
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
                    return Err(match object {
                        SemanticTransactionNodeRef::Existing(object) => {
                            SemanticMutationTransactionError::SubscriptionUsesRemovedSignal {
                                index,
                                object: *object,
                                property: *property,
                                signal: *signal,
                            }
                        }
                        SemanticTransactionNodeRef::Pending(token) => {
                            SemanticMutationTransactionError::PendingSubscriptionUsesRemovedSignal {
                                index,
                                object: *token,
                                property: *property,
                                signal: *signal,
                            }
                        }
                    });
                }
            }
            if let SemanticMutation::AddMember { family, member }
            | SemanticMutation::RemoveMember { family, member } = mutation
            {
                if let SemanticTransactionNodeRef::Existing(member) = member {
                    if removed_nodes.contains(member) {
                        let SemanticTransactionNodeRef::Existing(family) = family else {
                            return Err(SemanticMutationTransactionError::PendingFamilyEdgeUsesRemovedNode {
                            index,
                            family: *family,
                            member: *member,
                        });
                        };
                        return Err(
                            SemanticMutationTransactionError::FamilyEdgeUsesRemovedNode {
                                index,
                                family: *family,
                                member: *member,
                            },
                        );
                    }
                }
            }
            if let SemanticMutation::ReorderMember {
                family,
                member,
                before,
            } = mutation
            {
                if let SemanticTransactionNodeRef::Existing(member) = member {
                    if removed_nodes.contains(member) {
                        let SemanticTransactionNodeRef::Existing(family) = family else {
                            return Err(SemanticMutationTransactionError::PendingFamilyOrderUsesRemovedNode {
                            index,
                            family: *family,
                            node: (*member).into(),
                        });
                        };
                        return Err(
                            SemanticMutationTransactionError::FamilyOrderUsesRemovedNode {
                                index,
                                family: *family,
                                node: *member,
                            },
                        );
                    }
                }
                if let Some(SemanticTransactionNodeRef::Existing(anchor)) = before {
                    if removed_nodes.contains(anchor) {
                        let SemanticTransactionNodeRef::Existing(family) = family else {
                            return Err(SemanticMutationTransactionError::PendingFamilyOrderUsesRemovedNode {
                                index,
                                family: *family,
                                node: (*anchor).into(),
                            });
                        };
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
            if let SemanticMutation::AddAnimation { animation, .. } = mutation {
                for node in animation.intent().node_references() {
                    if let SemanticTransactionNodeRef::Existing(node) = node {
                        if removed_nodes.contains(&node) {
                            return Err(
                                SemanticMutationTransactionError::AnimationUsesRemovedNode {
                                    index,
                                    node,
                                },
                            );
                        }
                    }
                }
            }

            match mutation {
                SemanticMutation::ReplaceStyle { object, .. } => {
                    if style_property_writes.contains(object) {
                        return Err(conflicting_style_error(index, *object));
                    }
                    style_replacements.insert(*object);
                }
                SemanticMutation::SetProperty {
                    object, property, ..
                } if is_style_property(*property) => {
                    if style_replacements.contains(object) {
                        return Err(conflicting_style_error(index, *object));
                    }
                    style_property_writes.insert(*object);
                }
                _ => {}
            }

            if let Some(key) = mutation.key() {
                if !targets.insert(key) {
                    return Err(duplicate_mutation_error(index, key));
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
                    if state.native_input().is_some() {
                        return Err(SemanticMutationTransactionError::Signal {
                            index,
                            error: SemanticSignalError::NativeOwnedSignal { signal: *signal },
                        });
                    }
                    if !state.scalar_timeline().is_empty()
                        || staged_signal_timeline.contains_key(signal)
                    {
                        return Err(SemanticMutationTransactionError::Signal {
                            index,
                            error: SemanticSignalError::TimelineOwnedSignal { signal: *signal },
                        });
                    }
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
                    if *property == SemanticObjectProperty::Presence {
                        return Err(SemanticMutationTransactionError::UnsupportedPropertyWrite {
                            index,
                            object: *object,
                            property: *property,
                        });
                    }
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    if !value.is_finite() {
                        return Err(non_finite_property_error(index, *object, *property));
                    }
                    let expected = property.value_kind();
                    let actual = value.value_kind();
                    if actual != expected {
                        return Err(property_type_error(
                            index, *object, *property, expected, actual,
                        ));
                    }
                    let did_change = object_property_value(state, *property) != *value;
                    if did_change {
                        apply_object_property(state, *property, value.clone());
                    }
                    changed.push(did_change);
                }
                SemanticMutation::ReplaceContent { object, content } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    validate_object_content_resource(store, *content, index)?;
                    let did_change = state.content != *content;
                    if did_change {
                        state.content = *content;
                    }
                    changed.push(did_change);
                }
                SemanticMutation::ReplaceStyle { object, style } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    if !style.is_finite() {
                        return Err(invalid_style_error(index, *object));
                    }
                    let did_change = state.style != *style;
                    if did_change {
                        state.style = style.clone();
                    }
                    changed.push(did_change);
                }
                SemanticMutation::ChangeSubscription {
                    object,
                    property,
                    signal,
                } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
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
                            return Err(subscription_type_error(
                                index, *object, *property, *signal, expected, actual,
                            ));
                        }
                        let did_change = existing != Some(*signal);
                        if did_change {
                            replace_object_binding(state, *property, Some(*signal));
                        }
                        changed.push(did_change);
                    } else {
                        let did_change = existing.is_some();
                        if did_change {
                            replace_object_binding(state, *property, None);
                        }
                        changed.push(did_change);
                    }
                }
                SemanticMutation::AddUpdater {
                    target,
                    callback,
                    active_from,
                    position,
                } => {
                    catalog.ensure_authoring_node(*target, index)?;
                    let registration =
                        SemanticUpdaterRegistration::new(*callback, *active_from, None)
                            .map_err(|_| invalid_updater_interval(index, *target))?;
                    let registrations = staged_updaters
                        .entry(*target)
                        .or_insert_with(|| catalog.updater_registrations(*target));
                    insert_updater_registration(registrations, registration, *position)
                        .map_err(|error| updater_edit_error(index, *target, error))?;
                    changed.push(true);
                }
                SemanticMutation::AddScalarSignalTrack {
                    signal,
                    from,
                    to,
                    timing,
                } => {
                    if targets.contains(&SemanticMutationKey::Signal(*signal)) {
                        return Err(SemanticMutationTransactionError::Signal {
                            index,
                            error: SemanticSignalError::TimelineOwnedSignal { signal: *signal },
                        });
                    }
                    let track = SemanticScalarSignalTrack::new(*signal, *from, *to, *timing);
                    let existing_last = store
                        .semantic_signal_state(*signal)
                        .ok()
                        .and_then(|state| state.scalar_timeline().last().copied());
                    let timeline = staged_signal_timeline.entry(*signal).or_default();
                    let previous = timeline.last().copied().or(existing_last);
                    store
                        .validate_semantic_scalar_signal_track_after(track, previous)
                        .map_err(|error| SemanticMutationTransactionError::SignalTrack {
                            index,
                            error,
                        })?;
                    timeline.push(SemanticScalarSignalTimelineEntry::Track(track));
                    changed.push(true);
                }
                SemanticMutation::SetScalarSignalAt {
                    signal,
                    value,
                    time,
                } => {
                    if targets.contains(&SemanticMutationKey::Signal(*signal)) {
                        return Err(SemanticMutationTransactionError::Signal {
                            index,
                            error: SemanticSignalError::TimelineOwnedSignal { signal: *signal },
                        });
                    }
                    let hold = SemanticScalarSignalHold::new(*signal, *value, *time);
                    let existing_last = store
                        .semantic_signal_state(*signal)
                        .ok()
                        .and_then(|state| state.scalar_timeline().last().copied());
                    let timeline = staged_signal_timeline.entry(*signal).or_default();
                    let previous = timeline.last().copied().or(existing_last);
                    store
                        .validate_semantic_scalar_signal_entry_after(
                            SemanticScalarSignalTimelineEntry::Hold(hold),
                            previous,
                        )
                        .map_err(|error| SemanticMutationTransactionError::SignalTrack {
                            index,
                            error,
                        })?;
                    timeline.push(SemanticScalarSignalTimelineEntry::Hold(hold));
                    changed.push(true);
                }
                SemanticMutation::RemoveUpdater {
                    target,
                    callback,
                    inactive_from,
                } => {
                    catalog.ensure_authoring_node(*target, index)?;
                    validate_updater_boundary(index, *target, *inactive_from)?;
                    let registrations = staged_updaters
                        .entry(*target)
                        .or_insert_with(|| catalog.updater_registrations(*target));
                    let did_change =
                        close_first_updater_registration(registrations, *callback, *inactive_from)
                            .map_err(|error| updater_edit_error(index, *target, error))?;
                    changed.push(did_change);
                }
                SemanticMutation::ClearUpdaters {
                    target,
                    inactive_from,
                } => {
                    catalog.ensure_authoring_node(*target, index)?;
                    validate_updater_boundary(index, *target, *inactive_from)?;
                    let registrations = staged_updaters
                        .entry(*target)
                        .or_insert_with(|| catalog.updater_registrations(*target));
                    let did_change = close_all_updater_registrations(registrations, *inactive_from)
                        .map_err(|error| updater_edit_error(index, *target, error))?;
                    changed.push(did_change);
                }
                SemanticMutation::ScopeSignal { scope, signal } => {
                    catalog.ensure_family(*scope, index)?;
                    catalog.ensure_signal(*signal, index)?;
                    if matches!(signal, SemanticTransactionNodeRef::Existing(id) if removed_nodes.contains(id))
                    {
                        return Err(
                            SemanticMutationTransactionError::SignalScopeUsesRemovedNode {
                                index,
                                scope: *scope,
                                signal: *signal,
                            },
                        );
                    }
                    let pair = (*scope, *signal);
                    let already_scoped = staged_signal_scope_membership.contains(&pair)
                        || matches!(
                            pair,
                            (
                                SemanticTransactionNodeRef::Existing(scope),
                                SemanticTransactionNodeRef::Existing(signal)
                            ) if store.is_semantic_signal_scoped(scope, signal)
                        );
                    let did_change = !already_scoped;
                    if did_change {
                        staged_signal_scope_membership.insert(pair);
                        staged_signal_scope_additions.push(pair);
                    }
                    changed.push(did_change);
                }
                SemanticMutation::AddMember { family, member } => {
                    changed.push(family_edges.add(&catalog, *family, *member, index)?);
                }
                SemanticMutation::RemoveMember { family, member } => {
                    changed.push(family_edges.remove(&catalog, *family, *member, index)?);
                }
                SemanticMutation::ReorderMember {
                    family,
                    member,
                    before,
                } => {
                    changed.push(family_edges.reorder(&catalog, *family, *member, *before, index)?);
                }
                SemanticMutation::AddNode { token, creation } => {
                    preflight_add_node(
                        store,
                        creation,
                        &removed_nodes,
                        &mut pending_sources,
                        !removed_pending.contains(token),
                        index,
                    )?;
                    if let SemanticNodeCreation::Object { state, .. } = creation {
                        staged_objects
                            .entry((*token).into())
                            .or_insert_with(|| (**state).clone());
                    }
                    changed.push(!removed_pending.contains(token));
                }
                SemanticMutation::AddAnimation { token, animation } => {
                    preflight_transaction_animation(
                        &catalog,
                        *token,
                        animation,
                        &mut available_pending_animations,
                        &mut staged_objects,
                        &mut staged_object_order,
                        index,
                    )?;
                    changed.push(!removed_pending.contains(token));
                }
                SemanticMutation::RemoveAnimation { .. } => {
                    changed.push(true);
                }
                SemanticMutation::RemoveNode { node } => match node {
                    SemanticTransactionNodeRef::Existing(node) => {
                        if store.node(*node).is_none() {
                            return Err(SemanticMutationTransactionError::Node {
                                index,
                                error: SemanticStoreError::UnknownNode(*node),
                            });
                        }
                        changed.push(true);
                    }
                    SemanticTransactionNodeRef::Pending(_) => changed.push(false),
                },
            }
        }

        for (mutation, changed) in self.mutations.iter().zip(&mut changed) {
            if mutation.references_any_pending(&removed_pending) {
                *changed = false;
            }
        }
        staged_objects.retain(|node, _| {
            !matches!(node, SemanticTransactionNodeRef::Pending(token) if removed_pending.contains(token))
        });
        Ok(SemanticTransactionPreflight {
            changed,
            staged_objects,
            staged_object_order,
            family_edges,
            staged_signal_scope_additions,
            pending_creations,
            pending_animations,
            removed_existing: removed_nodes,
            removed_pending,
        })
    }
}

fn object_property_value(
    state: &SemanticObjectState,
    property: SemanticObjectProperty,
) -> SemanticSignalValue {
    match property {
        SemanticObjectProperty::Presence => {
            unreachable!("presence is currently authored only through a typed signal binding")
        }
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

fn validate_updater_boundary(
    index: usize,
    target: SemanticTransactionNodeRef,
    time: f64,
) -> Result<(), SemanticMutationTransactionError> {
    if !time.is_finite() || time < 0.0 {
        return Err(invalid_updater_interval(index, target));
    }
    Ok(())
}

fn invalid_updater_interval(
    index: usize,
    target: SemanticTransactionNodeRef,
) -> SemanticMutationTransactionError {
    SemanticMutationTransactionError::InvalidUpdaterActivation { index, target }
}

fn updater_edit_error(
    index: usize,
    target: SemanticTransactionNodeRef,
    error: UpdaterRegistrationEditError,
) -> SemanticMutationTransactionError {
    match error {
        UpdaterRegistrationEditError::InvalidActivationInterval => {
            invalid_updater_interval(index, target)
        }
        UpdaterRegistrationEditError::PositionOutOfBounds { position, active } => {
            SemanticMutationTransactionError::UpdaterPositionOutOfBounds {
                index,
                target,
                position,
                active,
            }
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

    apply_object_property(state, property, value);
}

fn apply_object_property(
    state: &mut SemanticObjectState,
    property: SemanticObjectProperty,
    value: SemanticSignalValue,
) {
    match (property, value) {
        (SemanticObjectProperty::Presence, _) => {
            unreachable!("presence property writes are rejected during transaction preflight")
        }
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

pub(super) fn validate_object_content_resource(
    store: &SemanticStore,
    content: SemanticObjectContent,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
    match content {
        SemanticObjectContent::Geometry(geometry) => {
            if !geometry.is_finite() {
                return Err(SemanticMutationTransactionError::InvalidObjectContent { index });
            }
            if let StoredGeometry::Resource(resource) = geometry {
                if store.geometry_resources().get(resource).is_none() {
                    return Err(SemanticMutationTransactionError::InvalidGeometryResource {
                        index,
                        resource,
                    });
                }
            }
        }
        SemanticObjectContent::Text(resource) => {
            if store.text_resources().get(resource).is_none() {
                return Err(SemanticMutationTransactionError::InvalidTextResource {
                    index,
                    resource,
                });
            }
        }
    }
    Ok(())
}

fn set_object_style(store: &mut SemanticStore, object: SemanticNodeId, style: SemanticStyle) {
    store
        .node_mut(object)
        .and_then(|node| node.semantic_object_state_mut())
        .expect("preflighted semantic object must remain valid while transaction owns the store")
        .style = style;
}

const fn is_style_property(property: SemanticObjectProperty) -> bool {
    matches!(
        property,
        SemanticObjectProperty::FillOpacity
            | SemanticObjectProperty::StrokeOpacity
            | SemanticObjectProperty::StrokeWidth
            | SemanticObjectProperty::ObjectOpacity
    )
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
    committed_nodes: HashMap<SemanticLocalNodeToken, SemanticNodeId>,
}

impl SemanticMutationTransactionResult {
    pub fn impacts(&self) -> &[SemanticMutationImpact] {
        &self.impacts
    }

    /// Resolve one token from this committed transaction to its real semantic ID.
    /// Canceled or foreign tokens return `None`.
    pub fn resolve(&self, token: SemanticLocalNodeToken) -> Option<SemanticNodeId> {
        self.committed_nodes.get(&token).copied()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticMutationTransactionError {
    SceneRevisionExhausted,
    InsertionOrderExhausted,
    PendingNodeFromDifferentTransaction {
        index: usize,
        token: SemanticLocalNodeToken,
    },
    UnknownPendingNode {
        index: usize,
        token: SemanticLocalNodeToken,
    },
    PendingNodeKindMismatch {
        index: usize,
        token: SemanticLocalNodeToken,
        expected: SemanticPendingNodeKind,
    },
    PendingAnimationForwardReference {
        index: usize,
        animation: SemanticLocalNodeToken,
    },
    DuplicatePendingMutation {
        index: usize,
        node: SemanticLocalNodeToken,
    },
    ConflictingPendingStyleMutation {
        index: usize,
        object: SemanticLocalNodeToken,
    },
    PendingFamilyCycle {
        index: usize,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    PendingNotFamilyMember {
        index: usize,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    PendingSubscriptionUsesRemovedSignal {
        index: usize,
        object: SemanticLocalNodeToken,
        property: SemanticObjectProperty,
        signal: SemanticNodeId,
    },
    PendingFamilyEdgeUsesRemovedNode {
        index: usize,
        family: SemanticTransactionNodeRef,
        member: SemanticNodeId,
    },
    PendingFamilyOrderUsesRemovedNode {
        index: usize,
        family: SemanticTransactionNodeRef,
        node: SemanticTransactionNodeRef,
    },
    PendingNonFinitePropertyValue {
        index: usize,
        object: SemanticLocalNodeToken,
        property: SemanticObjectProperty,
    },
    PendingPropertyTypeMismatch {
        index: usize,
        object: SemanticLocalNodeToken,
        property: SemanticObjectProperty,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    UnsupportedPropertyWrite {
        index: usize,
        object: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
    },
    InvalidPendingStyle {
        index: usize,
        object: SemanticLocalNodeToken,
    },
    PendingSubscriptionTypeMismatch {
        index: usize,
        object: SemanticLocalNodeToken,
        property: SemanticObjectProperty,
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    SamePendingAnimationTargetAndTargetState {
        index: usize,
        node: SemanticTransactionNodeRef,
    },
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
    DuplicateStyle {
        index: usize,
        object: SemanticNodeId,
    },
    ConflictingStyleMutation {
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
    DuplicateFamilyOrder {
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
    FamilyOrderUsesRemovedNode {
        index: usize,
        family: SemanticNodeId,
        node: SemanticNodeId,
    },
    NodeCreationUsesRemovedNode {
        index: usize,
        node: SemanticNodeId,
    },
    SignalScopeUsesRemovedNode {
        index: usize,
        scope: SemanticTransactionNodeRef,
        signal: SemanticTransactionNodeRef,
    },
    AnimationUsesRemovedNode {
        index: usize,
        node: SemanticNodeId,
    },
    InvalidNodeObjectState {
        index: usize,
    },
    InvalidObjectContent {
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
    SignalTrack {
        index: usize,
        error: SemanticScalarSignalTrackError,
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
    InvalidStyle {
        index: usize,
        object: SemanticNodeId,
    },
    InvalidGeometryResource {
        index: usize,
        resource: crate::GeometryResourceHandle,
    },
    InvalidTextResource {
        index: usize,
        resource: crate::TextResourceHandle,
    },
    InvalidUpdaterActivation {
        index: usize,
        target: SemanticTransactionNodeRef,
    },
    UpdaterPositionOutOfBounds {
        index: usize,
        target: SemanticTransactionNodeRef,
        position: usize,
        active: usize,
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
            Self::SceneRevisionExhausted => write!(formatter, "Noon scene revision space exhausted"),
            Self::InsertionOrderExhausted => {
                write!(formatter, "Noon semantic insertion-order space exhausted")
            }
            Self::SignalTrack { index, error } => write!(
                formatter,
                "semantic transaction mutation {index} has invalid signal track: {error}"
            ),
            Self::PendingNodeFromDifferentTransaction { index, token } => write!(
                formatter,
                "semantic transaction mutation {index} uses pending node {token:?} from another transaction"
            ),
            Self::UnknownPendingNode { index, token } => write!(
                formatter,
                "semantic transaction mutation {index} uses unknown pending node {token:?}"
            ),
            Self::PendingNodeKindMismatch { index, token, expected } => write!(
                formatter,
                "semantic transaction mutation {index} requires pending node {token:?} to be {expected:?}"
            ),
            Self::PendingAnimationForwardReference { index, animation } => write!(
                formatter,
                "semantic transaction mutation {index} references pending animation {animation:?} before its declaration"
            ),
            Self::DuplicatePendingMutation { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} repeats a mutation target involving pending node {node:?}"
            ),
            Self::ConflictingPendingStyleMutation { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} mixes full and scalar style mutation on pending object {object:?}"
            ),
            Self::PendingFamilyCycle { index, family, member } => write!(
                formatter,
                "semantic transaction mutation {index} creates a family cycle {family:?} -> {member:?}"
            ),
            Self::PendingNotFamilyMember { index, family, member } => write!(
                formatter,
                "semantic transaction mutation {index} cannot reorder non-member {member:?} in family {family:?}"
            ),
            Self::PendingSubscriptionUsesRemovedSignal { index, object, property, signal } => write!(
                formatter,
                "semantic transaction mutation {index} cannot bind removed signal {signal:?} to {property:?} on pending object {object:?}"
            ),
            Self::PendingFamilyEdgeUsesRemovedNode { index, family, member } => write!(
                formatter,
                "semantic transaction mutation {index} cannot use removed node {member:?} in pending family edge for {family:?}"
            ),
            Self::PendingFamilyOrderUsesRemovedNode { index, family, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot use removed node {node:?} in pending family order for {family:?}"
            ),
            Self::PendingNonFinitePropertyValue { index, object, property } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set {property:?} on pending object {object:?} to a non-finite value"
            ),
            Self::PendingPropertyTypeMismatch { index, object, property, expected, actual } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set {property:?} on pending object {object:?} requiring {expected} to {actual}"
            ),
            Self::UnsupportedPropertyWrite {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot directly set {property:?} on {object:?}; this property currently requires a typed signal binding"
            ),
            Self::InvalidPendingStyle { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set a non-finite style on pending object {object:?}"
            ),
            Self::PendingSubscriptionTypeMismatch { index, object, property, signal, expected, actual } => write!(
                formatter,
                "semantic transaction mutation {index} cannot bind {actual} signal {signal:?} to {property:?} on pending object {object:?} requiring {expected}"
            ),
            Self::SamePendingAnimationTargetAndTargetState { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an animation with identical pending target and target state {node:?}"
            ),
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
            Self::DuplicateStyle { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} repeats style replacement on object {}:{}",
                object.slot(),
                object.generation()
            ),
            Self::ConflictingStyleMutation { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} mixes full-style replacement with scalar style mutation on object {}:{}",
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
            Self::NodeCreationUsesRemovedNode { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add a node referencing semantic node {}:{} because that node is removed by the same transaction",
                node.slot(),
                node.generation()
            ),
            Self::SignalScopeUsesRemovedNode {
                index,
                scope,
                signal,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot scope removed signal {signal:?} under {scope:?}"
            ),
            Self::AnimationUsesRemovedNode { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an animation referencing node {}:{} because that node is removed by the same transaction",
                node.slot(),
                node.generation()
            ),
            Self::InvalidObjectContent { index } => write!(formatter, "semantic transaction mutation {index}: object geometry contains non-finite values"),
            Self::InvalidNodeObjectState { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an object with non-finite authored transform/style values"
            ),
            Self::InvalidUpdaterActivation { index, target } => write!(
                formatter,
                "semantic transaction mutation {index} has an invalid updater activation interval for {target:?}"
            ),
            Self::UpdaterPositionOutOfBounds {
                index,
                target,
                position,
                active,
            } => write!(
                formatter,
                "semantic transaction mutation {index} inserts updater at position {position} on {target:?}, but only {active} registrations are active"
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
            Self::InvalidStyle { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot replace style on object {}:{} with non-finite authored values",
                object.slot(),
                object.generation()
            ),
            Self::InvalidGeometryResource { index, resource } => write!(
                formatter,
                "semantic transaction mutation {index} references unavailable geometry resource {:?}",
                resource
            ),
            Self::InvalidTextResource { index, resource } => write!(
                formatter,
                "semantic transaction mutation {index} references unavailable text resource {:?}",
                resource
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
mod style_tests;

#[cfg(test)]
mod subscription_tests;

#[cfg(test)]
mod family_edge_tests;

#[cfg(test)]
mod reorder_member_tests;

#[cfg(test)]
mod add_node_tests;

#[cfg(test)]
mod add_animation_tests;

#[cfg(test)]
mod remove_animation_tests;

#[cfg(test)]
mod remove_node_tests;

#[cfg(test)]
mod prepared_tests;

#[cfg(test)]
mod provisional_tests;

#[cfg(test)]
mod signal_scope_tests;
