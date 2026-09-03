use std::collections::HashSet;

use crate::{
    SemanticNodeId, SemanticObjectState, SemanticStore, SemanticStoreError, SourceIdentity,
};

use super::SemanticMutationTransactionError;

/// Authored payload for allocating one new scene node through the semantic
/// mutation transaction.
///
/// Nodes created here start detached. Animation declarations keep their dedicated
/// `AddAnimation` mutation, while signal declarations keep the typed signal
/// authoring APIs; this creation payload owns only scene object/family identity.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticNodeCreation {
    Object {
        state: Box<SemanticObjectState>,
        source_identity: Option<SourceIdentity>,
    },
    Family {
        source_identity: Option<SourceIdentity>,
    },
}

impl SemanticNodeCreation {
    pub fn object(state: SemanticObjectState) -> Self {
        Self::Object {
            state: Box::new(state),
            source_identity: None,
        }
    }

    pub const fn family() -> Self {
        Self::Family {
            source_identity: None,
        }
    }

    pub fn with_source_identity(mut self, source_identity: SourceIdentity) -> Self {
        match &mut self {
            Self::Object {
                source_identity: source,
                ..
            }
            | Self::Family {
                source_identity: source,
            } => *source = Some(source_identity),
        }
        self
    }

    pub const fn source_identity(&self) -> Option<&SourceIdentity> {
        match self {
            Self::Object {
                source_identity, ..
            }
            | Self::Family { source_identity } => source_identity.as_ref(),
        }
    }
}

pub(super) fn preflight_add_node(
    store: &SemanticStore,
    creation: &SemanticNodeCreation,
    removed_nodes: &HashSet<SemanticNodeId>,
    pending_sources: &mut HashSet<SourceIdentity>,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
    if let Some(source) = creation.source_identity() {
        if let Some(existing) = store.node_for_source(source) {
            if !removed_nodes.contains(&existing) {
                return Err(SemanticMutationTransactionError::Node {
                    index,
                    error: SemanticStoreError::DuplicateSourceIdentity(source.clone()),
                });
            }
        }
        if !pending_sources.insert(source.clone()) {
            return Err(SemanticMutationTransactionError::Node {
                index,
                error: SemanticStoreError::DuplicateSourceIdentity(source.clone()),
            });
        }
    }

    let SemanticNodeCreation::Object { state, .. } = creation else {
        return Ok(());
    };

    if !state.transform.translation.is_finite()
        || !state.transform.scale.is_finite()
        || !state.transform.rotation_z.is_finite()
        || !state.style.fill_opacity.is_finite()
        || !state.style.stroke_opacity.is_finite()
        || !state.style.stroke_width.is_finite()
        || !state.style.object_opacity.is_finite()
    {
        return Err(SemanticMutationTransactionError::InvalidNodeObjectState { index });
    }

    for binding in state.signal_bindings() {
        let signal = binding.signal();
        if removed_nodes.contains(&signal) {
            return Err(
                SemanticMutationTransactionError::NodeCreationUsesRemovedNode {
                    index,
                    node: signal,
                },
            );
        }
        let actual = store
            .semantic_signal_value_kind(signal)
            .map_err(|error| SemanticMutationTransactionError::Signal { index, error })?;
        let expected = binding.property().value_kind();
        if actual != expected {
            return Err(
                SemanticMutationTransactionError::NodeCreationBindingTypeMismatch {
                    index,
                    signal,
                    expected,
                    actual,
                },
            );
        }
    }

    Ok(())
}

pub(super) fn commit_add_node(
    store: &mut SemanticStore,
    creation: SemanticNodeCreation,
) -> (SemanticNodeId, Option<SourceIdentity>) {
    match creation {
        SemanticNodeCreation::Object {
            state,
            source_identity,
        } => (store.insert_semantic_object(*state), source_identity),
        SemanticNodeCreation::Family { source_identity } => {
            (store.insert_family(), source_identity)
        }
    }
}
