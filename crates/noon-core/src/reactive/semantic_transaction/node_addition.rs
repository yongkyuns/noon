use std::collections::HashSet;

use crate::{
    SemanticNodeId, SemanticObjectState, SemanticSignalError, SemanticSignalState,
    SemanticSignalValue, SemanticStore, SemanticStoreError, SourceIdentity,
};

use super::{validate_object_content_resource, SemanticMutationTransactionError};

/// Authored payload for allocating one new scene node through the semantic
/// mutation transaction.
///
/// Nodes created here start detached. Animation declarations keep their dedicated
/// `AddAnimation` mutation. Input signals share this creation vocabulary so a
/// signal and its root scope can publish in one transaction.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticNodeCreation {
    Object {
        state: Box<SemanticObjectState>,
        source_identity: Option<SourceIdentity>,
    },
    Family {
        source_identity: Option<SourceIdentity>,
    },
    Signal {
        state: SemanticSignalState,
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

    pub fn input_signal(
        value: impl Into<SemanticSignalValue>,
    ) -> Result<Self, SemanticSignalError> {
        Ok(Self::Signal {
            state: SemanticSignalState::input(value.into())?,
        })
    }

    pub fn native_input_signal(
        value: impl Into<SemanticSignalValue>,
        source: crate::SemanticNativeInputSource,
    ) -> Result<Self, SemanticSignalError> {
        Ok(Self::Signal {
            state: SemanticSignalState::input_with_native_source(value.into(), source)?,
        })
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
            Self::Signal { .. } => {}
        }
        self
    }

    pub const fn source_identity(&self) -> Option<&SourceIdentity> {
        match self {
            Self::Object {
                source_identity, ..
            }
            | Self::Family { source_identity } => source_identity.as_ref(),
            Self::Signal { .. } => None,
        }
    }
}

pub(super) fn preflight_add_node(
    store: &SemanticStore,
    creation: &SemanticNodeCreation,
    removed_nodes: &HashSet<SemanticNodeId>,
    pending_sources: &mut HashSet<SourceIdentity>,
    validate_source_identity: bool,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
    if let Some(source) = creation
        .source_identity()
        .filter(|_| validate_source_identity)
    {
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
        || !state.style.is_finite()
    {
        return Err(SemanticMutationTransactionError::InvalidNodeObjectState { index });
    }
    validate_object_content_resource(store, state.content, index)?;

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
        SemanticNodeCreation::Signal { state } => (store.insert_semantic_signal_state(state), None),
    }
}
