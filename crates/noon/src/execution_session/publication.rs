use noon_compile::{
    prepare_semantic_publication, prepare_semantic_publication_with_scalar_timeline,
    semantic_execution_object_id, validate_semantic_publication, ExecutionMutationTransaction,
    ExecutionPatch, SemanticPublicationLoweringError, SemanticPublicationPreparationStats,
};
use noon_core::{
    PreparedSemanticMutationTransaction, PublicationContext, SceneRevision, SemanticMutation,
    SemanticMutationTransaction, SemanticMutationTransactionError,
    SemanticMutationTransactionResult, SemanticNodeId, SemanticNodeKind, SemanticStore,
    SemanticTransactionNodeRef,
};
use noon_runtime::{
    apply_execution_slot_membership_changes, preflight_execution_slot_membership_shape,
    AuthoredPublicationError, ExecutionSlotError, FrameObjectState, PreparedEffectivePropertyBatch,
    PreparedReactiveSignalEnrollmentBatch,
};

use super::ExecutionSession;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SemanticPublicationPurpose {
    AuthoredMutation,
    SegmentCompletion,
}

fn lower_root_order_patches(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    root: SemanticNodeId,
) -> Result<Vec<ExecutionPatch>, SemanticPublicationLoweringError> {
    fn leaves(
        store: &SemanticStore,
        node: SemanticNodeId,
        output: &mut Vec<SemanticNodeId>,
    ) -> Result<(), SemanticPublicationLoweringError> {
        let node_state = store.node(node).ok_or_else(|| {
            SemanticPublicationLoweringError::from(noon_compile::SemanticLoweringError::Store(
                noon_core::SemanticStoreError::UnknownNode(node),
            ))
        })?;
        match node_state.kind() {
            SemanticNodeKind::Object(_) | SemanticNodeKind::AuthoringObject => output.push(node),
            SemanticNodeKind::Family => {
                for member in node_state.members() {
                    leaves(store, member, output)?;
                }
            }
            SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => {}
        }
        Ok(())
    }

    let mut patches = Vec::new();
    for mutation in prepared.candidate_mutations() {
        match mutation {
            SemanticMutation::AddMember { family, member } if family.existing() == Some(root) => {
                let Some(member) = member.existing() else {
                    continue;
                };
                let mut member_leaves = Vec::new();
                leaves(prepared.store(), member, &mut member_leaves)?;
                for leaf in member_leaves {
                    patches.push(ExecutionPatch::ReorderObject {
                        object: semantic_execution_object_id(leaf),
                        before: None,
                    });
                }
            }
            SemanticMutation::ReorderMember {
                family,
                member,
                before,
            } if family.existing() == Some(root) => {
                let Some(member) = member.existing() else {
                    continue;
                };
                let mut member_leaves = Vec::new();
                leaves(prepared.store(), member, &mut member_leaves)?;
                let mut anchor =
                    if let Some(before) = before.and_then(SemanticTransactionNodeRef::existing) {
                        let mut anchor_leaves = Vec::new();
                        leaves(prepared.store(), before, &mut anchor_leaves)?;
                        anchor_leaves
                            .first()
                            .copied()
                            .map(semantic_execution_object_id)
                    } else {
                        None
                    };
                for leaf in member_leaves.into_iter().rev() {
                    let object = semantic_execution_object_id(leaf);
                    patches.push(ExecutionPatch::ReorderObject {
                        object,
                        before: anchor,
                    });
                    anchor = Some(object);
                }
            }
            _ => {}
        }
    }
    Ok(patches)
}

pub(crate) struct PreparedReactiveEnrollmentBatch {
    pub projection_enrollments: Vec<noon_compile::PreparedSemanticInputSignalEnrollment>,
    pub runtime_enrollment: PreparedReactiveSignalEnrollmentBatch,
}

