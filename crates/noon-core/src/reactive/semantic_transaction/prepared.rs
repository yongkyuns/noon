use std::collections::HashMap;

use super::*;
use crate::SceneRevision;

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
    preflight: Vec<bool>,
    next_revision: Option<SceneRevision>,
}

impl<'a> PreparedSemanticMutationTransaction<'a> {
    pub(super) fn new(
        transaction: SemanticMutationTransaction,
        store: &'a mut SemanticStore,
    ) -> Result<Self, SemanticMutationTransactionError> {
        let preflight = transaction.preflight(store)?;
        let next_revision = if preflight.iter().any(|changed| *changed) {
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
            .zip(&self.preflight)
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
    pub fn object_updates(&self) -> impl Iterator<Item = (SemanticNodeId, SemanticObjectState)> {
        let mut positions = HashMap::<SemanticNodeId, usize>::new();
        let mut updates: Vec<(SemanticNodeId, SemanticObjectState)> = Vec::new();
        for mutation in self.candidate_mutations() {
            let (SemanticMutation::SetProperty { object, .. }
            | SemanticMutation::ReplaceStyle { object, .. }
            | SemanticMutation::ReplaceContent { object, .. }) = mutation
            else {
                continue;
            };
            let position = *positions.entry(*object).or_insert_with(|| {
                let position = updates.len();
                updates.push((
                    *object,
                    self.store
                        .semantic_object_state_checked(*object)
                        .expect("preflighted object remains valid under the exclusive store borrow")
                        .clone(),
                ));
                position
            });
            let state = &mut updates[position].1;
            match mutation {
                SemanticMutation::SetProperty {
                    property, value, ..
                } => {
                    apply_object_property(state, *property, value.clone());
                }
                SemanticMutation::ReplaceStyle { style, .. } => state.style = style.clone(),
                SemanticMutation::ReplaceContent { content, .. } => state.content = *content,
                _ => unreachable!("presentation mutations selected above"),
            }
        }
        updates.into_iter()
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
        for (mutation, changed) in transaction.mutations.into_iter().zip(preflight) {
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
                SemanticMutation::ReplaceStyle { object, style } => {
                    set_object_style(store, object, style);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectStyle { object });
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
                SemanticMutation::AddNode { creation } => {
                    let (node, source_identity) = commit_add_node(store, creation);
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
        if !written_slots.is_empty() {
            store.publish_scene_revision(next_revision.expect("changed transaction preflighted"));
        }

        SemanticMutationTransactionResult { impacts }
    }
}
