use noon_compile::{
    prepare_semantic_publication, validate_semantic_publication, ExecutionMutationTransaction,
    ExecutionPatch, SemanticPublicationLoweringError, SemanticPublicationPreparationStats,
};
use noon_core::{
    PublicationContext, SceneRevision, SemanticMutationTransaction,
    SemanticMutationTransactionError, SemanticMutationTransactionResult, SemanticNodeId,
    SemanticStore,
};
use noon_runtime::{
    apply_execution_slot_membership_changes, preflight_execution_slot_membership_shape,
    AuthoredPublicationError, ExecutionSlotError, FrameObjectState,
};

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
    ExecutionSlot(ExecutionSlotError),
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
            Self::ExecutionSlot(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ExecutionSessionPublicationError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StructuralPublicationStats {
    pub preparation: SemanticPublicationPreparationStats,
    pub entered_objects: usize,
    pub exited_objects: usize,
}

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
    /// Structural publication admits append-compatible geometry entries, local
    /// family exits, and content already owned by this store before session bootstrap.
    /// Aliases are reduced to exact net membership after semantic commit. Resource
    /// allocation, reactive membership, and painter-order interleaving remain explicit
    /// unsupported cases.
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
        let publication = prepare_semantic_publication(
            &prepared,
            &self.execution_index,
            &self.reachability,
            self.painter_order.tail(),
        )
        .map_err(ExecutionSessionPublicationError::Lowering)?;
        let preparation_stats = publication.stats();
        let mut conservative_patches = publication.value_transaction().mutations().to_vec();
        conservative_patches.extend(
            publication
                .possible_exits()
                .iter()
                .copied()
                .map(ExecutionPatch::RemoveObject),
        );
        let conservative = ExecutionMutationTransaction::from_mutations(conservative_patches);
        let structural_change_possible =
            publication.possible_entry_count() != 0 || !publication.possible_exits().is_empty();
        self.runtime
            .preflight_authored_transaction_shape_with_resources(
                &conservative,
                publication.resource_additions(),
                self.publication_context(),
                prepared.proposed_scene_revision(),
                publication.possible_entry_count(),
                structural_change_possible,
            )
            .map_err(ExecutionSessionPublicationError::Runtime)?;
        preflight_execution_slot_membership_shape(
            &self.slots,
            publication.possible_exits(),
            publication.possible_entry_count(),
        )
        .map_err(ExecutionSessionPublicationError::ExecutionSlot)?;

        let result = prepared.commit();
        let membership = self
            .reachability
            .apply_transaction_result(store, &result)
            .expect("prepared publication validated every possible reachable object");
        let entered = membership.entered_execution_objects().collect::<Vec<_>>();
        let exited = membership.exited_execution_objects().collect::<Vec<_>>();
        let execution = publication.bind(&result, &membership);
        let (execution, resource_additions) = execution.into_parts();
        self.runtime
            .apply_authored_execution_transaction(
                &execution,
                resource_additions,
                self.publication_context(),
                store.scene_revision(),
            )
            .expect("runtime publication was fully preflighted before semantic commit");
        apply_execution_slot_membership_changes(&mut self.slots, &exited, &entered)
            .expect("exact membership is a subset of the preflighted structural shape");
        self.execution_index
            .apply_transaction_result(store, &result);
        self.execution_index.apply_reachability_update(&membership);
        for node in membership.exited_objects() {
            self.painter_order.remove(*node);
        }
        for node in membership.entered_objects() {
            let state = store
                .semantic_object_state_checked(*node)
                .expect("entered semantic object remains live after commit");
            self.painter_order
                .insert(*node, state.presentation().order_key());
        }
        self.last_structural_publication = StructuralPublicationStats {
            preparation: preparation_stats,
            entered_objects: entered.len(),
            exited_objects: exited.len(),
        };
        Ok(result)
    }

    pub const fn last_structural_publication_stats(&self) -> StructuralPublicationStats {
        self.last_structural_publication
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