struct PreparedScalarPublicationContract {
    handled_signals: std::collections::HashSet<SemanticNodeId>,
    reactive_enrollment: Option<PreparedReactiveEnrollmentBatch>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionPublicationError {
    RequiredCallbackPending,
    SegmentCompletionPending,
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
            Self::RequiredCallbackPending => {
                f.write_str("a required callback publication is pending")
            }
            Self::SegmentCompletionPending => f.write_str(
                "an active animation segment must be completed before authored publication",
            ),
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
    authored_content_layout_applicable: bool,
}

impl EffectiveSemanticObject<'_> {
    /// Whether authored content plus the effective affine transform exactly
    /// describes this frame's layout. Morph/reveal render overrides require a
    /// dedicated effective-content layout path and are rejected by the first
    /// ordinary live-query subset.
    pub const fn authored_content_layout_applicable(&self) -> bool {
        self.authored_content_layout_applicable
    }
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
    /// Structural publication admits geometry entries, local
    /// family exits, and content already owned by this store before session bootstrap.
    /// Aliases are reduced to exact net membership after semantic commit. Root-relative
    /// painter reordering is published through [`LiveSession`](crate::LiveSession), which
    /// supplies the authoritative execution root. Resource allocation and reactive
    /// membership remain explicit unsupported cases.
    pub fn apply_semantic_transaction(
        &mut self,
        store: &mut SemanticStore,
        transaction: SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        self.apply_semantic_transaction_with_execution(
            store,
            transaction,
            Vec::new(),
            None,
            SemanticPublicationPurpose::AuthoredMutation,
        )
    }

    pub(crate) fn apply_semantic_transaction_at_root(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        transaction: SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        self.require_published_store(store)?;
        validate_semantic_publication(&transaction)
            .map_err(ExecutionSessionPublicationError::Lowering)?;
        let prepared = transaction
            .prepare(store)
            .map_err(ExecutionSessionPublicationError::Semantic)?;
        self.apply_prepared_semantic_transaction_with_execution_contract(
            prepared,
            Vec::new(),
            None,
            SemanticPublicationPurpose::AuthoredMutation,
            None,
            Some(root),
        )
    }

    pub(crate) fn apply_semantic_transaction_with_execution(
        &mut self,
        store: &mut SemanticStore,
        transaction: SemanticMutationTransaction,
        execution_prefix: Vec<ExecutionPatch>,
        effective: Option<PreparedEffectivePropertyBatch>,
        purpose: SemanticPublicationPurpose,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        if self.pending_callback.is_some() {
            return Err(ExecutionSessionPublicationError::RequiredCallbackPending);
        }
        if self.pending_segment_completion.is_some()
            && purpose != SemanticPublicationPurpose::SegmentCompletion
        {
            return Err(ExecutionSessionPublicationError::SegmentCompletionPending);
        }
        self.require_published_store(store)?;
        validate_semantic_publication(&transaction)
            .map_err(ExecutionSessionPublicationError::Lowering)?;
        let prepared = transaction
            .prepare(store)
            .map_err(ExecutionSessionPublicationError::Semantic)?;
        self.apply_prepared_semantic_transaction_with_execution(
            prepared,
            execution_prefix,
            effective,
            purpose,
        )
    }

    /// Publish an already-prepared semantic transaction with its preflighted execution prefix.
    ///
    /// This is the shared infallible suffix for callers that must inspect transaction-local
    /// semantic references while preparing compiler/runtime work. All fallible work remains
    /// before `commit_with_store`, and the returned result is the sole local-name mapping.
    pub(crate) fn apply_prepared_semantic_transaction_with_execution(
        &mut self,
        prepared: PreparedSemanticMutationTransaction<'_>,
        execution_prefix: Vec<ExecutionPatch>,
        effective: Option<PreparedEffectivePropertyBatch>,
        purpose: SemanticPublicationPurpose,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        self.apply_prepared_semantic_transaction_with_execution_contract(
            prepared,
            execution_prefix,
            effective,
            purpose,
            None,
            None,
        )
    }

