use std::collections::{HashMap, HashSet};

use super::*;
use crate::SceneRevision;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticTransactionReadError {
    PendingNodeFromDifferentTransaction(SemanticLocalNodeToken),
    UnknownPendingNode(SemanticLocalNodeToken),
    RemovedPendingNode(SemanticLocalNodeToken),
    RemovedExistingNode(SemanticNodeId),
    UnknownExistingNode(SemanticNodeId),
    NotObject(SemanticTransactionNodeRef),
    NotFamily(SemanticTransactionNodeRef),
    Existing(SemanticSceneOperationError),
}

impl std::fmt::Display for SemanticTransactionReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "semantic transaction staged read failed: {self:?}"
        )
    }
}

impl std::error::Error for SemanticTransactionReadError {}

/// A fully validated mutation batch holding the store exclusively until commit.
///
/// Preparing or dropping this value does not change authored state, revision, or
/// work counters. The exclusive borrow prevents invalidating its preflight proof.
///
/// ```compile_fail
/// use noon_core::{SemanticMutationTransaction, SemanticStore};
/// let mut store = SemanticStore::new();
/// let prepared = SemanticMutationTransaction::new().prepare(&mut store).unwrap();
/// let competing = SemanticMutationTransaction::new().prepare(&mut store);
/// prepared.commit();
/// ```
#[must_use = "dropping a prepared transaction discards its uncommitted mutations"]
pub struct PreparedSemanticMutationTransaction<'a> {
    store: &'a mut SemanticStore,
    transaction: SemanticMutationTransaction,
    preflight: SemanticTransactionPreflight,
    next_revision: Option<SceneRevision>,
}

impl<'a> PreparedSemanticMutationTransaction<'a> {
    pub(super) fn new(
        transaction: SemanticMutationTransaction,
        store: &'a mut SemanticStore,
    ) -> Result<Self, SemanticMutationTransactionError> {
        let preflight = transaction.preflight(store)?;
        let next_revision = if preflight.changed.iter().any(|changed| *changed) {
            Some(
                store
                    .scene_revision()
                    .checked_next()
                    .ok_or(SemanticMutationTransactionError::SceneRevisionExhausted)?,
            )
        } else {
            None
        };
        Ok(Self {
            store,
            transaction,
            preflight,
            next_revision,
        })
    }

    /// The published store, held read-only while this batch is staged.
    pub fn store(&self) -> &SemanticStore {
        self.store
    }

    /// All submitted mutations, preserving original indices and exact no-ops.
    pub fn mutations(&self) -> &[SemanticMutation] {
        self.transaction.mutations()
    }

    /// Potentially changing mutations in their validated commit order.
    ///
    /// No-ops resolved during preflight are excluded. Family reordering can still
    /// resolve to a no-op at commit without materializing sibling order here.
    pub fn candidate_mutations(&self) -> impl Iterator<Item = &SemanticMutation> {
        self.transaction
            .mutations
            .iter()
            .zip(&self.preflight.changed)
            .filter_map(|(mutation, changed)| changed.then_some(mutation))
    }

    /// Reserved candidate revision, or the current revision when no candidates exist.
    ///
    /// Commit can retain the current revision if all candidates resolve to no-ops
    /// (for example, an already-positioned family reorder). Consumers publishing
    /// before commit must restrict admission to mutation classes with exact
    /// preflight effects, as live value publication does.
    pub fn proposed_scene_revision(&self) -> SceneRevision {
        self.next_revision
            .unwrap_or_else(|| self.store.scene_revision())
    }

