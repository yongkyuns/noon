use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

static NEXT_SEMANTIC_TRANSACTION_ID: AtomicU32 = AtomicU32::new(1);

pub(super) fn next_transaction_id() -> u32 {
    NEXT_SEMANTIC_TRANSACTION_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("Noon semantic transaction token space exhausted")
}

/// A transaction-scoped name for a semantic node that will be allocated at commit.
///
/// This token is not a semantic identity. Its transaction provenance prevents a
/// token from accidentally naming an equally-positioned creation in another batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticLocalNodeToken {
    transaction: u32,
    ordinal: u32,
}

impl SemanticLocalNodeToken {
    pub(super) const fn new(transaction: u32, ordinal: u32) -> Self {
        Self {
            transaction,
            ordinal,
        }
    }

    pub(super) const fn belongs_to(self, transaction: u32) -> bool {
        self.transaction == transaction
    }
}

/// A node reference accepted by staged semantic mutations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticTransactionNodeRef {
    Existing(SemanticNodeId),
    Pending(SemanticLocalNodeToken),
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
    fn from(node: SemanticNodeId) -> Self {
        Self::Existing(node)
    }
}

impl From<SemanticLocalNodeToken> for SemanticTransactionNodeRef {
    fn from(token: SemanticLocalNodeToken) -> Self {
        Self::Pending(token)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticPendingNodeKind {
    Object,
    Family,
    AuthoringNode,
    Animation,
    Signal,
}

#[derive(Clone, Copy)]
enum PendingSemanticNode<'a> {
    Creation(&'a SemanticNodeCreation),
    Animation(&'a SemanticTransactionAnimation),
}

pub(super) struct TransactionNodeCatalog<'a> {
    transaction_id: u32,
    store: &'a SemanticStore,
    pending: HashMap<SemanticLocalNodeToken, PendingSemanticNode<'a>>,
}

impl<'a> TransactionNodeCatalog<'a> {
    pub(super) fn ensure_scalar_animation_target(
        &self,
        signal: SemanticNodeId,
        target: f64,
        index: usize,
    ) -> Result<(), SemanticMutationTransactionError> {
        self.store
            .validate_semantic_scalar_animation_target(signal, target)
            .map_err(|error| SemanticMutationTransactionError::SignalTrack { index, error })
    }

    pub(super) fn new(
        transaction: &'a SemanticMutationTransaction,
        store: &'a SemanticStore,
    ) -> Self {
        let pending = transaction
            .mutations
            .iter()
            .filter_map(|mutation| match mutation {
                SemanticMutation::AddNode { token, creation } => {
                    Some((*token, PendingSemanticNode::Creation(creation)))
                }
                SemanticMutation::AddAnimation { token, animation } => {
                    Some((*token, PendingSemanticNode::Animation(animation)))
                }
                _ => None,
            })
            .collect();
        Self {
            transaction_id: transaction.id,
            store,
            pending,
        }
    }

    pub(super) fn validate_pending(
        &self,
        node: SemanticTransactionNodeRef,
        index: usize,
    ) -> Result<(), SemanticMutationTransactionError> {
        let SemanticTransactionNodeRef::Pending(token) = node else {
            return Ok(());
        };
        if !token.belongs_to(self.transaction_id) {
            return Err(
                SemanticMutationTransactionError::PendingNodeFromDifferentTransaction {
                    index,
                    token,
                },
            );
        }
        if !self.pending.contains_key(&token) {
            return Err(SemanticMutationTransactionError::UnknownPendingNode { index, token });
        }
        Ok(())
    }

    pub(super) fn cloned_creations(&self) -> HashMap<SemanticLocalNodeToken, SemanticNodeCreation> {
        self.pending
            .iter()
            .filter_map(|(token, pending)| match pending {
                PendingSemanticNode::Creation(creation) => Some((*token, (*creation).clone())),
                PendingSemanticNode::Animation(_) => None,
            })
            .collect()
    }

    pub(super) fn cloned_animations(
        &self,
    ) -> HashMap<SemanticLocalNodeToken, SemanticTransactionAnimation> {
        self.pending
            .iter()
            .filter_map(|(token, pending)| match pending {
                PendingSemanticNode::Animation(animation) => Some((*token, (*animation).clone())),
                PendingSemanticNode::Creation(_) => None,
            })
            .collect()
    }

    pub(super) fn ensure_family(
        &self,
        node: SemanticTransactionNodeRef,
        index: usize,
    ) -> Result<(), SemanticMutationTransactionError> {
        match node {
            SemanticTransactionNodeRef::Existing(node) => {
                let Some(existing) = self.store.node(node) else {
                    return Err(SemanticMutationTransactionError::Family {
                        index,
                        error: SemanticSceneOperationError::UnknownNode(node),
                    });
                };
                if !matches!(existing.kind(), SemanticNodeKind::Family) {
                    return Err(SemanticMutationTransactionError::Family {
                        index,
                        error: SemanticSceneOperationError::NotSemanticFamily(node),
                    });
                }
            }
            SemanticTransactionNodeRef::Pending(token) => {
                self.validate_pending(node, index)?;
                if !matches!(
                    self.pending[&token],
                    PendingSemanticNode::Creation(SemanticNodeCreation::Family { .. })
                ) {
                    return Err(SemanticMutationTransactionError::PendingNodeKindMismatch {
                        index,
                        token,
                        expected: SemanticPendingNodeKind::Family,
                    });
                }
            }
        }
        Ok(())
    }

    pub(super) fn ensure_authoring_node(
        &self,
        node: SemanticTransactionNodeRef,
        index: usize,
    ) -> Result<(), SemanticMutationTransactionError> {
        match node {
            SemanticTransactionNodeRef::Existing(node) => {
                let Some(existing) = self.store.node(node) else {
                    return Err(SemanticMutationTransactionError::Family {
                        index,
                        error: SemanticSceneOperationError::UnknownNode(node),
                    });
                };
                let valid = matches!(existing.kind(), SemanticNodeKind::Family)
                    || matches!(existing.kind(), SemanticNodeKind::AuthoringObject)
                        && existing.semantic_object_state().is_some();
                if !valid {
                    return Err(SemanticMutationTransactionError::Family {
                        index,
                        error: SemanticSceneOperationError::NotSemanticAuthoringNode(node),
                    });
                }
            }
            SemanticTransactionNodeRef::Pending(token) => {
                self.validate_pending(node, index)?;
                if !matches!(
                    self.pending[&token],
                    PendingSemanticNode::Creation(
                        SemanticNodeCreation::Object { .. } | SemanticNodeCreation::Family { .. }
                    )
                ) {
                    return Err(SemanticMutationTransactionError::PendingNodeKindMismatch {
                        index,
                        token,
                        expected: SemanticPendingNodeKind::AuthoringNode,
                    });
                }
            }
        }
        Ok(())
    }

    pub(super) fn ensure_animation(
        &self,
        node: SemanticTransactionNodeRef,
        index: usize,
    ) -> Result<(), SemanticMutationTransactionError> {
        match node {
            SemanticTransactionNodeRef::Existing(node) => {
                let Some(existing) = self.store.node(node) else {
                    return Err(SemanticMutationTransactionError::UnknownAnimation {
                        index,
                        animation: node,
                    });
                };
                if !matches!(existing.kind(), SemanticNodeKind::Animation(_)) {
                    return Err(SemanticMutationTransactionError::NotAnimation {
                        index,
                        animation: node,
                    });
                }
            }
            SemanticTransactionNodeRef::Pending(token) => {
                self.validate_pending(node, index)?;
                if !matches!(self.pending[&token], PendingSemanticNode::Animation(_)) {
                    return Err(SemanticMutationTransactionError::PendingNodeKindMismatch {
                        index,
                        token,
                        expected: SemanticPendingNodeKind::Animation,
                    });
                }
            }
        }
        Ok(())
    }

    pub(super) fn ensure_signal(
        &self,
        node: SemanticTransactionNodeRef,
        index: usize,
    ) -> Result<(), SemanticMutationTransactionError> {
        match node {
            SemanticTransactionNodeRef::Existing(node) => {
                self.store
                    .semantic_signal_state(node)
                    .map_err(|error| SemanticMutationTransactionError::Signal { index, error })?;
            }
            SemanticTransactionNodeRef::Pending(token) => {
                self.validate_pending(node, index)?;
                if !matches!(
                    self.pending[&token],
                    PendingSemanticNode::Creation(SemanticNodeCreation::Signal { .. })
                ) {
                    return Err(SemanticMutationTransactionError::PendingNodeKindMismatch {
                        index,
                        token,
                        expected: SemanticPendingNodeKind::Signal,
                    });
                }
            }
        }
        Ok(())
    }

    pub(super) fn ensure_animation_target(
        &self,
        node: SemanticTransactionNodeRef,
        index: usize,
    ) -> Result<(), SemanticMutationTransactionError> {
        match node {
            SemanticTransactionNodeRef::Existing(node) => self
                .store
                .semantic_object_state_checked(node)
                .map(|_| ())
                .map_err(|error| SemanticMutationTransactionError::AnimationTarget {
                    index,
                    error,
                }),
            SemanticTransactionNodeRef::Pending(token) => {
                self.validate_pending(node, index)?;
                if matches!(
                    self.pending[&token],
                    PendingSemanticNode::Creation(SemanticNodeCreation::Object { .. })
                ) {
                    Ok(())
                } else {
                    Err(SemanticMutationTransactionError::PendingNodeKindMismatch {
                        index,
                        token,
                        expected: SemanticPendingNodeKind::Object,
                    })
                }
            }
        }
    }

    pub(super) fn members(
        &self,
        family: SemanticTransactionNodeRef,
    ) -> Vec<SemanticTransactionNodeRef> {
        match family {
            SemanticTransactionNodeRef::Existing(family) => self
                .store
                .node(family)
                .map(|node| node.members().into_iter().map(Into::into).collect())
                .unwrap_or_default(),
            SemanticTransactionNodeRef::Pending(_) => Vec::new(),
        }
    }

    pub(super) fn contains(
        &self,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> bool {
        match (family, member) {
            (
                SemanticTransactionNodeRef::Existing(family),
                SemanticTransactionNodeRef::Existing(member),
            ) => self
                .store
                .node(member)
                .is_some_and(|node| node.parents().contains(&family)),
            _ => false,
        }
    }

    pub(super) fn updater_registrations(
        &self,
        target: SemanticTransactionNodeRef,
    ) -> Vec<SemanticUpdaterRegistration> {
        match target {
            SemanticTransactionNodeRef::Existing(target) => self
                .store
                .node(target)
                .map(|node| node.host_updaters().to_vec())
                .unwrap_or_default(),
            SemanticTransactionNodeRef::Pending(_) => Vec::new(),
        }
    }

    pub(super) fn staged_object_state<'b>(
        &self,
        staged: &'b mut HashMap<SemanticTransactionNodeRef, SemanticObjectState>,
        order: &mut Vec<SemanticTransactionNodeRef>,
        object: SemanticTransactionNodeRef,
        index: usize,
    ) -> Result<&'b mut SemanticObjectState, SemanticMutationTransactionError> {
        if let std::collections::hash_map::Entry::Vacant(entry) = staged.entry(object) {
            let state = match object {
                SemanticTransactionNodeRef::Existing(object) => self
                    .store
                    .semantic_object_state_checked(object)
                    .map_err(|error| SemanticMutationTransactionError::Object { index, error })?
                    .clone(),
                SemanticTransactionNodeRef::Pending(token) => match self.pending[&token] {
                    PendingSemanticNode::Creation(SemanticNodeCreation::Object {
                        state, ..
                    }) => (**state).clone(),
                    PendingSemanticNode::Creation(
                        SemanticNodeCreation::Family { .. } | SemanticNodeCreation::Signal { .. },
                    )
                    | PendingSemanticNode::Animation(_) => {
                        return Err(SemanticMutationTransactionError::PendingNodeKindMismatch {
                            index,
                            token,
                            expected: SemanticPendingNodeKind::Object,
                        });
                    }
                },
            };
            entry.insert(state);
            order.push(object);
        }
        Ok(staged
            .get_mut(&object)
            .expect("staged object inserted above"))
    }
}

pub(super) fn replace_object_binding(
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
            bindings[position] = SemanticSignalBinding::new(signal, property)
        }
        (None, Some(signal)) => bindings.push(SemanticSignalBinding::new(signal, property)),
        (Some(position), None) => {
            bindings.remove(position);
        }
        (None, None) => {}
    }
}

