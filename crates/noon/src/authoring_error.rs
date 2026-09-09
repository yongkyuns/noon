//! Structured failures for shared handle validation and scene membership.
//!
//! These errors retain the semantic cause and generational/resource identities.
//! They deliberately do not replace the live session's publication/segment errors.

use noon_core::{
    GeometryResourceHandle, SemanticMutationTransactionError, SemanticSceneOperationError,
    TextResourceHandle,
};

/// Failure of shared handle validation or an authored scene-membership operation.
///
/// Other authoring operations are being migrated separately. Inspect variants or
/// [`std::error::Error::source`], never the human-readable diagnostic, for control flow.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AuthoringError {
    /// A handle was supplied to an operation on a different semantic store.
    ForeignStore,
    /// The semantic operation rejected a node, family, or membership request.
    Semantic(SemanticSceneOperationError),
    /// Transaction preflight failed; no partial membership edit was committed.
    Transaction(SemanticMutationTransactionError),
    /// The handle's object references a missing or stale geometry resource.
    MissingGeometryResource(GeometryResourceHandle),
    /// The handle's object references a missing or stale text resource.
    MissingTextResource(TextResourceHandle),
}

impl std::fmt::Display for AuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignStore => f.write_str("membership target belongs to another scene store"),
            Self::Semantic(error) => error.fmt(f),
            Self::Transaction(error) => error.fmt(f),
            Self::MissingGeometryResource(handle) => {
                write!(f, "unknown or stale geometry resource {handle:?}")
            }
            Self::MissingTextResource(handle) => {
                write!(f, "unknown or stale text resource {handle:?}")
            }
        }
    }
}

impl std::error::Error for AuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Semantic(error) => Some(error),
            Self::Transaction(error) => Some(error),
            Self::ForeignStore
            | Self::MissingGeometryResource(_)
            | Self::MissingTextResource(_) => None,
        }
    }
}

impl From<SemanticSceneOperationError> for AuthoringError {
    fn from(error: SemanticSceneOperationError) -> Self {
        Self::Semantic(error)
    }
}

impl From<SemanticMutationTransactionError> for AuthoringError {
    fn from(error: SemanticMutationTransactionError) -> Self {
        Self::Transaction(error)
    }
}
