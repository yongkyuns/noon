use noon_compile::{
    is_semantic_updater_publication, prepare_semantic_publication,
    prepare_semantic_publication_with_scalar_timeline, prepare_semantic_root_order,
    prepare_semantic_updater_publication, validate_semantic_publication,
    ExecutionMutationTransaction, ExecutionPatch, SemanticPublicationLoweringError,
    SemanticPublicationPreparationStats,
};
use noon_core::{
    PreparedSemanticMutationTransaction, PublicationContext, SceneRevision, SemanticMutation,
    SemanticMutationTransaction, SemanticMutationTransactionError,
    SemanticMutationTransactionResult, SemanticNodeId, SemanticStore,
};
use noon_runtime::{
    preflight_execution_slot_membership_shape, AuthoredPublicationError, ExecutionSlotError,
    FrameObjectState, PreparedEffectivePropertyBatch, PreparedReactiveSignalEnrollmentBatch,
};

use super::ExecutionSession;

mod prepared_value;
use prepared_value::PreparedPublication;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SemanticPublicationPurpose {
    AuthoredMutation,
    SegmentCompletion,
    TranslationDrag(SemanticNodeId),
    Callback(super::CallbackPhaseToken),
}

/// Runtime half of the shared authored publication. Native ingress and host
/// callbacks can supply a speculative frame without publishing it first.
#[derive(Default)]
pub(super) struct PreparedRuntimePublication {
    pub effective: Option<PreparedEffectivePropertyBatch>,
    pub frame: Option<noon_runtime::PreparedFrameEvaluation>,
}

impl From<Option<PreparedEffectivePropertyBatch>> for PreparedRuntimePublication {
    fn from(effective: Option<PreparedEffectivePropertyBatch>) -> Self {
        Self {
            effective,
            frame: None,
        }
    }
}

pub(crate) struct PreparedReactiveEnrollmentBatch {
    pub projection_enrollments: Vec<noon_compile::PreparedSemanticInputSignalEnrollment>,
    pub runtime_enrollment: PreparedReactiveSignalEnrollmentBatch,
}