pub(super) fn conflicting_style_error(
    index: usize,
    object: SemanticTransactionNodeRef,
) -> SemanticMutationTransactionError {
    match object {
        SemanticTransactionNodeRef::Existing(object) => {
            SemanticMutationTransactionError::ConflictingStyleMutation { index, object }
        }
        SemanticTransactionNodeRef::Pending(object) => {
            SemanticMutationTransactionError::ConflictingPendingStyleMutation { index, object }
        }
    }
}

pub(super) fn duplicate_mutation_error(
    index: usize,
    key: SemanticMutationKey,
) -> SemanticMutationTransactionError {
    let pending = match key {
        SemanticMutationKey::ObjectProperty { object, .. }
        | SemanticMutationKey::ObjectContent(object)
        | SemanticMutationKey::ObjectStyle(object)
        | SemanticMutationKey::Subscription { object, .. }
        | SemanticMutationKey::NodeRemoval(object) => match object {
            SemanticTransactionNodeRef::Pending(token) => Some(token),
            SemanticTransactionNodeRef::Existing(_) => None,
        },
        SemanticMutationKey::FamilyEdge { family, member }
        | SemanticMutationKey::FamilyOrder { family, member } => {
            [family, member].into_iter().find_map(|node| match node {
                SemanticTransactionNodeRef::Pending(token) => Some(token),
                SemanticTransactionNodeRef::Existing(_) => None,
            })
        }
        SemanticMutationKey::Signal(_) => None,
    };
    if let Some(node) = pending {
        return SemanticMutationTransactionError::DuplicatePendingMutation { index, node };
    }
    match key {
        SemanticMutationKey::Signal(target) => {
            SemanticMutationTransactionError::DuplicateTarget { index, target }
        }
        SemanticMutationKey::ObjectProperty {
            object: SemanticTransactionNodeRef::Existing(object),
            property,
        } => SemanticMutationTransactionError::DuplicateProperty {
            index,
            object,
            property,
        },
        SemanticMutationKey::ObjectContent(SemanticTransactionNodeRef::Existing(object)) => {
            SemanticMutationTransactionError::DuplicateContent { index, object }
        }
        SemanticMutationKey::ObjectStyle(SemanticTransactionNodeRef::Existing(object)) => {
            SemanticMutationTransactionError::DuplicateStyle { index, object }
        }
        SemanticMutationKey::Subscription {
            object: SemanticTransactionNodeRef::Existing(object),
            property,
        } => SemanticMutationTransactionError::DuplicateSubscription {
            index,
            object,
            property,
        },
        SemanticMutationKey::FamilyEdge {
            family: SemanticTransactionNodeRef::Existing(family),
            member: SemanticTransactionNodeRef::Existing(member),
        } => SemanticMutationTransactionError::DuplicateFamilyEdge {
            index,
            family,
            member,
        },
        SemanticMutationKey::FamilyOrder {
            family: SemanticTransactionNodeRef::Existing(family),
            member: SemanticTransactionNodeRef::Existing(member),
        } => SemanticMutationTransactionError::DuplicateFamilyOrder {
            index,
            family,
            member,
        },
        SemanticMutationKey::NodeRemoval(SemanticTransactionNodeRef::Existing(node)) => {
            SemanticMutationTransactionError::DuplicateNodeRemoval { index, node }
        }
        _ => unreachable!("pending duplicate returned above"),
    }
}

