use super::*;

/// Final P1 proof for ordinary local transform/style publication.
///
/// This value owns the exclusive semantic preflight and borrows the execution
/// session mutably until publication, so neither authority can change between
/// Runtime preparation and the one semantic point of no return.
pub(super) struct PreparedPublication<'session, 'store> {
    session: &'session mut ExecutionSession,
    semantic: PreparedSemanticMutationTransaction<'store>,
    runtime: noon_runtime::PreparedAuthoredValuePublication,
    preparation: SemanticPublicationPreparationStats,
}

impl<'session, 'store> PreparedPublication<'session, 'store> {
    pub(super) fn supports(prepared: &PreparedSemanticMutationTransaction<'_>) -> bool {
        prepared.mutations().iter().all(|mutation| {
            matches!(
                mutation,
                SemanticMutation::SetProperty { .. } | SemanticMutation::ReplaceStyle { .. }
            )
        })
    }

    pub(super) fn prepare(
        session: &'session mut ExecutionSession,
        semantic: PreparedSemanticMutationTransaction<'store>,
        order_root: Option<SemanticNodeId>,
    ) -> Result<Self, ExecutionSessionPublicationError> {
        session.require_publication_ready(SemanticPublicationPurpose::AuthoredMutation)?;
        session.require_published_store(semantic.store())?;
        if let Some(root) = order_root {
            if !session.reachability.is_execution_root(root) {
                return Err(ExecutionSessionPublicationError::UnknownObject(root));
            }
        }

        let plan = prepare_semantic_publication(
            &semantic,
            &session.execution_index,
            &session.reachability,
        )
        .map_err(ExecutionSessionPublicationError::Lowering)?;
        let preparation = plan.stats();
        debug_assert_eq!(plan.possible_entry_count(), 0);
        debug_assert!(plan.possible_exits().is_empty());
        debug_assert_eq!(plan.resource_additions().text_count(), 0);
        debug_assert_eq!(plan.resource_additions().font_count(), 0);
        debug_assert_eq!(plan.resource_additions().geometry_count(), 0);

        let runtime = session
            .runtime
            .prepare_authored_value_publication(
                plan.value_transaction(),
                session.publication_context(),
                semantic.proposed_scene_revision(),
            )
            .map_err(ExecutionSessionPublicationError::Runtime)?
            .ok_or(ExecutionSessionPublicationError::Lowering(
                SemanticPublicationLoweringError::UnsupportedMutation { index: 0 },
            ))?;

        Ok(Self {
            session,
            semantic,
            runtime,
            preparation,
        })
    }

    /// Cross the Semantic Scene point of no return exactly once, then perform only
    /// commits whose complete compiler/Runtime proof is already owned by `self`.
    pub(super) fn publish(self) -> SemanticMutationTransactionResult {
        let Self {
            session,
            semantic,
            runtime,
            preparation,
        } = self;
        let (result, store) = semantic.commit_with_store();
        session
            .runtime
            .commit_prepared_authored_value_publication(runtime);
        session
            .execution_index
            .apply_transaction_result(store, &result);
        session.last_structural_publication = StructuralPublicationStats {
            preparation,
            entered_objects: 0,
            exited_objects: 0,
        };
        result
    }
}
