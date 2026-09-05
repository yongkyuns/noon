use noon_compile::{
    lower_semantic_publication, validate_semantic_publication, SemanticPublicationLoweringError,
};
use noon_core::{
    PublicationContext, SceneRevision, SemanticMutationTransaction,
    SemanticMutationTransactionError, SemanticMutationTransactionResult, SemanticNodeId,
    SemanticStore,
};
use noon_runtime::{AuthoredPublicationError, FrameObjectState};

use super::ExecutionSession;

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionPublicationError {
    ForeignSemanticStore,
    StaleSceneRevision {
        expected: SceneRevision,
        actual: SceneRevision,
    },
    UnknownObject(SemanticNodeId),
    Semantic(SemanticMutationTransactionError),
    Lowering(SemanticPublicationLoweringError),
    Runtime(AuthoredPublicationError),
}

impl std::fmt::Display for ExecutionSessionPublicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignSemanticStore => {
                f.write_str("semantic store does not own this execution session")
            }
            Self::StaleSceneRevision { expected, actual } => write!(
                f,
                "semantic revision {} has not been published into execution revision context {}",
                actual.get(),
                expected.get()
            ),
            Self::UnknownObject(node) => write!(
                f,
                "semantic object {}:{} is not live in this execution session",
                node.slot(),
                node.generation()
            ),
            Self::Semantic(error) => error.fmt(f),
            Self::Lowering(error) => error.fmt(f),
            Self::Runtime(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ExecutionSessionPublicationError {}

/// A borrowed effective runtime value and the exact context that published it.
/// Active drivers can make this value differ from authored/base state.
#[derive(Clone, Copy, Debug)]
pub struct EffectiveSemanticObject<'a> {
    pub object: &'a FrameObjectState,
    pub publication: PublicationContext,
}

impl ExecutionSession {
    fn require_published_store(
        &self,
        store: &SemanticStore,
    ) -> Result<(), ExecutionSessionPublicationError> {
        if store.identity() != self.store_identity {
            return Err(ExecutionSessionPublicationError::ForeignSemanticStore);
        }
        let expected = self.publication_context().scene_revision();
        if store.scene_revision() != expected {
            return Err(ExecutionSessionPublicationError::StaleSceneRevision {
                expected,
                actual: store.scene_revision(),
            });
        }
        Ok(())
    }

    /// Commit authored values and their local execution projection together.
    ///
    /// The caller must use this entry point for mutations after initial lowering.
    /// A store modified separately is rejected, never repaired by rebuilding or
    /// overwriting the current effective frame. Preparation holds the store
    /// exclusively; all semantic/compiler/runtime failures precede publication.
    /// The final semantic commit is infallible and synchronous with runtime commit.
    ///
    /// Supported work is transform/style edits and detached node/animation
    /// declarations. Edits to detached objects are authored-only. Changed authored
    /// state publishes one scene revision and frame epoch even when no execution
    /// value changes; execution revision and renderer dirtiness remain unchanged
    /// in that case. Membership/content/reactive topology require their own
    /// incremental lowering and are rejected before either authority changes.
    pub fn apply_semantic_transaction(
        &mut self,
        store: &mut SemanticStore,
        transaction: SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        self.require_published_store(store)?;
        validate_semantic_publication(&transaction)
            .map_err(ExecutionSessionPublicationError::Lowering)?;
        let prepared = transaction
            .prepare(store)
            .map_err(ExecutionSessionPublicationError::Semantic)?;
        let execution = lower_semantic_publication(&prepared, &self.execution_index)
            .map_err(ExecutionSessionPublicationError::Lowering)?;
        self.runtime
            .apply_authored_transaction(
                &execution,
                self.publication_context(),
                prepared.proposed_scene_revision(),
            )
            .map_err(ExecutionSessionPublicationError::Runtime)?;
        Ok(prepared.commit())
    }

    /// Query a live semantic object through its originating store in indexed time.
    /// Both provenance and scene revision must match this published session.
    pub fn effective_semantic_object(
        &self,
        store: &SemanticStore,
        node: SemanticNodeId,
    ) -> Result<EffectiveSemanticObject<'_>, ExecutionSessionPublicationError> {
        self.require_published_store(store)?;
        store
            .semantic_object_state_checked(node)
            .map_err(|_| ExecutionSessionPublicationError::UnknownObject(node))?;
        let object = self
            .execution_index
            .execution_object_id(node)
            .and_then(|id| self.runtime.effective_object(id))
            .ok_or(ExecutionSessionPublicationError::UnknownObject(node))?;
        Ok(EffectiveSemanticObject {
            object,
            publication: self.publication_context(),
        })
    }
}

#[cfg(test)]
mod tests;