    pub(crate) fn apply_prepared_semantic_transaction_with_execution_and_reactive_enrollment(
        &mut self,
        prepared: PreparedSemanticMutationTransaction<'_>,
        execution_prefix: Vec<ExecutionPatch>,
        effective: Option<PreparedEffectivePropertyBatch>,
        purpose: SemanticPublicationPurpose,
        reactive_enrollment: Option<PreparedReactiveEnrollmentBatch>,
        handled_scalar_signals: std::collections::HashSet<SemanticNodeId>,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        self.apply_prepared_semantic_transaction_with_execution_contract(
            prepared,
            execution_prefix,
            effective,
            purpose,
            Some(PreparedScalarPublicationContract {
                handled_signals: handled_scalar_signals,
                reactive_enrollment,
            }),
            None,
        )
    }

    pub(crate) fn apply_prepared_scalar_timeline_transaction_with_execution(
        &mut self,
        prepared: PreparedSemanticMutationTransaction<'_>,
        execution_prefix: Vec<ExecutionPatch>,
        effective: Option<PreparedEffectivePropertyBatch>,
        purpose: SemanticPublicationPurpose,
        handled_scalar_signals: std::collections::HashSet<SemanticNodeId>,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        self.apply_prepared_semantic_transaction_with_execution_contract(
            prepared,
            execution_prefix,
            effective,
            purpose,
            Some(PreparedScalarPublicationContract {
                handled_signals: handled_scalar_signals,
                reactive_enrollment: None,
            }),
            None,
        )
    }

