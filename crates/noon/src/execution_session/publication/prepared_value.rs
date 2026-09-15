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

impl ExecutionSession {
    /// Add the stable painter anchors required by active identity-free transient
    /// presentation to an ordinary retained viewport query.
    ///
    /// Spatial visibility remains authoritative for ordinary stable content. A
    /// transient occurrence may move independently of its stable anchor, however,
    /// so the renderer must retain that anchor as painter-placement provenance even
    /// when the anchor's own bounds are outside the viewport. Runtime painter ranks
    /// are the one ordering authority: merging visits only the visible candidates
    /// plus unique active anchors and never scans the full painter permutation.
    ///
    /// `spatial_stats()` continues to describe only the spatial-index query. The
    /// returned object indices may therefore contain additional mandatory anchors.
    #[doc(hidden)]
    pub fn renderer_viewport_query(
        &self,
        mut query: crate::execution_session::ExecutionViewportQuery,
    ) -> crate::execution_session::ExecutionViewportQuery {
        let Some(plan) = self.derived_display_plan.as_ref() else {
            return query;
        };
        let transient = plan
            .evaluate(self.runtime.frame().time)
            .expect("validated derived display plan must evaluate at runtime time");
        if transient.is_empty() {
            return query;
        }

        let mut seen = query
            .object_indices
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let mut anchors = Vec::<(u32, usize)>::new();
        for occurrence in transient {
            let anchor = occurrence.anchor_object_index() as usize;
            if !seen.insert(anchor) {
                continue;
            }
            let rank = self.runtime.painter_rank(anchor).unwrap_or_else(|| {
                panic!(
                    "validated transient presentation anchor {anchor} has no runtime painter rank"
                )
            });
            anchors.push((rank, anchor));
        }
        if anchors.is_empty() {
            return query;
        }
        anchors.sort_unstable_by_key(|&(rank, _)| rank);

        let visible = std::mem::take(&mut query.object_indices);
        let visible = visible
            .into_iter()
            .map(|object_index| {
                let rank = self.runtime.painter_rank(object_index).unwrap_or_else(|| {
                    panic!(
                        "retained viewport object {object_index} has no runtime painter rank"
                    )
                });
                (rank, object_index)
            })
            .collect::<Vec<_>>();
        debug_assert!(visible.windows(2).all(|pair| pair[0].0 < pair[1].0));

        let mut merged = Vec::with_capacity(visible.len() + anchors.len());
        let mut visible = visible.into_iter().peekable();
        let mut anchors = anchors.into_iter().peekable();
        loop {
            match (visible.peek(), anchors.peek()) {
                (Some(&(visible_rank, _)), Some(&(anchor_rank, _))) => {
                    if visible_rank < anchor_rank {
                        merged.push(visible.next().expect("peeked visible row").1);
                    } else {
                        debug_assert_ne!(visible_rank, anchor_rank);
                        merged.push(anchors.next().expect("peeked transient anchor").1);
                    }
                }
                (Some(_), None) => {
                    merged.extend(visible.map(|(_, object_index)| object_index));
                    break;
                }
                (None, Some(_)) => {
                    merged.extend(anchors.map(|(_, object_index)| object_index));
                    break;
                }
                (None, None) => break,
            }
        }
        query.object_indices = merged;
        query
    }
}

#[cfg(test)]
mod viewport_tests {
    fn merge_ranked(
        visible: &[(u32, usize)],
        anchors: &[(u32, usize)],
    ) -> Vec<usize> {
        let mut merged = Vec::with_capacity(visible.len() + anchors.len());
        let mut visible = visible.iter().copied().peekable();
        let mut anchors = anchors.iter().copied().peekable();
        loop {
            match (visible.peek(), anchors.peek()) {
                (Some(&(visible_rank, _)), Some(&(anchor_rank, _))) => {
                    if visible_rank < anchor_rank {
                        merged.push(visible.next().unwrap().1);
                    } else {
                        merged.push(anchors.next().unwrap().1);
                    }
                }
                (Some(_), None) => {
                    merged.extend(visible.map(|(_, object)| object));
                    break;
                }
                (None, Some(_)) => {
                    merged.extend(anchors.map(|(_, object)| object));
                    break;
                }
                (None, None) => break,
            }
        }
        merged
    }

    #[test]
    fn renderer_viewport_anchor_merge_preserves_runtime_painter_order() {
        assert_eq!(
            merge_ranked(&[(1, 20), (4, 50), (7, 80)], &[(0, 10), (3, 40), (9, 100)]),
            [10, 20, 40, 50, 80, 100]
        );
    }
}