pub(super) fn non_finite_property_error(
    index: usize,
    object: SemanticTransactionNodeRef,
    property: SemanticObjectProperty,
) -> SemanticMutationTransactionError {
    match object {
        SemanticTransactionNodeRef::Existing(object) => {
            SemanticMutationTransactionError::NonFinitePropertyValue {
                index,
                object,
                property,
            }
        }
        SemanticTransactionNodeRef::Pending(object) => {
            SemanticMutationTransactionError::PendingNonFinitePropertyValue {
                index,
                object,
                property,
            }
        }
    }
}

pub(super) fn property_type_error(
    index: usize,
    object: SemanticTransactionNodeRef,
    property: SemanticObjectProperty,
    expected: SemanticSignalValueKind,
    actual: SemanticSignalValueKind,
) -> SemanticMutationTransactionError {
    match object {
        SemanticTransactionNodeRef::Existing(object) => {
            SemanticMutationTransactionError::PropertyTypeMismatch {
                index,
                object,
                property,
                expected,
                actual,
            }
        }
        SemanticTransactionNodeRef::Pending(object) => {
            SemanticMutationTransactionError::PendingPropertyTypeMismatch {
                index,
                object,
                property,
                expected,
                actual,
            }
        }
    }
}

pub(super) fn invalid_style_error(
    index: usize,
    object: SemanticTransactionNodeRef,
) -> SemanticMutationTransactionError {
    match object {
        SemanticTransactionNodeRef::Existing(object) => {
            SemanticMutationTransactionError::InvalidStyle { index, object }
        }
        SemanticTransactionNodeRef::Pending(object) => {
            SemanticMutationTransactionError::InvalidPendingStyle { index, object }
        }
    }
}

pub(super) fn subscription_type_error(
    index: usize,
    object: SemanticTransactionNodeRef,
    property: SemanticObjectProperty,
    signal: SemanticNodeId,
    expected: SemanticSignalValueKind,
    actual: SemanticSignalValueKind,
) -> SemanticMutationTransactionError {
    match object {
        SemanticTransactionNodeRef::Existing(object) => {
            SemanticMutationTransactionError::SubscriptionTypeMismatch {
                index,
                object,
                property,
                signal,
                expected,
                actual,
            }
        }
        SemanticTransactionNodeRef::Pending(object) => {
            SemanticMutationTransactionError::PendingSubscriptionTypeMismatch {
                index,
                object,
                property,
                signal,
                expected,
                actual,
            }
        }
    }
}
