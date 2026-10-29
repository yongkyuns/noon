//! An owned existing transaction across external resource preparation.
use super::*;
use noon_core::{SemanticNodeCreation, SemanticStoreIdentity, WgslEffectProgram};

/// Unpublished effect attachments or membership. This contains the original transaction and
/// immutable resource requests, never a Scene, execution copy, or borrowed proof.
#[derive(Debug, PartialEq)]
pub struct StagedEffectPublication {
    store: SemanticStoreIdentity,
    runtime: noon_runtime::RuntimeIdentity,
    root: SemanticNodeId,
    transaction: SemanticMutationTransaction,
    programs: Vec<WgslEffectProgram>,
}
impl StagedEffectPublication {
    pub fn programs(&self) -> &[WgslEffectProgram] {
        &self.programs
    }
    pub(crate) fn belongs_to(&self, session: &ExecutionSession) -> bool {
        self.store == session.store_identity && self.runtime == session.runtime_identity()
    }
}
impl ExecutionSession {
    /// Require attached hosts to prepare newly executable shader modules before publication.
    pub fn require_effect_resource_preparation(&mut self) {
        self.require_effect_preparation = true;
    }
    pub(crate) fn stage_effect_publication(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        transaction: SemanticMutationTransaction,
    ) -> Result<StagedEffectPublication, ExecutionSessionPublicationError> {
        self.require_publication_ready(SemanticPublicationPurpose::AuthoredMutation)?;
        self.require_published_store(store)?;
        if !self.reachability.is_execution_root(root) {
            return Err(ExecutionSessionPublicationError::UnknownObject(root));
        }
        for (index, mutation) in transaction.mutations().iter().enumerate() {
            if !matches!(
                mutation,
                SemanticMutation::AddNode {
                    creation: SemanticNodeCreation::Effect { .. },
                    ..
                } | SemanticMutation::AddMember { .. }
                    | SemanticMutation::RemoveMember { .. }
                    | SemanticMutation::ReorderMember { .. }
                    | SemanticMutation::ReplaceMemberContainer { .. }
                    | SemanticMutation::SetForegroundMembers { .. }
            ) {
                return Err(ExecutionSessionPublicationError::Lowering(
                    SemanticPublicationLoweringError::UnsupportedMutation { index },
                ));
            };
        }
        let prepared = transaction
            .prepare(store)
            .map_err(ExecutionSessionPublicationError::Semantic)?;
        // Validate the final graph through the ordinary compiler before device
        // work. Drop its borrowed proof; publication repeats all CPU admission
        // against the then-current store/runtime after the external await.
        let publication =
            prepare_semantic_publication(&prepared, &self.execution_index, &self.reachability)
                .map_err(ExecutionSessionPublicationError::Lowering)?;
        self.validate_shader_view_profile(&prepared, &publication)?;
        let mut programs = Vec::new();
        for program in publication.introduced_shader_programs() {
            if !programs.contains(program) {
                programs.push(program.clone());
            }
        }
        Ok(StagedEffectPublication {
            store: prepared.store().identity(),
            runtime: self.runtime_identity(),
            root,
            transaction: prepared.into_transaction(),
            programs,
        })
    }
    pub(crate) fn publish_staged_effects(
        &mut self,
        store: &mut SemanticStore,
        staged: StagedEffectPublication,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        if !staged.belongs_to(self) {
            return Err(ExecutionSessionPublicationError::ForeignSemanticStore);
        }
        self.require_published_store(store)?;
        let prepared =
            self.prepare_semantic_transaction_with_callback_release(store, staged.transaction)?;
        self.apply_prepared_semantic_transaction_with_execution_contract(
            prepared,
            Vec::new(),
            PreparedRuntimePublication {
                prepared_effect_programs: staged.programs,
                ..Default::default()
            },
            SemanticPublicationPurpose::AuthoredMutation,
            None,
            Some(staged.root),
        )
        .map(|(result, _)| result)
    }
}