    /// Proposed presentation states, grouped in first changed-object order.
    ///
    /// This derived overlay applies only `SetProperty`, `ReplaceStyle`, and
    /// `ReplaceContent`. It is not a structural or subscription overlay; consumers
    /// must separately check which mutation classes they support. One pass over
    /// the batch clones only the affected object states, never the store or arena.
    pub fn object_updates(
        &self,
    ) -> impl Iterator<Item = (SemanticNodeId, SemanticObjectState)> + '_ {
        let changed_objects: HashSet<_> = self
            .candidate_mutations()
            .filter_map(|mutation| match mutation {
                SemanticMutation::SetProperty { object, .. }
                | SemanticMutation::ReplaceStyle { object, .. }
                | SemanticMutation::ReplaceContent { object, .. } => Some(*object),
                _ => None,
            })
            .collect();
        self.preflight
            .staged_object_order
            .iter()
            .filter_map(move |node| {
                let SemanticTransactionNodeRef::Existing(node_id) = node else {
                    return None;
                };
                changed_objects
                    .contains(node)
                    .then(|| (*node_id, self.preflight.staged_objects[node].clone()))
            })
    }

    /// Read the final staged authored object state without publishing the batch.
    pub fn object_state(
        &self,
        object: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<&SemanticObjectState, SemanticTransactionReadError> {
        let object = object.into();
        if let SemanticTransactionNodeRef::Existing(node) = object {
            if self.preflight.removed_existing.contains(&node) {
                return Err(SemanticTransactionReadError::RemovedExistingNode(node));
            }
        }
        if let SemanticTransactionNodeRef::Pending(token) = object {
            self.validate_read_token(token)?;
            if self.preflight.removed_pending.contains(&token) {
                return Err(SemanticTransactionReadError::RemovedPendingNode(token));
            }
        }
        if let Some(state) = self.preflight.staged_objects.get(&object) {
            return Ok(state);
        }
        match object {
            SemanticTransactionNodeRef::Existing(object) => self
                .store
                .semantic_object_state_checked(object)
                .map_err(SemanticTransactionReadError::Existing),
            SemanticTransactionNodeRef::Pending(token) => match self.pending_creation(token) {
                Some(SemanticNodeCreation::Object { state, .. }) => Ok(state),
                Some(SemanticNodeCreation::Family { .. }) => {
                    Err(SemanticTransactionReadError::NotObject(object))
                }
                None => Err(SemanticTransactionReadError::UnknownPendingNode(token)),
            },
        }
    }

    /// Clone the final staged object state with the insertion order it will receive
    /// if this transaction commits. Existing identities preserve their authored
    /// order; pending objects are numbered in allocation order without reserving a
    /// semantic identity or mutating the store.
    pub fn proposed_object_state(
        &self,
        object: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<SemanticObjectState, SemanticTransactionReadError> {
        let object = object.into();
        let mut state = self.object_state(object)?.clone();
        let SemanticTransactionNodeRef::Pending(token) = object else {
            return Ok(state);
        };
        let mut insertion_order = self.store.next_insertion_order();
        for mutation in self.transaction.mutations() {
            let SemanticMutation::AddNode {
                token: candidate,
                creation: SemanticNodeCreation::Object { .. },
            } = mutation
            else {
                continue;
            };
            if self.preflight.removed_pending.contains(candidate) {
                continue;
            }
            if *candidate == token {
                state.assign_insertion_order(insertion_order);
                return Ok(state);
            }
            insertion_order = insertion_order
                .checked_add(1)
                .expect("preflighted semantic insertion order must remain available");
        }
        Err(SemanticTransactionReadError::UnknownPendingNode(token))
    }

    pub fn node_is_removed(&self, node: impl Into<SemanticTransactionNodeRef>) -> bool {
        self.is_removed_ref(node.into())
    }

    /// Read final direct family order through the transaction-local overlay.
    pub fn family_members(
        &self,
        family: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<Vec<SemanticTransactionNodeRef>, SemanticTransactionReadError> {
        let family = family.into();
        if let SemanticTransactionNodeRef::Existing(node) = family {
            if self.preflight.removed_existing.contains(&node) {
                return Err(SemanticTransactionReadError::RemovedExistingNode(node));
            }
        }
        if let SemanticTransactionNodeRef::Pending(token) = family {
            self.validate_read_token(token)?;
            if self.preflight.removed_pending.contains(&token) {
                return Err(SemanticTransactionReadError::RemovedPendingNode(token));
            }
        }
        match family {
            SemanticTransactionNodeRef::Existing(family) => {
                let node = self
                    .store
                    .node(family)
                    .ok_or(SemanticTransactionReadError::UnknownExistingNode(family))?;
                if !matches!(node.kind(), SemanticNodeKind::Family) {
                    return Err(SemanticTransactionReadError::NotFamily(family.into()));
                }
                Ok(self
                    .preflight
                    .family_edges
                    .members_for_read(self.store, family.into())
                    .into_iter()
                    .filter(|member| !self.is_removed_ref(*member))
                    .collect())
            }
            SemanticTransactionNodeRef::Pending(token) => match self.pending_creation(token) {
                Some(SemanticNodeCreation::Family { .. }) => Ok(self
                    .preflight
                    .family_edges
                    .members_for_read(self.store, family)
                    .into_iter()
                    .filter(|member| !self.is_removed_ref(*member))
                    .collect()),
                Some(SemanticNodeCreation::Object { .. }) => {
                    Err(SemanticTransactionReadError::NotFamily(family))
                }
                None => Err(SemanticTransactionReadError::UnknownPendingNode(token)),
            },
        }
    }

    fn validate_read_token(
        &self,
        token: SemanticLocalNodeToken,
    ) -> Result<(), SemanticTransactionReadError> {
        if !token.belongs_to(self.transaction.id) {
            return Err(SemanticTransactionReadError::PendingNodeFromDifferentTransaction(token));
        }
        Ok(())
    }

    fn pending_creation(&self, token: SemanticLocalNodeToken) -> Option<&SemanticNodeCreation> {
        self.preflight.pending_creations.get(&token)
    }

    fn is_removed_ref(&self, node: SemanticTransactionNodeRef) -> bool {
        match node {
            SemanticTransactionNodeRef::Existing(node) => {
                self.preflight.removed_existing.contains(&node)
            }
            SemanticTransactionNodeRef::Pending(token) => {
                self.preflight.removed_pending.contains(&token)
            }
        }
    }

    /// Publish the validated batch exactly once, without another preflight.
    pub fn commit(self) -> SemanticMutationTransactionResult {
        let Self {
            store,
            transaction,
            preflight,
            next_revision,
        } = self;
        let mut impacts = Vec::with_capacity(transaction.mutations.len());
        let mut written_slots = HashSet::with_capacity(transaction.mutations.len());
        let mut pending_source_assignments = Vec::new();
        let mut committed_nodes = HashMap::new();
        for mutation in &transaction.mutations {
            let SemanticMutation::AddNode { token, creation } = mutation else {
                continue;
            };
            if preflight.removed_pending.contains(token) {
                continue;
            }
            let (node, source_identity) = commit_add_node(store, creation.clone());
            committed_nodes.insert(*token, node);
            written_slots.insert(node);
            if let Some(source_identity) = source_identity {
                pending_source_assignments.push((node, source_identity));
            }
        }
        for (mutation, changed) in transaction.mutations.into_iter().zip(preflight.changed) {
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
                    let object = resolve_node_ref(object, &committed_nodes);
                    set_object_property(store, object, property, value);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectProperty { object, property });
                }
                SemanticMutation::ReplaceContent { object, content } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    set_object_content(store, object, content);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectContent { object });
                }
                SemanticMutation::ReplaceStyle { object, style } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    set_object_style(store, object, style);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectStyle { object });
                }
                SemanticMutation::ChangeSubscription {
                    object,
                    property,
                    signal,
                } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    set_object_subscription(store, object, property, signal);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::Subscription { object, property });
                }
                SemanticMutation::AddMember { family, member } => {
                    let family = resolve_node_ref(family, &committed_nodes);
                    let member = resolve_node_ref(member, &committed_nodes);
                    store.add_member(family, member).expect(
                        "preflighted family add must remain valid while transaction owns the semantic store",
                    );
                    written_slots.insert(family);
                    written_slots.insert(member);
                    impacts.push(SemanticMutationImpact::FamilyMemberAdded { family, member });
                }
                SemanticMutation::RemoveMember { family, member } => {
                    let family = resolve_node_ref(family, &committed_nodes);
                    let member = resolve_node_ref(member, &committed_nodes);
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
                    let family = resolve_node_ref(family, &committed_nodes);
                    let member = resolve_node_ref(member, &committed_nodes);
                    let before = before.map(|node| resolve_node_ref(node, &committed_nodes));
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
                SemanticMutation::AddNode { token, .. } => {
                    let node = committed_nodes[&token];
                    impacts.push(SemanticMutationImpact::NodeAdded { node });
                }
                SemanticMutation::AddAnimation { state } => {
                    let animation = commit_add_animation(store, &state);
                    written_slots.insert(animation);
                    impacts.push(SemanticMutationImpact::AnimationAdded { animation });
                }
                SemanticMutation::AddTransformAnimation {
                    target,
                    target_state,
                    options,
                } => {
                    let target = resolve_node_ref(target, &committed_nodes);
                    let target_state = resolve_node_ref(target_state, &committed_nodes);
                    let state = SemanticAnimationState::new(
                        SemanticAnimationIntent::TransformTo {
                            target,
                            target_state,
                        },
                        options,
                    );
                    let animation = commit_add_animation(store, &state);
                    written_slots.insert(animation);
                    impacts.push(SemanticMutationImpact::AnimationAdded { animation });
                }
                SemanticMutation::RemoveAnimation { animation }
                | SemanticMutation::RemoveNode {
                    node: SemanticTransactionNodeRef::Existing(animation),
                } => {
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
                SemanticMutation::RemoveNode {
                    node: SemanticTransactionNodeRef::Pending(_),
                } => unreachable!("pending removal cancels allocation during preflight"),
            }
        }

        for (node, source_identity) in pending_source_assignments {
            store
                .set_source_identity(node, Some(source_identity))
                .expect("preflighted source identity must be available after terminal removals");
        }
        store.set_last_mutation_writes(written_slots.len());
        if !written_slots.is_empty() {
            store.publish_scene_revision(next_revision.expect("changed transaction preflighted"));
        }

        SemanticMutationTransactionResult {
            impacts,
            committed_nodes,
        }
    }
}

fn resolve_node_ref(
    node: SemanticTransactionNodeRef,
    committed: &HashMap<SemanticLocalNodeToken, SemanticNodeId>,
) -> SemanticNodeId {
    match node {
        SemanticTransactionNodeRef::Existing(node) => node,
        SemanticTransactionNodeRef::Pending(token) => committed[&token],
    }
}