    fn apply_prepared_semantic_transaction_with_execution_contract(
        &mut self,
        prepared: PreparedSemanticMutationTransaction<'_>,
        execution_prefix: Vec<ExecutionPatch>,
        effective: Option<PreparedEffectivePropertyBatch>,
        purpose: SemanticPublicationPurpose,
        scalar: Option<PreparedScalarPublicationContract>,
        order_root: Option<SemanticNodeId>,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        if self.pending_callback.is_some() {
            return Err(ExecutionSessionPublicationError::RequiredCallbackPending);
        }
        if self.pending_segment_completion.is_some()
            && purpose != SemanticPublicationPurpose::SegmentCompletion
        {
            return Err(ExecutionSessionPublicationError::SegmentCompletionPending);
        }
        self.require_published_store(prepared.store())?;
        if order_root.is_none() {
            if let Some(SemanticMutation::ReorderMember { family, .. }) = prepared
                .candidate_mutations()
                .find(|mutation| matches!(mutation, SemanticMutation::ReorderMember { .. }))
            {
                return Err(ExecutionSessionPublicationError::Lowering(
                    SemanticPublicationLoweringError::PainterOrderRootRequired { family: *family },
                ));
            }
        }
        let publication = match scalar.as_ref() {
            Some(scalar) => prepare_semantic_publication_with_scalar_timeline(
                &prepared,
                &self.execution_index,
                &self.reachability,
                &scalar.handled_signals,
            ),
            None => {
                prepare_semantic_publication(&prepared, &self.execution_index, &self.reachability)
            }
        }
        .map_err(ExecutionSessionPublicationError::Lowering)?;
        let preparation_stats = publication.stats();
        let order_patches = order_root
            .map(|root| lower_root_order_patches(&prepared, root))
            .transpose()
            .map_err(ExecutionSessionPublicationError::Lowering)?;
        let (execution_suffix, execution_prefix): (Vec<_>, Vec<_>) =
            execution_prefix.into_iter().partition(|patch| {
                matches!(
                    patch,
                    ExecutionPatch::AddTrack(_) | ExecutionPatch::AddFamilyAnimation(_)
                )
            });
        // Root-order targets and anchors remain visible in the proposed projection.
        // Removing every possible old family exit would incorrectly retire a
        // promoted survivor before validating its order patch.
        let ordered_survivors: std::collections::HashSet<_> = order_patches
            .iter()
            .flatten()
            .flat_map(|patch| match patch {
                ExecutionPatch::ReorderObject { object, before } => [Some(*object), *before],
                _ => [None, None],
            })
            .flatten()
            .collect();
        let mut conservative_patches = execution_prefix.clone();
        conservative_patches.extend_from_slice(publication.value_transaction().mutations());
        conservative_patches.extend(
            publication
                .possible_exits()
                .iter()
                .copied()
                .filter(|object| !ordered_survivors.contains(object))
                .map(ExecutionPatch::RemoveObject),
        );
        if !execution_suffix.is_empty()
            || order_patches
                .as_ref()
                .is_some_and(|items| !items.is_empty())
        {
            conservative_patches.extend(publication.conservative_existing_entry_patches());
        }
        conservative_patches.extend(order_patches.iter().flatten().cloned());
        conservative_patches.extend(execution_suffix.iter().cloned());
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
        if let Some(effective) = effective.as_ref() {
            self.runtime
                .preflight_effective_carry_forward(effective, self.publication_context())
                .map_err(ExecutionSessionPublicationError::Runtime)?;
        }
        preflight_execution_slot_membership_shape(
            &self.slots,
            publication.possible_exits(),
            publication.possible_entry_count(),
        )
        .map_err(ExecutionSessionPublicationError::ExecutionSlot)?;

        let (result, store) = prepared.commit_with_store();
        let membership = self
            .reachability
            .apply_transaction_result(store, &result)
            .expect("prepared publication validated every possible reachable object");
        let entered = membership.entered_execution_objects().collect::<Vec<_>>();
        let exited = membership.exited_execution_objects().collect::<Vec<_>>();
        let execution = publication.bind(&result, &membership);
        let (execution, resource_additions) = execution.into_parts();
        let execution = ExecutionMutationTransaction::from_mutations(
            execution_prefix
                .into_iter()
                .chain(execution.mutations().iter().cloned())
                .chain(order_patches.into_iter().flatten())
                .chain(execution_suffix),
        );
        if let Some(reactive_enrollment) = scalar.and_then(|scalar| scalar.reactive_enrollment) {
            for projection_enrollment in reactive_enrollment.projection_enrollments {
                let expected = projection_enrollment.execution_signal();
                let signal = self
                    .reactive_projection
                    .commit_input_signal_enrollment(projection_enrollment);
                debug_assert_eq!(signal, expected);
            }
            self.runtime
                .commit_reactive_signal_enrollment_batch(reactive_enrollment.runtime_enrollment);
        }
        self.runtime
            .apply_authored_execution_transaction_with_effective(
                &execution,
                resource_additions,
                effective,
                self.publication_context(),
                store.scene_revision(),
            )
            .expect("runtime publication was fully preflighted before semantic commit");
        apply_execution_slot_membership_changes(&mut self.slots, &exited, &entered)
            .expect("exact membership is a subset of the preflighted structural shape");
        self.execution_index
            .apply_transaction_result(store, &result);
        self.execution_index.apply_reachability_update(&membership);
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
        let execution_object = self
            .execution_index
            .execution_object_id(node)
            .ok_or(ExecutionSessionPublicationError::UnknownObject(node))?;
        let object_index = self
            .runtime
            .frame_index_for_object(execution_object)
            .ok_or(ExecutionSessionPublicationError::UnknownObject(node))?;
        let frame = self.runtime.frame();
        let object = frame
            .objects
            .get(object_index)
            .ok_or(ExecutionSessionPublicationError::UnknownObject(node))?;
        let authored_content_layout_applicable = frame.render_geometries[object_index].is_none()
            && frame.render_transforms[object_index].is_none()
            && frame.reveals[object_index] == 1.0
            && frame.morphs[object_index] == 0.0;
        Ok(EffectiveSemanticObject {
            object,
            publication: self.publication_context(),
            authored_content_layout_applicable,
        })
    }
}

#[cfg(test)]
mod tests;