pub(super) struct PreparedScalarPublicationContract {
    handled_signals: std::collections::HashSet<SemanticNodeId>,
    reactive_enrollment: Option<PreparedReactiveEnrollmentBatch>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionPublicationError {
    RequiredCallbackPending,
    SegmentCompletionPending,
    TranslationDragActive,
    ForeignSemanticStore,
    ReplaySealed,
    StaleSceneRevision {
        expected: SceneRevision,
        actual: SceneRevision,
    },
    UnknownObject(SemanticNodeId),
    Semantic(SemanticMutationTransactionError),
    Lowering(SemanticPublicationLoweringError),
    Runtime(AuthoredPublicationError),
    NumericText(noon_core::NumericTextResourceError),
    Geometry(noon_core::GeometryResourceError),
    ExecutionSlot(ExecutionSlotError),
}

impl std::fmt::Display for ExecutionSessionPublicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReplaySealed => f.write_str("discard sealed replay before authored mutation"),
            Self::RequiredCallbackPending => {
                f.write_str("a required callback publication is pending")
            }
            Self::SegmentCompletionPending => f.write_str(
                "an active animation segment must be completed before authored publication",
            ),
            Self::TranslationDragActive => f.write_str(
                "cancel or release the active translation drag before authored publication",
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
            Self::NumericText(error) => error.fmt(f),
            Self::Geometry(error) => error.fmt(f),
            Self::ExecutionSlot(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ExecutionSessionPublicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Semantic(error) => Some(error),
            Self::Lowering(error) => Some(error),
            Self::Runtime(error) => Some(error),
            Self::NumericText(error) => Some(error),
            Self::Geometry(error) => Some(error),
            Self::ExecutionSlot(error) => Some(error),
            Self::RequiredCallbackPending
            | Self::ReplaySealed
            | Self::SegmentCompletionPending
            | Self::TranslationDragActive
            | Self::ForeignSemanticStore
            | Self::StaleSceneRevision { .. }
            | Self::UnknownObject(_) => None,
        }
    }
}

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
    pub(crate) render_geometry: Option<&'a noon_core::GeometryRef>,
    pub(crate) render_transform: Option<noon_core::Transform2D>,
    pub(crate) reveal: f32,
    pub(crate) morph: f32,
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
    pub(crate) fn require_published_store(
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

    pub(super) fn require_publication_ready(
        &self,
        purpose: SemanticPublicationPurpose,
    ) -> Result<(), ExecutionSessionPublicationError> {
        if self.runtime.replay_is_sealed() {
            return Err(ExecutionSessionPublicationError::ReplaySealed);
        }
        if self
            .pending_callback_token()
            .is_some_and(|token| purpose != SemanticPublicationPurpose::Callback(token))
        {
            return Err(ExecutionSessionPublicationError::RequiredCallbackPending);
        }
        if let Some(pending) = self.pending_segment_completion.as_ref() {
            let admitted = purpose == SemanticPublicationPurpose::SegmentCompletion
                || matches!(purpose, SemanticPublicationPurpose::TranslationDrag(target)
                    if pending.allows_translation_drag(target));
            if !admitted {
                return Err(ExecutionSessionPublicationError::SegmentCompletionPending);
            }
        }
        // Ordinary edits cannot overwrite a held Position. An unrelated segment
        // may still finish through the same atomic publication lane.
        if let Some(target) = self.translation_drag.target() {
            let independent_completion = purpose == SemanticPublicationPurpose::SegmentCompletion
                && self
                    .pending_segment_completion
                    .as_ref()
                    .is_some_and(|pending| pending.allows_translation_drag(target));
            if !independent_completion {
                return Err(ExecutionSessionPublicationError::TranslationDragActive);
            }
        }
        Ok(())
    }

    /// Admit resource-producing detached construction before it can change the arena.
    /// Ordinary waits remain admissible: only animation completion and required
    /// callback publication block authored construction. No scene traversal is needed.
    pub(crate) fn require_resource_creation_at_root(
        &self,
        store: &SemanticStore,
        root: SemanticNodeId,
    ) -> Result<(), ExecutionSessionPublicationError> {
        self.require_published_store(store)?;
        self.require_publication_ready(SemanticPublicationPurpose::AuthoredMutation)?;
        // Match ordinary publication's execution-root precedence before
        // validating root shape, without importing any resource.
        if !self.reachability.is_execution_root(root) {
            return Err(ExecutionSessionPublicationError::UnknownObject(root));
        }
        noon_compile::validate_semantic_publication_root(store, root)
            .map_err(ExecutionSessionPublicationError::Lowering)?;
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
        if !is_semantic_updater_publication(transaction.mutations()) {
            validate_semantic_publication(&transaction)
                .map_err(ExecutionSessionPublicationError::Lowering)?;
        }
        let prepared = transaction
            .prepare(store)
            .map_err(ExecutionSessionPublicationError::Semantic)?;
        self.apply_prepared_semantic_transaction_with_execution_contract(
            prepared,
            Vec::new(),
            None.into(),
            SemanticPublicationPurpose::AuthoredMutation,
            None,
            Some(root),
        )
        .map(|(result, _)| result)
    }

    pub(crate) fn apply_semantic_transaction_with_execution(
        &mut self,
        store: &mut SemanticStore,
        transaction: SemanticMutationTransaction,
        execution_prefix: Vec<ExecutionPatch>,
        effective: Option<PreparedEffectivePropertyBatch>,
        purpose: SemanticPublicationPurpose,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        self.require_publication_ready(purpose)?;
        self.require_published_store(store)?;
        if !is_semantic_updater_publication(transaction.mutations()) {
            validate_semantic_publication(&transaction)
                .map_err(ExecutionSessionPublicationError::Lowering)?;
        }
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
            effective.into(),
            purpose,
            None,
            None,
        )
        .map(|(result, _)| result)
    }

    pub(crate) fn apply_prepared_semantic_transaction_with_execution_and_reactive_enrollment(
        &mut self,
        prepared: PreparedSemanticMutationTransaction<'_>,
        execution_prefix: Vec<ExecutionPatch>,
        order_root: Option<SemanticNodeId>,
        purpose: SemanticPublicationPurpose,
        reactive_enrollment: Option<PreparedReactiveEnrollmentBatch>,
        handled_scalar_signals: std::collections::HashSet<SemanticNodeId>,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        self.apply_prepared_semantic_transaction_with_execution_contract(
            prepared,
            execution_prefix,
            None.into(),
            purpose,
            Some(PreparedScalarPublicationContract {
                handled_signals: handled_scalar_signals,
                reactive_enrollment,
            }),
            order_root,
        )
        .map(|(result, _)| result)
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
            effective.into(),
            purpose,
            Some(PreparedScalarPublicationContract {
                handled_signals: handled_scalar_signals,
                reactive_enrollment: None,
            }),
            None,
        )
        .map(|(result, _)| result)
    }

    pub(crate) fn apply_prepared_scalar_timeline_transaction_with_execution_at_root(
        &mut self,
        prepared: PreparedSemanticMutationTransaction<'_>,
        execution_prefix: Vec<ExecutionPatch>,
        effective: Option<PreparedEffectivePropertyBatch>,
        purpose: SemanticPublicationPurpose,
        handled_scalar_signals: std::collections::HashSet<SemanticNodeId>,
        order_root: SemanticNodeId,
    ) -> Result<SemanticMutationTransactionResult, ExecutionSessionPublicationError> {
        self.apply_prepared_semantic_transaction_with_execution_contract(
            prepared,
            execution_prefix,
            effective.into(),
            purpose,
            Some(PreparedScalarPublicationContract {
                handled_signals: handled_scalar_signals,
                reactive_enrollment: None,
            }),
            Some(order_root),
        )
        .map(|(result, _)| result)
    }

    pub(super) fn apply_prepared_semantic_transaction_with_execution_contract(
        &mut self,
        prepared: PreparedSemanticMutationTransaction<'_>,
        execution_prefix: Vec<ExecutionPatch>,
        runtime: PreparedRuntimePublication,
        purpose: SemanticPublicationPurpose,
        scalar: Option<PreparedScalarPublicationContract>,
        order_root: Option<SemanticNodeId>,
    ) -> Result<
        (
            SemanticMutationTransactionResult,
            Option<super::callback::CallbackCompletion>,
        ),
        ExecutionSessionPublicationError,
    > {
        prepared
            .with_pending_geometry_paths(|prepared| {
                self.publish_materialized_semantic_transaction(
                    prepared,
                    execution_prefix,
                    runtime,
                    purpose,
                    scalar,
                    order_root,
                )
            })
            .map_err(|error| match error {
                noon_core::PendingGeometryPublicationError::Resource(error) => {
                    ExecutionSessionPublicationError::Geometry(error)
                }
                noon_core::PendingGeometryPublicationError::Transaction(error) => {
                    ExecutionSessionPublicationError::Semantic(error)
                }
                noon_core::PendingGeometryPublicationError::Publication(error) => error,
            })
    }

    // No provisional resource declaration reaches lowering. The outer existing
    // resource scope owns rollback through this complete publication boundary.
    fn publish_materialized_semantic_transaction(
        &mut self,
        prepared: PreparedSemanticMutationTransaction<'_>,
        execution_prefix: Vec<ExecutionPatch>,
        runtime: PreparedRuntimePublication,
        purpose: SemanticPublicationPurpose,
        scalar: Option<PreparedScalarPublicationContract>,
        order_root: Option<SemanticNodeId>,
    ) -> Result<
        (
            SemanticMutationTransactionResult,
            Option<super::callback::CallbackCompletion>,
        ),
        ExecutionSessionPublicationError,
    > {
        let PreparedRuntimePublication { effective, frame } = runtime;
        debug_assert!(
            frame.is_none() || !matches!(purpose, SemanticPublicationPurpose::Callback(_))
        );
        let prepared_frame = match purpose {
            SemanticPublicationPurpose::Callback(_) => self
                .pending_callback
                .as_ref()
                .map(|pending| &pending.prepared),
            _ => frame.as_ref(),
        };
        if purpose == SemanticPublicationPurpose::AuthoredMutation
            && execution_prefix.is_empty()
            && effective.is_none()
            && frame.is_none()
            && scalar.is_none()
            && !self.runtime.replay_scope_active()
            && PreparedPublication::supports(&prepared)
        {
            return Ok((
                PreparedPublication::prepare(self, prepared, order_root)?.publish(),
                None,
            ));
        }

        self.require_publication_ready(purpose)?;
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
        let (publication, revised_callbacks) =
            if is_semantic_updater_publication(prepared.mutations()) {
                // Registration publication cannot smuggle an execution prefix or
                // completion carry into its callback-only lowering contract.
                if !execution_prefix.is_empty()
                    || effective.is_some()
                    || scalar.is_some()
                    || purpose != SemanticPublicationPurpose::AuthoredMutation
                {
                    return Err(ExecutionSessionPublicationError::Lowering(
                        SemanticPublicationLoweringError::UnsupportedMutation { index: 0 },
                    ));
                }
                prepare_semantic_updater_publication(
                    &prepared,
                    self.callback_schedule.plan(),
                    self.frame().time,
                )
                .map_err(ExecutionSessionPublicationError::Lowering)?
            } else {
                let publication = match scalar.as_ref() {
                    Some(scalar) => prepare_semantic_publication_with_scalar_timeline(
                        &prepared,
                        &self.execution_index,
                        &self.reachability,
                        &scalar.handled_signals,
                    ),
                    None => prepare_semantic_publication(
                        &prepared,
                        &self.execution_index,
                        &self.reachability,
                    ),
                }
                .map_err(ExecutionSessionPublicationError::Lowering)?;
                let revised = self
                    .callback_schedule
                    .plan()
                    .prepare_registration_revision(&prepared, self.frame().time)
                    .map_err(ExecutionSessionPublicationError::Lowering)?;
                (publication, revised)
            };
        let preparation_stats = publication.stats();
        let order_patches = order_root
            .map(|root| prepare_semantic_root_order(&prepared, root))
            .transpose()
            .map_err(ExecutionSessionPublicationError::Lowering)?;
        // Compiler validation preserves stale/wrong-kind root diagnostics. A valid
        // family with the same leaves still cannot select a different execution domain.
        if let Some(root) = order_root {
            if !self.reachability.is_execution_root(root) {
                return Err(ExecutionSessionPublicationError::UnknownObject(root));
            }
        }
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
        let graph_patches = publication.conservative_graph_patches().collect::<Vec<_>>();
        if !execution_suffix.is_empty()
            || order_patches
                .as_ref()
                .is_some_and(|items| !items.is_empty())
            || !graph_patches.is_empty()
        {
            conservative_patches.extend(publication.conservative_entry_patches(&prepared));
        }
        conservative_patches.extend(order_patches.iter().flatten().cloned());
        conservative_patches.extend(graph_patches);
        conservative_patches.extend(execution_suffix.iter().cloned());
        let conservative = ExecutionMutationTransaction::from_mutations(conservative_patches);
        let numeric_entries = publication
            .conservative_numeric_text(&prepared)
            .into_iter()
            .map(|entry| noon_runtime::NumericTextDriverRevisionEntry {
                object: entry.object,
                declaration: entry.declaration,
            })
            .collect::<Vec<_>>();
        let mut numeric_patches = conservative.mutations().to_vec();
        numeric_patches.extend(publication.conservative_entry_patches(&prepared));
        let numeric_transaction = ExecutionMutationTransaction::from_mutations(numeric_patches);
        let mut pending_numeric_signals = scalar
            .as_ref()
            .and_then(|scalar| scalar.reactive_enrollment.as_ref())
            .into_iter()
            .flat_map(|enrollment| &enrollment.projection_enrollments)
            .map(|enrollment| (enrollment.execution_signal(), enrollment.value().clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        if let Some(frame) = prepared_frame {
            for declaration in numeric_entries
                .iter()
                .filter_map(|entry| entry.declaration.as_ref())
            {
                if let Some(value) = self
                    .runtime
                    .prepared_reactive_value(frame, declaration.signal)
                {
                    pending_numeric_signals.insert(declaration.signal, value);
                }
            }
        }
        let prepared_numeric = self
            .runtime
            .prepare_numeric_text_driver_revision(
                &numeric_transaction,
                numeric_entries,
                prepared.store().text_resources(),
                &pending_numeric_signals,
            )
            .map_err(ExecutionSessionPublicationError::NumericText)?;
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

        if let Some(frame) = prepared_frame {
            let empty = self
                .runtime
                .prepare_effective_property_batch(&[])
                .expect("empty effective writes are valid");
            self.runtime
                .preflight_prepared_frame_commit(frame, effective.as_ref().unwrap_or(&empty))
                .map_err(|error| {
                    ExecutionSessionPublicationError::Runtime(
                        AuthoredPublicationError::PreparedFrame(error),
                    )
                })?;
        }

        let changed_native_bindings = prepared
            .mutations()
            .iter()
            .filter_map(|mutation| match mutation {
                noon_core::SemanticMutation::ChangeSubscription { object, .. } => object.existing(),
                _ => None,
            })
            .collect::<Vec<_>>();
        let (result, store) = prepared.commit_with_store();
        let membership = self
            .reachability
            .apply_transaction_result(store, &result)
            .expect("prepared publication validated every possible reachable object");
        let entered = membership.entered_execution_objects().collect::<Vec<_>>();
        let exited = membership.exited_execution_objects().collect::<Vec<_>>();
        let execution = publication.bind(&result, &membership);
        let (execution, resource_additions, exact_numeric) =
            execution.into_parts_with_numeric_text();
        let exact_numeric = exact_numeric
            .into_iter()
            .map(|entry| noon_runtime::NumericTextDriverRevisionEntry {
                object: entry.object,
                declaration: entry.declaration,
            })
            .collect::<Vec<_>>();
        let prepared_numeric = prepared_numeric.retain_exact(&exact_numeric);
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
        let callback = match purpose {
            SemanticPublicationPurpose::Callback(_) => Some(
                self.pending_callback
                    .take()
                    .expect("callback remained pending through semantic preflight")
                    .into_parts(),
            ),
            _ => None,
        };
        let (frame, mut completion) = callback.map_or((frame, None), |(frame, completion)| {
            (Some(frame), Some(completion))
        });
        self.runtime
            .apply_authored_execution_transaction_with_frame(
                &execution,
                resource_additions,
                effective,
                frame,
                self.publication_context(),
                store.scene_revision(),
            )
            .expect("runtime publication was fully preflighted before semantic commit");
        // Publish the frame's schedule preview before reconciling membership.
        // Otherwise that old preview could undo an enter/exit in this batch.
        if let Some(completion) = completion.as_mut() {
            self.commit_callback_progress(completion);
        }
        if let Some(revision) = revised_callbacks {
            self.callback_schedule
                .apply_revision(revision, store, self.frame().time);
            self.last_callback_receipt = None;
        }
        if !changed_native_bindings.is_empty() {
            self.callback_schedule
                .refresh_native_bindings(store, changed_native_bindings);
        }
        self.publish_replay_membership(&exited, &entered);
        self.reconcile_callback_membership(&exited, false);
        self.execution_index
            .apply_transaction_result(store, &result);
        self.execution_index.apply_reachability_update(&membership);
        self.reconcile_callback_membership(&entered, true);
        self.runtime
            .commit_numeric_text_driver_revision(prepared_numeric);
        self.sync_inset_2d_view_bindings(store);
        self.refresh_interaction_bindings(store, &result, &entered, &exited);
        self.reconcile_pointer_selection();
        self.last_structural_publication = StructuralPublicationStats {
            preparation: preparation_stats,
            entered_objects: entered.len(),
            exited_objects: exited.len(),
        };
        Ok((result, completion))
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
            render_geometry: frame.render_geometries[object_index].as_deref(),
            render_transform: frame.render_transforms[object_index],
            reveal: frame.reveals[object_index],
            morph: frame.morphs[object_index],
        })
    }
}

#[cfg(test)]
mod tests;
