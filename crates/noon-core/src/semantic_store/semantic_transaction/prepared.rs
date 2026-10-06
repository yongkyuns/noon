use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
};

use super::*;
use crate::{
    SceneRevision, SemanticGraphDeclaration, SemanticGraphEdgeBinding, SemanticGraphEdgeDependency,
    SemanticTransactionGraphEdgeDependency,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticTransactionReadError {
    PendingNodeFromDifferentTransaction(SemanticLocalNodeToken),
    UnknownPendingNode(SemanticLocalNodeToken),
    RemovedPendingNode(SemanticLocalNodeToken),
    /// Existing-handle planners cannot traverse an order link that leads to a
    /// transaction-local node. They must reject rather than truncate it.
    PendingMembershipAdjacency(SemanticLocalNodeToken),
    /// The requested local object does not carry transaction-local path content.
    NotPendingGeometry(SemanticTransactionNodeRef),
    /// A pending path declaration lost its transaction-owned payload before read.
    UnknownPendingGeometry(SemanticTransactionNodeRef),
    RemovedExistingNode(SemanticNodeId),
    UnknownExistingNode(SemanticNodeId),
    NotObject(SemanticTransactionNodeRef),
    NotFamily(SemanticTransactionNodeRef),
    NotAnimation(SemanticTransactionNodeRef),
    Existing(SemanticSceneOperationError),
}

impl std::fmt::Display for SemanticTransactionReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "semantic transaction staged read failed: {self:?}"
        )
    }
}

impl std::error::Error for SemanticTransactionReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Existing(error) => Some(error),
            _ => None,
        }
    }
}

/// A fully validated mutation batch holding the store exclusively until commit.
///
/// Preparing or dropping this value does not change authored state, revision, or
/// work counters. The exclusive borrow prevents invalidating its preflight proof.
///
/// ```compile_fail
/// use noon_core::{SemanticMutationTransaction, SemanticStore};
/// let mut store = SemanticStore::new();
/// let prepared = SemanticMutationTransaction::new().prepare(&mut store).unwrap();
/// let competing = SemanticMutationTransaction::new().prepare(&mut store);
/// prepared.commit();
/// ```
#[must_use = "dropping a prepared transaction discards its uncommitted mutations"]
pub struct PreparedSemanticMutationTransaction<'a> {
    store: &'a mut SemanticStore,
    transaction: SemanticMutationTransaction,
    preflight: SemanticTransactionPreflight,
    next_revision: Option<SceneRevision>,
    planned_nodes: HashMap<SemanticLocalNodeToken, SemanticNodeId>,
}

struct PreparedTransactionParts {
    preflight: SemanticTransactionPreflight,
    next_revision: Option<SceneRevision>,
    planned_nodes: HashMap<SemanticLocalNodeToken, SemanticNodeId>,
}

enum PreparedExtensionError<E> {
    Extension(E),
    Preflight(SemanticMutationTransactionError),
}

/// Failure while materializing transaction-owned path payloads inside the same
/// scope as a final semantic/execution publication.
#[derive(Debug)]
pub enum PendingGeometryPublicationError<E> {
    Resource(crate::GeometryResourceError),
    Transaction(SemanticMutationTransactionError),
    Publication(E),
}

impl<E> From<crate::GeometryResourceError> for PendingGeometryPublicationError<E> {
    fn from(error: crate::GeometryResourceError) -> Self {
        Self::Resource(error)
    }
}

/// A recoverable extension failure that may have retained fresh transaction-local
/// resource payloads. Recovery drops only the extension's payload suffix while
/// preserving prior callback declarations and monotonic local token allocation.
#[derive(Debug)]
pub enum PendingResourceExtensionError<E> {
    Extension(E),
    Preflight(SemanticMutationTransactionError),
}

impl<'a> PreparedSemanticMutationTransaction<'a> {
    fn pending_path_resource_tokens(
        preflight: &SemanticTransactionPreflight,
    ) -> Vec<SemanticLocalResourceToken> {
        let mut seen = HashSet::with_capacity(preflight.staged_pending_paths.len());
        preflight
            .staged_pending_paths
            .values()
            .map(|state| state.resource())
            .filter(|resource| seen.insert(*resource))
            .collect()
    }
    pub(super) fn new(
        transaction: SemanticMutationTransaction,
        store: &'a mut SemanticStore,
    ) -> Result<Self, SemanticMutationTransactionError> {
        Self::new_recoverable(transaction, store).map_err(|(_, error)| error)
    }

    pub(super) fn new_recoverable(
        transaction: SemanticMutationTransaction,
        store: &'a mut SemanticStore,
    ) -> Result<
        Self,
        (
            SemanticMutationTransaction,
            SemanticMutationTransactionError,
        ),
    > {
        let PreparedTransactionParts {
            preflight,
            next_revision,
            planned_nodes,
        } = match Self::preflight_parts(&transaction, store) {
            Ok(parts) => parts,
            Err(error) => return Err((transaction, error)),
        };
        Ok(Self {
            store,
            transaction,
            preflight,
            next_revision,
            planned_nodes,
        })
    }

    /// The published store, held read-only while this batch is staged.
    pub fn store(&self) -> &SemanticStore {
        self.store
    }

    /// Discard this preflight proof and recover its exact unpublished
    /// transaction.
    ///
    /// This is intentionally consuming: the returned transaction keeps its
    /// transaction identity and local-node allocator, while releasing the
    /// exclusive store borrow held by the proof. Callback collectors use it to
    /// stage several fallible existing-handle operations before preparing one
    /// final publication. It never clones or commits authored state.
    pub fn into_transaction(self) -> SemanticMutationTransaction {
        self.transaction
    }

    /// Extend an unpublished transaction, then preflight it atomically.
    ///
    /// On either extension or preflight failure, this restores the exact prior
    /// mutation prefix and preflight proof. Local-token allocation remains
    /// monotonic: a token exposed by a rejected extension is never reused by a
    /// later retry, preventing an escaped phase-local token from aliasing a new
    /// declaration. The small callers below select only their mutation source
    /// and error vocabulary.
    fn with_recoverable_extension<E>(
        self,
        allow_repeated_membership: Option<bool>,
        extend: impl FnOnce(&mut SemanticMutationTransaction, &SemanticStore) -> Result<(), E>,
    ) -> Result<Self, (Box<Self>, PreparedExtensionError<E>)> {
        let Self {
            store,
            mut transaction,
            preflight,
            next_revision,
            planned_nodes,
        } = self;
        let original_len = transaction.mutations.len();
        let original_resource_len = transaction.pending_resource_count();
        let original_repeated_membership = transaction.allow_repeated_membership_mutations;
        if let Some(allow_repeated_membership) = allow_repeated_membership {
            transaction.allow_repeated_membership_mutations = allow_repeated_membership;
        }
        if let Err(error) = extend(&mut transaction, store) {
            transaction.mutations.truncate(original_len);
            transaction.truncate_pending_resources(original_resource_len);
            transaction.allow_repeated_membership_mutations = original_repeated_membership;
            return Err((
                Box::new(Self {
                    store,
                    transaction,
                    preflight,
                    next_revision,
                    planned_nodes,
                }),
                PreparedExtensionError::Extension(error),
            ));
        }
        let PreparedTransactionParts {
            preflight: candidate_preflight,
            next_revision: candidate_next_revision,
            planned_nodes: candidate_planned_nodes,
        } = match Self::preflight_parts(&transaction, store) {
            Ok(parts) => parts,
            Err(error) => {
                transaction.mutations.truncate(original_len);
                transaction.truncate_pending_resources(original_resource_len);
                transaction.allow_repeated_membership_mutations = original_repeated_membership;
                return Err((
                    Box::new(Self {
                        store,
                        transaction,
                        preflight,
                        next_revision,
                        planned_nodes,
                    }),
                    PreparedExtensionError::Preflight(error),
                ));
            }
        };
        Ok(Self {
            store,
            transaction,
            preflight: candidate_preflight,
            next_revision: candidate_next_revision,
            planned_nodes: candidate_planned_nodes,
        })
    }

    /// Consume this unpublished proof after an existing-handle planner has read
    /// it, then re-preflight the combined transaction under the same exclusive
    /// store borrow. This is intentionally not `Clone`: provisional names and
    /// transaction provenance remain in their original allocation domain.
    pub(crate) fn with_existing_plan(
        self,
        plan: SemanticMutationTransaction,
    ) -> Result<Self, (Box<Self>, SemanticMutationTransactionError)> {
        debug_assert!(plan.mutations.iter().all(|mutation| {
            !matches!(
                mutation,
                SemanticMutation::AddNode { .. } | SemanticMutation::AddAnimation { .. }
            ) && mutation
                .node_references()
                .into_iter()
                .all(|node| node.existing().is_some())
        }));
        self.with_recoverable_extension(Some(true), move |transaction, _store| {
            transaction.mutations.extend(plan.mutations);
            Ok::<(), Infallible>(())
        })
        .map_err(|(prepared, error)| match error {
            PreparedExtensionError::Extension(never) => match never {},
            PreparedExtensionError::Preflight(error) => (prepared, error),
        })
    }

    /// Extend this prepared proof with one shared pending-node scene admission.
    ///
    /// This keeps the transaction-local allocator and prior mutation prefix in
    /// place. Both planner rejection and final preflight rejection truncate the
    /// attempted edge/order suffix before reconstructing the original proof.
    pub(crate) fn with_pending_scene_admission(
        self,
        scene_root: SemanticNodeId,
        admitted: &[SemanticTransactionNodeRef],
    ) -> Result<Self, (Box<Self>, crate::PreparedSemanticMembershipErrorKind)> {
        self.with_recoverable_extension(Some(true), |transaction, store| {
            crate::stage_semantic_scene_admission(store, scene_root, admitted, transaction)
        })
        .map_err(|(prepared, error)| match error {
            PreparedExtensionError::Extension(error) => (
                prepared,
                crate::PreparedSemanticMembershipErrorKind::Operation(error),
            ),
            PreparedExtensionError::Preflight(error) => (
                prepared,
                crate::PreparedSemanticMembershipErrorKind::Transaction(error),
            ),
        })
    }

    fn preflight_parts(
        transaction: &SemanticMutationTransaction,
        store: &SemanticStore,
    ) -> Result<PreparedTransactionParts, SemanticMutationTransactionError> {
        let preflight = transaction.preflight(store)?;
        let next_revision = if preflight.changed.iter().any(|changed| *changed) {
            Some(
                store
                    .scene_revision()
                    .checked_next()
                    .ok_or(SemanticMutationTransactionError::SceneRevisionExhausted)?,
            )
        } else {
            None
        };
        let tokens = transaction.mutations.iter().filter_map(|mutation| match mutation {
            SemanticMutation::AddNode { token, .. } if !preflight.removed_pending.contains(token) => {
                Some(*token)
            }
            SemanticMutation::AddAnimation { token, animation }
                if !preflight.removed_pending.contains(token)
                    && !animation.intent().node_references().any(|reference|
                        matches!(reference, SemanticTransactionNodeRef::Pending(dependency) if preflight.removed_pending.contains(&dependency))) =>
            {
                Some(*token)
            }
            _ => None,
        });
        let planned_nodes = tokens.zip(store.preview_node_allocations()).collect();
        Ok(PreparedTransactionParts {
            preflight,
            next_revision,
            planned_nodes,
        })
    }

    /// Allocator-derived identity for fallible execution preparation under this
    /// exclusive borrow. Pending identities are not published handles and must not
    /// escape preparation; commit verifies and returns the same allocator result.
    pub fn planned_node_id(
        &self,
        node: impl Into<SemanticTransactionNodeRef>,
    ) -> Option<SemanticNodeId> {
        let node = node.into();
        if self.node_is_removed(node) {
            return None;
        }
        match node {
            SemanticTransactionNodeRef::Existing(node) => self.store.node(node).map(|_| node),
            SemanticTransactionNodeRef::Pending(token) => self.planned_nodes.get(&token).copied(),
        }
    }

    /// Extend this prepared proof with authored writes to pending nodes while
    /// retaining the exact prior proof when the added writes fail preflight.
    ///
    /// Callback-local construction uses this for a phase-bound object before it
    /// has a durable semantic identity. The closure appends ordinary semantic
    /// transaction mutations; it does not introduce a second patch vocabulary.
    pub fn with_pending_object_update(
        self,
        update: impl FnOnce(&mut SemanticMutationTransaction),
    ) -> Result<Self, (Box<Self>, SemanticMutationTransactionError)> {
        // Pending-only coalescing can replace a mutation that was already in
        // the prefix. Keep a bounded exact snapshot so a late invalid update
        // restores that declaration as well as appended suffix mutations.
        // Local-token allocation intentionally remains monotonic on recovery.
        let original_mutations = self.transaction.mutations.clone();
        self.with_recoverable_extension(None, |transaction, _store| {
            update(transaction);
            Ok::<(), Infallible>(())
        })
        .map_err(|(mut prepared, error)| {
            prepared.transaction.mutations = original_mutations;
            match error {
                PreparedExtensionError::Extension(never) => match never {},
                PreparedExtensionError::Preflight(error) => (prepared, error),
            }
        })
    }

    /// Extend this proof with transaction-owned resource content while keeping
    /// a caught construction/preflight error recoverable. Fresh raw payloads are
    /// truncated on error; their token allocation remains monotonic so an escaped
    /// rejected token cannot name a later declaration.
    pub fn with_pending_resource_object<T, E>(
        self,
        extend: impl FnOnce(&mut SemanticMutationTransaction) -> Result<T, E>,
    ) -> Result<(Self, T), (Box<Self>, PendingResourceExtensionError<E>)> {
        // This closure accepts the transaction's normal mutable vocabulary, so
        // it may coalesce an earlier provisional write before returning an
        // error. Preserve that exact prefix as well as the helper's resource
        // suffix rollback.
        let original_mutations = self.transaction.mutations.clone();
        let mut value = None;
        self.with_recoverable_extension(None, |transaction, _store| {
            value = Some(extend(transaction)?);
            Ok(())
        })
        .map(|prepared| {
            (
                prepared,
                value.expect("successful resource extension returns its staged value"),
            )
        })
        .map_err(|(mut prepared, error)| {
            prepared.transaction.mutations = original_mutations;
            let error = match error {
                PreparedExtensionError::Extension(error) => {
                    PendingResourceExtensionError::Extension(error)
                }
                PreparedExtensionError::Preflight(error) => {
                    PendingResourceExtensionError::Preflight(error)
                }
            };
            (prepared, error)
        })
    }

    /// Materialize every live transaction-local path payload inside the existing
    /// scoped geometry admission boundary, then invoke one final publication.
    ///
    /// This consuming seam is intentionally terminal: the callback/runtime
    /// publication may consume lowering proof before reporting an error. Fresh
    /// paths are removed by [`SemanticStore::with_geometry_paths`] on any
    /// materialization, preflight, or publication failure; no durable resource
    /// exists before this scope starts. Canceled pending nodes are excluded and
    /// their raw payloads are simply dropped.
    pub fn with_pending_geometry_paths<T, E>(
        self,
        publish: impl for<'scope> FnOnce(PreparedSemanticMutationTransaction<'scope>) -> Result<T, E>,
    ) -> Result<T, PendingGeometryPublicationError<E>> {
        let Self {
            store,
            mut transaction,
            preflight,
            next_revision,
            planned_nodes,
        } = self;
        let tokens = Self::pending_path_resource_tokens(&preflight);
        if tokens.is_empty() {
            return publish(Self {
                store,
                transaction,
                preflight,
                next_revision,
                planned_nodes,
            })
            .map_err(PendingGeometryPublicationError::Publication);
        }
        let payloads = transaction.take_pending_geometry_paths(&tokens);
        debug_assert_eq!(payloads.len(), tokens.len());
        let resource_tokens = payloads.iter().map(|(token, _)| *token).collect::<Vec<_>>();
        let final_states = preflight.staged_pending_paths.clone();
        store.with_geometry_paths(
            payloads.into_iter().map(|(_, path)| path),
            move |store, handles| {
                let resources = resource_tokens
                    .iter()
                    .copied()
                    .zip(handles.iter().copied())
                    .collect::<HashMap<_, _>>();
                transaction.materialize_pending_geometry_paths(&resources, &final_states);
                let prepared =
                    PreparedSemanticMutationTransaction::new_recoverable(transaction, store)
                        .map_err(|(_, error)| {
                            PendingGeometryPublicationError::Transaction(error)
                        })?;
                publish(prepared).map_err(PendingGeometryPublicationError::Publication)
            },
        )
    }

    /// Re-preflight this still-unpublished batch with compiler-derived scalar tracks.
    ///
    /// This narrow consuming extension keeps animation declarations, object mutations,
    /// and their scheduled scalar leaves under one eventual semantic commit.
    pub fn with_scalar_signal_tracks(
        self,
        tracks: impl IntoIterator<Item = SemanticScalarSignalTrack>,
    ) -> Result<Self, SemanticMutationTransactionError> {
        let Self {
            store,
            mut transaction,
            ..
        } = self;
        for track in tracks {
            transaction.add_scalar_signal_track_with_time_map(
                track.signal(),
                track.from(),
                track.to(),
                track.timing(),
                track.time_map().clone(),
            );
        }
        Self::new(transaction, store)
    }

    /// Re-preflight this unpublished batch after resolving metadata against a
    /// transaction-local allocator identity.
    ///
    /// Composite resource authors use this narrow extension when a newly
    /// created DecimalNumber binds to a signal created by the same transaction.
    /// The allocator remains exclusively borrowed, so the planned signal id is
    /// stable across this second preflight and still cannot escape publication.
    pub fn with_decimal_number(
        self,
        object: impl Into<SemanticTransactionNodeRef>,
        number: crate::SemanticDecimalNumber,
    ) -> Result<Self, SemanticMutationTransactionError> {
        let Self {
            store,
            mut transaction,
            ..
        } = self;
        transaction.replace_decimal_number(object, number);
        Self::new(transaction, store)
    }

    /// All submitted mutations, preserving original indices and exact no-ops.
    pub fn mutations(&self) -> &[SemanticMutation] {
        self.transaction.mutations()
    }

    /// Potentially changing mutations in their validated commit order.
    ///
    /// No-ops resolved during preflight are excluded. Family reordering can still
    /// resolve to a no-op at commit without materializing sibling order here.
    pub fn candidate_mutations(&self) -> impl Iterator<Item = &SemanticMutation> {
        self.transaction
            .mutations
            .iter()
            .zip(&self.preflight.changed)
            .filter_map(|(mutation, changed)| changed.then_some(mutation))
    }

    /// Staged callback registrations computed by the semantic transaction's own
    /// preflight. Compiler preparation reads these rather than reimplementing
    /// updater insertion, occurrence identity, or interval-closing semantics.
    pub fn proposed_updater_registrations(
        &self,
        target: SemanticNodeId,
    ) -> Option<&[SemanticUpdaterRegistration]> {
        self.preflight
            .staged_updaters
            .get(&target.into())
            .map(Vec::as_slice)
    }

    /// Reserved candidate revision, or the current revision when no candidates exist.
    ///
    /// Commit can retain the current revision if all candidates resolve to no-ops
    /// (for example, an already-positioned family reorder). Consumers publishing
    /// before commit must restrict admission to mutation classes with exact
    /// preflight effects, as live value publication does.
    pub fn proposed_scene_revision(&self) -> SceneRevision {
        self.next_revision
            .unwrap_or_else(|| self.store.scene_revision())
    }

    /// Existing objects whose shared spatial anchor is cleared by the staged
    /// structural removal. These rows require local spatial-state publication.
    pub fn spatial_anchor_cleared_owners(&self) -> &[SemanticNodeId] {
        &self.preflight.spatial_anchor_cleared
    }

    /// Proposed presentation states, grouped in first changed-object order.
    ///
    /// This derived overlay applies object `SetZIndex`, `SetProperty`, `ReplaceStyle`,
    /// and `ReplaceContent`. It is not a structural or subscription overlay; consumers
    /// must separately check which mutation classes they support. One pass over
    /// the batch clones only the affected object states, never the store or arena.
    pub fn object_updates(
        &self,
    ) -> impl Iterator<Item = (SemanticNodeId, SemanticObjectState)> + '_ {
        let changed_objects: HashSet<_> = self
            .candidate_mutations()
            .filter_map(|mutation| match mutation {
                SemanticMutation::SetZIndex { node: object, .. }
                | SemanticMutation::SetProperty { object, .. }
                | SemanticMutation::SetObjectTransform { object, .. }
                | SemanticMutation::SetClickIndicate { object, .. }
                | SemanticMutation::SetSpatialCompositionDomain { object, .. }
                | SemanticMutation::SetCameraProfile { object, .. }
                | SemanticMutation::SetCameraMotions { object, .. }
                | SemanticMutation::ReplaceStyle { object, .. }
                | SemanticMutation::ReplaceContent { object, .. }
                | SemanticMutation::ReplaceDecimalNumber { object, .. }
                | SemanticMutation::ReplaceTextPresentationBaseline { object, .. } => Some(*object),
                _ => None,
            })
            .chain(
                self.preflight
                    .spatial_anchor_cleared
                    .iter()
                    .copied()
                    .map(SemanticTransactionNodeRef::Existing),
            )
            .collect();
        self.preflight
            .staged_object_order
            .iter()
            .filter_map(move |node| {
                let SemanticTransactionNodeRef::Existing(node_id) = node else {
                    return None;
                };
                if !changed_objects.contains(node) {
                    return None;
                }
                let mut state = self.preflight.staged_objects[node].clone();
                if let Some((domain, Some(anchor))) = self
                    .transaction
                    .mutations
                    .iter()
                    .rev()
                    .find_map(|mutation| match mutation {
                        SemanticMutation::SetSpatialCompositionDomain {
                            object: SemanticTransactionNodeRef::Existing(object),
                            domain,
                            anchor_family: Some(anchor),
                        } if object == node_id => Some((*domain, Some(*anchor))),
                        SemanticMutation::SetSpatialCompositionDomain {
                            object: SemanticTransactionNodeRef::Existing(object),
                            domain,
                            anchor_family: None,
                        } if object == node_id => Some((*domain, None)),
                        _ => None,
                    })
                {
                    let anchor = resolve_node_ref(anchor, &self.planned_nodes);
                    state
                        .set_spatial_composition_domain_with_anchor(domain, Some(anchor))
                        .expect("preflight validated resolved spatial anchor");
                }
                Some((*node_id, state))
            })
    }

    /// Read the final staged authored object state without publishing the batch.
    pub fn object_state(
        &self,
        object: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<&SemanticObjectState, SemanticTransactionReadError> {
        let object = object.into();
        if let SemanticTransactionNodeRef::Existing(node) = object {
            if self.preflight.removed_existing.contains(&node) {
                return Err(SemanticTransactionReadError::RemovedExistingNode(node));
            }
        }
        if let SemanticTransactionNodeRef::Pending(token) = object {
            self.validate_read_token(token)?;
            if self.preflight.removed_pending.contains(&token) {
                return Err(SemanticTransactionReadError::RemovedPendingNode(token));
            }
        }
        if let Some(state) = self.preflight.staged_objects.get(&object) {
            return Ok(state);
        }
        match object {
            SemanticTransactionNodeRef::Existing(object) => self
                .store
                .semantic_object_state_checked(object)
                .map_err(SemanticTransactionReadError::Existing),
            SemanticTransactionNodeRef::Pending(token) => match self.pending_creation(token) {
                Some(SemanticNodeCreation::Object { state, .. }) => Ok(state),
                Some(
                    SemanticNodeCreation::PendingPathObject { .. }
                    | SemanticNodeCreation::Family { .. }
                    | SemanticNodeCreation::Signal { .. },
                ) => Err(SemanticTransactionReadError::NotObject(object)),
                None if self.preflight.pending_animations.contains_key(&token) => {
                    Err(SemanticTransactionReadError::NotObject(object))
                }
                None => Err(SemanticTransactionReadError::UnknownPendingNode(token)),
            },
        }
    }

    fn pending_path_state(
        &self,
        object: SemanticTransactionNodeRef,
    ) -> Result<&super::node_addition::SemanticPendingPathObject, SemanticTransactionReadError>
    {
        let SemanticTransactionNodeRef::Pending(token) = object else {
            return Err(SemanticTransactionReadError::NotPendingGeometry(object));
        };
        self.validate_read_token(token)?;
        if self.preflight.removed_pending.contains(&token) {
            return Err(SemanticTransactionReadError::RemovedPendingNode(token));
        }
        self.preflight
            .staged_pending_paths
            .get(&token)
            .ok_or(SemanticTransactionReadError::NotPendingGeometry(object))
    }

    /// Read the true staged transform for a transaction-only path constructor.
    /// Unlike `object_state`, this does not fabricate durable content before
    /// resource admission.
    pub fn pending_path_transform(
        &self,
        object: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<crate::SemanticTransform2_5D, SemanticTransactionReadError> {
        Ok(self.pending_path_state(object.into())?.transform)
    }

    /// Read the true staged style for a transaction-only path constructor.
    pub fn pending_path_style(
        &self,
        object: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<crate::SemanticStyle, SemanticTransactionReadError> {
        Ok(self.pending_path_state(object.into())?.style.clone())
    }

    /// Read the true staged painter priority for a transaction-only path constructor.
    pub fn pending_path_z_index(
        &self,
        object: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<f64, SemanticTransactionReadError> {
        Ok(self.pending_path_state(object.into())?.z_index)
    }

    /// Resolve the actual transaction-owned vector path for one pending object.
    ///
    /// This is intentionally separate from durable object-state getters: a
    /// pending resource token is never a store handle. Callers that need path
    /// data can read this real staged payload; ordinary lowered/runtime queries
    /// remain unavailable until materialization.
    pub fn pending_geometry_path(
        &self,
        object: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<&crate::VectorPath, SemanticTransactionReadError> {
        let object = object.into();
        let state = self.pending_path_state(object)?;
        self.transaction
            .pending_geometry_path(state.resource())
            .ok_or(SemanticTransactionReadError::UnknownPendingGeometry(object))
    }

    /// Return conservative local bounds from the real staged path payload.
    /// Empty paths have no bounds, matching ordinary [`VectorPath`] semantics.
    pub fn pending_geometry_local_bounds(
        &self,
        object: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<Option<crate::Rect>, SemanticTransactionReadError> {
        Ok(self.pending_geometry_path(object)?.conservative_bounds())
    }

    /// Read proposed object or family painter priority without committing it.
    pub fn z_index(
        &self,
        node: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<f64, SemanticTransactionReadError> {
        let node = node.into();
        match node {
            SemanticTransactionNodeRef::Existing(id)
                if self.preflight.removed_existing.contains(&id) =>
            {
                return Err(SemanticTransactionReadError::RemovedExistingNode(id));
            }
            SemanticTransactionNodeRef::Pending(token) => {
                self.validate_read_token(token)?;
                if self.preflight.removed_pending.contains(&token) {
                    return Err(SemanticTransactionReadError::RemovedPendingNode(token));
                }
            }
            _ => {}
        }
        if let Some(value) = self.preflight.staged_family_z.get(&node) {
            return Ok(*value);
        }
        match node {
            SemanticTransactionNodeRef::Existing(id) => {
                if let Some(crate::SemanticNodeKind::Family(presentation)) =
                    self.store.node(id).map(|node| node.kind())
                {
                    return Ok(presentation.z_index);
                }
            }
            SemanticTransactionNodeRef::Pending(token)
                if matches!(
                    self.pending_creation(token),
                    Some(SemanticNodeCreation::Family { .. })
                ) =>
            {
                return Ok(0.0);
            }
            SemanticTransactionNodeRef::Pending(token)
                if self.preflight.staged_pending_paths.contains_key(&token) =>
            {
                return self.pending_path_z_index(node);
            }
            _ => {}
        }
        self.object_state(node).map(|state| state.z_index())
    }

    /// Clone the final staged object state with the insertion order it will receive
    /// if this transaction commits. Existing identities preserve their authored
    /// order; pending objects are numbered in allocation order without reserving a
    /// semantic identity or mutating the store.
    pub fn proposed_object_state(
        &self,
        object: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<SemanticObjectState, SemanticTransactionReadError> {
        let object = object.into();
        let mut state = self.object_state(object)?.clone();
        let SemanticTransactionNodeRef::Pending(token) = object else {
            return Ok(state);
        };
        let mut insertion_order = self.store.next_insertion_order();
        for mutation in self.transaction.mutations() {
            let SemanticMutation::AddNode {
                token: candidate,
                creation:
                    SemanticNodeCreation::Object { .. } | SemanticNodeCreation::PendingPathObject { .. },
            } = mutation
            else {
                continue;
            };
            if self.preflight.removed_pending.contains(candidate) {
                continue;
            }
            if *candidate == token {
                state.assign_insertion_order(insertion_order);
                return Ok(state);
            }
            insertion_order = insertion_order
                .checked_add(1)
                .expect("preflighted semantic insertion order must remain available");
        }
        Err(SemanticTransactionReadError::UnknownPendingNode(token))
    }

    pub fn node_is_removed(&self, node: impl Into<SemanticTransactionNodeRef>) -> bool {
        self.is_removed_ref(node.into())
    }

    /// Read one staged animation declaration without assigning it a permanent
    /// semantic identity. Its intent retains transaction-local node references.
    pub fn pending_animation(
        &self,
        token: SemanticLocalNodeToken,
    ) -> Result<&SemanticTransactionAnimation, SemanticTransactionReadError> {
        self.validate_read_token(token)?;
        if self.preflight.removed_pending.contains(&token) {
            return Err(SemanticTransactionReadError::RemovedPendingNode(token));
        }
        if let Some(animation) = self.preflight.pending_animations.get(&token) {
            return Ok(animation);
        }
        if self.preflight.pending_creations.contains_key(&token) {
            return Err(SemanticTransactionReadError::NotAnimation(token.into()));
        }
        Err(SemanticTransactionReadError::UnknownPendingNode(token))
    }

    /// Read final direct family order through the transaction-local overlay.
    pub fn family_members(
        &self,
        family: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<Vec<SemanticTransactionNodeRef>, SemanticTransactionReadError> {
        let family = family.into();
        if let SemanticTransactionNodeRef::Existing(node) = family {
            if self.preflight.removed_existing.contains(&node) {
                return Err(SemanticTransactionReadError::RemovedExistingNode(node));
            }
        }
        if let SemanticTransactionNodeRef::Pending(token) = family {
            self.validate_read_token(token)?;
            if self.preflight.removed_pending.contains(&token) {
                return Err(SemanticTransactionReadError::RemovedPendingNode(token));
            }
        }
        match family {
            SemanticTransactionNodeRef::Existing(family) => {
                let node = self
                    .store
                    .node(family)
                    .ok_or(SemanticTransactionReadError::UnknownExistingNode(family))?;
                if !matches!(node.kind(), SemanticNodeKind::Family(_)) {
                    return Err(SemanticTransactionReadError::NotFamily(family.into()));
                }
                Ok(self
                    .preflight
                    .family_edges
                    .members_for_read(self.store, family.into())
                    .into_iter()
                    .filter(|member| !self.is_removed_ref(*member))
                    .collect())
            }
            SemanticTransactionNodeRef::Pending(token) => match self.pending_creation(token) {
                Some(SemanticNodeCreation::Family { .. }) => Ok(self
                    .preflight
                    .family_edges
                    .members_for_read(self.store, family)
                    .into_iter()
                    .filter(|member| !self.is_removed_ref(*member))
                    .collect()),
                Some(
                    SemanticNodeCreation::Object { .. }
                    | SemanticNodeCreation::PendingPathObject { .. }
                    | SemanticNodeCreation::Signal { .. },
                ) => Err(SemanticTransactionReadError::NotFamily(family)),
                None if self.preflight.pending_animations.contains_key(&token) => {
                    Err(SemanticTransactionReadError::NotFamily(family))
                }
                None => Err(SemanticTransactionReadError::UnknownPendingNode(token)),
            },
        }
    }

    /// Query one staged direct family edge without materializing that family's
    /// complete order.  Local structural planners use these adjacency reads for
    /// large scene roots.
    pub(crate) fn family_contains_existing(
        &self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<bool, SemanticTransactionReadError> {
        self.validate_existing_family(family)?;
        self.validate_existing_authoring_node(member)?;
        Ok(self
            .preflight
            .family_edges
            .contains_existing(self.store, family, member))
    }

    /// First member in final staged order, without cloning an unrelated root.
    pub(crate) fn family_first_member_existing(
        &self,
        family: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticTransactionReadError> {
        self.validate_existing_family(family)?;
        self.preflight
            .family_edges
            .first_existing(self.store, family)
    }

    /// Next member in final staged order, without cloning an unrelated root.
    pub(crate) fn family_next_member_existing(
        &self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticTransactionReadError> {
        self.validate_existing_family(family)?;
        self.validate_existing_authoring_node(member)?;
        self.preflight
            .family_edges
            .next_existing(self.store, family, member)
    }

    /// Previous member in final staged order, without cloning an unrelated root.
    pub(crate) fn family_previous_member_existing(
        &self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticTransactionReadError> {
        self.validate_existing_family(family)?;
        self.validate_existing_authoring_node(member)?;
        self.preflight
            .family_edges
            .previous_existing(self.store, family, member)
    }

    pub(crate) fn staged_parent_additions_existing(
        &self,
        member: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticTransactionReadError> {
        self.validate_existing_authoring_node(member)?;
        self.preflight.family_edges.added_parents_existing(member)
    }

    /// Read final foreground declarations without inspecting display membership.
    /// Structural deletion removes soft references in the staged view as well
    /// as at commit; abandoned preparation leaves the published list untouched.
    pub fn foreground_members(
        &self,
        scope: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<Vec<SemanticTransactionNodeRef>, SemanticTransactionReadError> {
        let scope = scope.into();
        let published = match scope {
            SemanticTransactionNodeRef::Existing(id) => {
                if self.preflight.removed_existing.contains(&id) {
                    return Err(SemanticTransactionReadError::RemovedExistingNode(id));
                }
                let node = self
                    .store
                    .node(id)
                    .ok_or(SemanticTransactionReadError::UnknownExistingNode(id))?;
                if !matches!(node.kind(), SemanticNodeKind::Family(_)) {
                    return Err(SemanticTransactionReadError::NotFamily(scope));
                }
                node.foreground_members()
            }
            SemanticTransactionNodeRef::Pending(token) => {
                self.validate_read_token(token)?;
                if self.preflight.removed_pending.contains(&token) {
                    return Err(SemanticTransactionReadError::RemovedPendingNode(token));
                }
                match self.pending_creation(token) {
                    Some(SemanticNodeCreation::Family { .. }) => &[][..],
                    Some(_) => return Err(SemanticTransactionReadError::NotFamily(scope)),
                    None if self.preflight.pending_animations.contains_key(&token) => {
                        return Err(SemanticTransactionReadError::NotFamily(scope));
                    }
                    None => return Err(SemanticTransactionReadError::UnknownPendingNode(token)),
                }
            }
        };
        let mut members = self
            .preflight
            .staged_foreground
            .get(&scope)
            .cloned()
            .unwrap_or_else(|| published.iter().copied().map(Into::into).collect());
        members.retain(|member| !self.is_removed_ref(*member));
        Ok(members)
    }

    /// Read final signal associations for one family root through the staged view.
    pub fn scoped_signals(
        &self,
        scope: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<Vec<SemanticTransactionNodeRef>, SemanticTransactionReadError> {
        let scope = scope.into();
        match scope {
            SemanticTransactionNodeRef::Existing(node)
                if self.preflight.removed_existing.contains(&node) =>
            {
                return Err(SemanticTransactionReadError::RemovedExistingNode(node));
            }
            SemanticTransactionNodeRef::Pending(token) => {
                self.validate_read_token(token)?;
                if self.preflight.removed_pending.contains(&token) {
                    return Err(SemanticTransactionReadError::RemovedPendingNode(token));
                }
            }
            _ => {}
        }
        let mut scoped = match scope {
            SemanticTransactionNodeRef::Existing(scope) => {
                let node = self
                    .store
                    .node(scope)
                    .ok_or(SemanticTransactionReadError::UnknownExistingNode(scope))?;
                if !matches!(node.kind(), SemanticNodeKind::Family(_)) {
                    return Err(SemanticTransactionReadError::NotFamily(scope.into()));
                }
                node.scoped_signals()
                    .iter()
                    .copied()
                    .map(Into::into)
                    .collect()
            }
            SemanticTransactionNodeRef::Pending(token) => match self.pending_creation(token) {
                Some(SemanticNodeCreation::Family { .. }) => Vec::new(),
                Some(_) => return Err(SemanticTransactionReadError::NotFamily(scope)),
                None => return Err(SemanticTransactionReadError::UnknownPendingNode(token)),
            },
        };
        scoped.extend(
            self.preflight
                .staged_signal_scope_additions
                .iter()
                .filter_map(|(candidate_scope, signal)| {
                    (*candidate_scope == scope).then_some(*signal)
                }),
        );
        scoped.retain(|signal| !self.is_removed_ref(*signal));
        Ok(scoped)
    }

    fn validate_read_token(
        &self,
        token: SemanticLocalNodeToken,
    ) -> Result<(), SemanticTransactionReadError> {
        if !token.belongs_to(self.transaction.id) {
            return Err(SemanticTransactionReadError::PendingNodeFromDifferentTransaction(token));
        }
        Ok(())
    }

    fn validate_existing_family(
        &self,
        family: SemanticNodeId,
    ) -> Result<(), SemanticTransactionReadError> {
        if self.preflight.removed_existing.contains(&family) {
            return Err(SemanticTransactionReadError::RemovedExistingNode(family));
        }
        let node = self
            .store
            .node(family)
            .ok_or(SemanticTransactionReadError::UnknownExistingNode(family))?;
        if !matches!(node.kind(), SemanticNodeKind::Family(_)) {
            return Err(SemanticTransactionReadError::NotFamily(family.into()));
        }
        Ok(())
    }

    fn validate_existing_authoring_node(
        &self,
        node: SemanticNodeId,
    ) -> Result<(), SemanticTransactionReadError> {
        if self.preflight.removed_existing.contains(&node) {
            return Err(SemanticTransactionReadError::RemovedExistingNode(node));
        }
        let existing = self
            .store
            .node(node)
            .ok_or(SemanticTransactionReadError::UnknownExistingNode(node))?;
        let authoring = matches!(existing.kind(), SemanticNodeKind::Family(_))
            || matches!(existing.kind(), SemanticNodeKind::AuthoringObject)
                && existing.semantic_object_state().is_some();
        if !authoring {
            return Err(SemanticTransactionReadError::NotObject(node.into()));
        }
        Ok(())
    }

    fn pending_creation(&self, token: SemanticLocalNodeToken) -> Option<&SemanticNodeCreation> {
        self.preflight.pending_creations.get(&token)
    }

    fn is_removed_ref(&self, node: SemanticTransactionNodeRef) -> bool {
        match node {
            SemanticTransactionNodeRef::Existing(node) => {
                self.preflight.removed_existing.contains(&node)
            }
            SemanticTransactionNodeRef::Pending(token) => {
                self.preflight.removed_pending.contains(&token)
            }
        }
    }

    /// Publish the validated batch exactly once, without another preflight.
    pub fn commit(self) -> SemanticMutationTransactionResult {
        self.commit_with_store().0
    }

    /// Publish the validated batch and return its still-exclusively-borrowed store.
    ///
    /// This supports one semantic-plus-execution publication suffix: callers can
    /// prepare compiler/runtime work while the transaction holds the store, commit
    /// semantic identity once, then bind those local names without reacquiring or
    /// revalidating a separate store reference.
    pub fn commit_with_store(self) -> (SemanticMutationTransactionResult, &'a mut SemanticStore) {
        let Self {
            store,
            mut transaction,
            preflight,
            next_revision,
            planned_nodes,
        } = self;
        // A direct semantic commit has no later fallible lowering step, so it
        // materializes live path payloads immediately before writing nodes. The
        // combined semantic/runtime path instead uses `with_pending_geometry_paths`
        // to retain the same admission scope across fallible lowering/publication.
        let tokens = Self::pending_path_resource_tokens(&preflight);
        if !tokens.is_empty() {
            let payloads = transaction.take_pending_geometry_paths(&tokens);
            let resources = payloads
                .into_iter()
                .map(|(token, path)| (token, store.insert_preflighted_geometry_path(path)))
                .collect::<HashMap<_, _>>();
            transaction
                .materialize_pending_geometry_paths(&resources, &preflight.staged_pending_paths);
        }
        let mut impacts = Vec::with_capacity(transaction.mutations.len());
        let mut written_slots = HashSet::with_capacity(transaction.mutations.len());
        let mut pending_source_assignments = Vec::new();
        let mut committed_nodes = HashMap::new();
        store.begin_semantic_resource_reclamation_defer();
        for mutation in &transaction.mutations {
            match mutation {
                SemanticMutation::AddNode { token, creation } => {
                    if !planned_nodes.contains_key(token) {
                        continue;
                    }
                    let (node, source_identity) = commit_add_node(store, creation.clone());
                    assert_eq!(
                        planned_nodes.get(token),
                        Some(&node),
                        "prepared allocator identity changed"
                    );
                    committed_nodes.insert(*token, node);
                    written_slots.insert(node);
                    if let Some(source_identity) = source_identity {
                        pending_source_assignments.push((node, source_identity));
                    }
                }
                SemanticMutation::AddAnimation { token, animation } => {
                    if !planned_nodes.contains_key(token) {
                        continue;
                    }
                    let state = animation.resolve(&committed_nodes);
                    let node = commit_add_animation(store, &state);
                    assert_eq!(
                        planned_nodes.get(token),
                        Some(&node),
                        "prepared allocator identity changed"
                    );
                    committed_nodes.insert(*token, node);
                    written_slots.insert(node);
                }
                _ => {}
            }
        }
        for (mutation, changed) in transaction.mutations.into_iter().zip(preflight.changed) {
            if !changed {
                continue;
            }
            match mutation {
                SemanticMutation::SetSignal { signal, value } => {
                    let changed = store
                        .set_semantic_signal_source(signal, SemanticSignalSource::Input(value))
                        .expect(
                            "preflighted input signal update must remain valid while transaction owns the semantic store",
                        );
                    debug_assert!(changed);
                    written_slots.insert(signal);
                    impacts.push(SemanticMutationImpact::SignalValue { signal });
                }
                SemanticMutation::AddScalarSignalTrack {
                    signal,
                    from,
                    to,
                    timing,
                    time_map,
                } => {
                    store.add_validated_semantic_scalar_signal_track(
                        SemanticScalarSignalTrack::new_with_time_map(
                            signal, from, to, timing, time_map,
                        ),
                    );
                    written_slots.insert(signal);
                    impacts.push(SemanticMutationImpact::SignalTimeline { signal });
                }
                SemanticMutation::SetScalarSignalAt {
                    signal,
                    value,
                    time,
                } => {
                    store.add_validated_semantic_scalar_signal_hold(SemanticScalarSignalHold::new(
                        signal, value, time,
                    ));
                    written_slots.insert(signal);
                    impacts.push(SemanticMutationImpact::SignalTimeline { signal });
                }
                SemanticMutation::SetProperty {
                    object,
                    property,
                    value,
                } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    set_object_property(store, object, property, value);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectProperty { object, property });
                }
                SemanticMutation::SetObjectTransform { object, transform } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    store
                        .node_mut(object)
                        .and_then(|node| node.semantic_object_state_mut())
                        .expect("preflighted semantic object")
                        .set_transform(transform);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectTransform { object });
                }
                SemanticMutation::SetCameraMotions { object, motions } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    store
                        .node_mut(object)
                        .and_then(|node| node.semantic_object_state_mut())
                        .expect("preflighted semantic camera")
                        .set_camera_motions(motions)
                        .expect("preflighted camera motion history");
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::CameraMotions { object });
                }
                SemanticMutation::SetCameraProfile {
                    object,
                    profile,
                    near,
                    far,
                } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    store
                        .node_mut(object)
                        .and_then(|node| node.semantic_object_state_mut())
                        .expect("preflighted semantic object")
                        .set_camera_profile(profile, near, far)
                        .expect("preflighted camera profile");
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::CameraProfile { object });
                }
                SemanticMutation::SetSpatialCompositionDomain {
                    object,
                    domain,
                    anchor_family,
                } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    let anchor_family =
                        anchor_family.map(|anchor| resolve_node_ref(anchor, &committed_nodes));
                    store.unregister_semantic_references_for_owner(object);
                    store
                        .node_mut(object)
                        .and_then(|node| node.semantic_object_state_mut())
                        .expect("preflighted semantic object")
                        .set_spatial_composition_domain_with_anchor(domain, anchor_family)
                        .expect("preflight validated spatial composition domain");
                    store.register_semantic_references_for_owner(object);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::SpatialCompositionDomain { object });
                    if anchor_family.is_some()
                        || domain == crate::SemanticSpatialCompositionDomain::FixedOrientation
                    {
                        impacts.push(SemanticMutationImpact::SpatialAnchorChanged { object });
                    }
                }
                SemanticMutation::SetClickIndicate { object, binding } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    store
                        .node_mut(object)
                        .expect("preflighted semantic object")
                        .semantic_object_state_mut()
                        .expect("preflighted semantic object state")
                        .set_click_indicate(binding);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ClickIndicate { object });
                }
                SemanticMutation::ReplaceContent { object, content } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    store.replace_semantic_object_content(object, content);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectContent { object });
                }
                SemanticMutation::SetBarMetadata { object, metadata } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    store
                        .node_mut(object)
                        .and_then(|node| node.semantic_object_state_mut())
                        .expect("preflighted semantic object must remain valid while transaction owns the semantic store")
                        .set_bar_metadata(metadata);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::BarMetadata { object });
                }
                SemanticMutation::SetInset2DView {
                    object,
                    camera_frame,
                    capture_own_display,
                } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    let role = camera_frame.map_or(SemanticObjectRole::Ordinary, |camera| {
                        SemanticObjectRole::Inset2DView(
                            crate::SemanticInset2DViewRole::new(resolve_node_ref(
                                camera,
                                &committed_nodes,
                            ))
                            .capture_own_display(capture_own_display),
                        )
                    });
                    store.replace_semantic_object_role(object, role);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectRole { object });
                }
                SemanticMutation::ReplaceDecimalNumber { object, number } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    store.replace_semantic_decimal_number(object, number);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::DecimalNumber { object });
                }
                SemanticMutation::ReplaceTextPresentationBaseline { object, baseline } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    let state = store
                        .node_mut(object)
                        .expect("preflighted semantic object")
                        .semantic_object_state_mut()
                        .expect("preflighted semantic object state");
                    match baseline {
                        Some(baseline) => state.set_text_presentation_baseline(baseline),
                        None => state.clear_text_presentation_baseline(),
                    }
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::TextPresentationBaseline { object });
                }
                SemanticMutation::SetZIndex { node, value } => {
                    let node = resolve_node_ref(node, &committed_nodes);
                    store
                        .node_mut(node)
                        .expect("preflighted authoring node")
                        .set_z_index(value);
                    written_slots.insert(node);
                    impacts.push(SemanticMutationImpact::ZIndex { node });
                }
                SemanticMutation::ReplaceStyle { object, style } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    set_object_style(store, object, style);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::ObjectStyle { object });
                }
                SemanticMutation::ChangeSubscription {
                    object,
                    property,
                    signal,
                } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    set_object_subscription(store, object, property, signal);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::Subscription { object, property });
                }
                SemanticMutation::AddUpdater {
                    target,
                    callback,
                    active_from,
                    inactive_from,
                    endpoint_policy,
                    position,
                } => {
                    let target = resolve_node_ref(target, &committed_nodes);
                    let registration = SemanticUpdaterRegistration::with_endpoint_policy(
                        callback,
                        active_from,
                        inactive_from,
                        endpoint_policy,
                    )
                    .expect("preflighted updater activation interval remains valid");
                    store
                        .insert_semantic_updater_registration(target, registration, position)
                        .expect("preflighted updater insertion remains valid");
                    written_slots.insert(target);
                    impacts.push(SemanticMutationImpact::UpdaterRegistrations { target });
                }
                SemanticMutation::RemoveUpdater {
                    target,
                    callback,
                    inactive_from,
                } => {
                    let target = resolve_node_ref(target, &committed_nodes);
                    let closed = store
                        .close_first_semantic_updater_registration(target, callback, inactive_from)
                        .expect("preflighted updater removal remains valid");
                    debug_assert!(closed);
                    written_slots.insert(target);
                    impacts.push(SemanticMutationImpact::UpdaterRegistrations { target });
                }
                SemanticMutation::ClearUpdaters {
                    target,
                    inactive_from,
                } => {
                    let target = resolve_node_ref(target, &committed_nodes);
                    let closed = store
                        .close_all_semantic_updater_registrations(target, inactive_from)
                        .expect("preflighted updater clear remains valid");
                    debug_assert!(closed);
                    written_slots.insert(target);
                    impacts.push(SemanticMutationImpact::UpdaterRegistrations { target });
                }
                SemanticMutation::SetForegroundMembers { scope, members } => {
                    let scope = resolve_node_ref(scope, &committed_nodes);
                    let members = members
                        .into_iter()
                        .map(|member| resolve_node_ref(member, &committed_nodes))
                        .collect();
                    store.replace_semantic_foreground_members(scope, members);
                    written_slots.insert(scope);
                    impacts.push(SemanticMutationImpact::ForegroundMembers { scope });
                }
                SemanticMutation::SetGraphDeclaration { scope, graph } => {
                    let scope = resolve_node_ref(scope, &committed_nodes);
                    let vertices = graph
                        .vertices()
                        .iter()
                        .copied()
                        .map(|(id, vertex)| (id, resolve_node_ref(vertex, &committed_nodes)))
                        .collect::<Vec<_>>();
                    let edges = graph
                        .edges()
                        .iter()
                        .copied()
                        .map(|edge| {
                            let dependency = match edge.dependency() {
                                SemanticTransactionGraphEdgeDependency::Line => {
                                    SemanticGraphEdgeDependency::Line
                                }
                                SemanticTransactionGraphEdgeDependency::Arrow {
                                    end_tip,
                                    start_tip,
                                    policy,
                                } => SemanticGraphEdgeDependency::Arrow {
                                    end_tip: resolve_node_ref(end_tip, &committed_nodes),
                                    start_tip: start_tip
                                        .map(|tip| resolve_node_ref(tip, &committed_nodes)),
                                    policy,
                                },
                            };
                            SemanticGraphEdgeBinding::from_resolved(
                                edge.id(),
                                resolve_node_ref(edge.family(), &committed_nodes),
                                resolve_node_ref(edge.line(), &committed_nodes),
                                dependency,
                            )
                        })
                        .collect();
                    let graph = SemanticGraphDeclaration::from_resolved(
                        graph.topology().clone(),
                        vertices,
                        edges,
                    );
                    let previous = store
                        .replace_semantic_graph_declaration(scope, Some(graph))
                        .expect("preflighted graph scope remains a family");
                    // Replacements are explicitly validated against the final
                    // transaction overlay. Dropping the prior declaration here
                    // only changes graph authority; unrelated semantic nodes
                    // retain their identities and resources.
                    drop(previous);
                    written_slots.insert(scope);
                    impacts.push(SemanticMutationImpact::GraphDeclaration { scope });
                }
                SemanticMutation::SetTableLayout { scope, layout } => {
                    let scope = resolve_node_ref(scope, &committed_nodes);
                    store
                        .replace_semantic_table_layout(scope, layout)
                        .expect("preflighted table layout scope remains a family");
                    written_slots.insert(scope);
                }
                SemanticMutation::ScopeSignal { scope, signal } => {
                    let scope = resolve_node_ref(scope, &committed_nodes);
                    let signal = resolve_node_ref(signal, &committed_nodes);
                    let scoped = store
                        .scope_semantic_signal(scope, signal)
                        .expect("preflighted signal scope remains valid");
                    debug_assert!(scoped);
                    written_slots.insert(scope);
                    impacts.push(SemanticMutationImpact::SignalScoped { scope, signal });
                }
                SemanticMutation::AddMember { family, member } => {
                    let family = resolve_node_ref(family, &committed_nodes);
                    let member = resolve_node_ref(member, &committed_nodes);
                    store.add_member(family, member).expect(
                        "preflighted family add must remain valid while transaction owns the semantic store",
                    );
                    written_slots.insert(family);
                    written_slots.insert(member);
                    impacts.push(SemanticMutationImpact::FamilyMemberAdded { family, member });
                }
                SemanticMutation::RemoveMember { family, member } => {
                    let family = resolve_node_ref(family, &committed_nodes);
                    let member = resolve_node_ref(member, &committed_nodes);
                    let removed = store.remove_member(family, member).expect(
                        "preflighted family removal must remain valid while transaction owns the semantic store",
                    );
                    debug_assert!(removed);
                    written_slots.insert(family);
                    written_slots.insert(member);
                    impacts.push(SemanticMutationImpact::FamilyMemberRemoved { family, member });
                }
                SemanticMutation::ReorderMember {
                    family,
                    member,
                    before,
                } => {
                    let family = resolve_node_ref(family, &committed_nodes);
                    let member = resolve_node_ref(member, &committed_nodes);
                    let before = before.map(|node| resolve_node_ref(node, &committed_nodes));
                    let reordered = store.reorder_member(family, member, before).expect(
                        "preflighted family reorder must remain valid while transaction owns the semantic store",
                    );
                    if !reordered {
                        continue;
                    }
                    written_slots.insert(family);
                    impacts.push(SemanticMutationImpact::FamilyMemberReordered {
                        family,
                        member,
                        before,
                    });
                }
                SemanticMutation::AddNode { token, .. } => {
                    let node = committed_nodes[&token];
                    impacts.push(SemanticMutationImpact::NodeAdded { node });
                }
                SemanticMutation::AddAnimation { token, .. } => {
                    let animation = committed_nodes[&token];
                    impacts.push(SemanticMutationImpact::AnimationAdded { animation });
                }
                SemanticMutation::RemoveAnimation { animation }
                | SemanticMutation::RemoveNode {
                    node: SemanticTransactionNodeRef::Existing(animation),
                } => {
                    // An earlier explicit removal may have cascade-removed this
                    // node already. Preflight proved the handle was live at the
                    // transaction boundary; a cascade therefore satisfies this
                    // later structural mutation without duplicate impacts.
                    if store.node(animation).is_none() {
                        continue;
                    }
                    let outcome = store
                        .remove_node_with_reverse_cleanup(animation)
                        .expect("preflighted node removal must remain valid while transaction owns the store");
                    written_slots.extend(outcome.written_slots().iter().copied());
                    for effect in outcome.effects() {
                        match effect {
                            SemanticRemoveNodeEffect::NodeRemoved(node) => {
                                impacts.push(SemanticMutationImpact::NodeRemoved { node: *node });
                            }
                            SemanticRemoveNodeEffect::ForegroundMembersChanged { scope } => {
                                impacts.push(SemanticMutationImpact::ForegroundMembers {
                                    scope: *scope,
                                });
                            }
                            SemanticRemoveNodeEffect::SubscriptionRemoved { object, property } => {
                                impacts.push(SemanticMutationImpact::Subscription {
                                    object: *object,
                                    property: *property,
                                });
                            }
                            SemanticRemoveNodeEffect::ObjectRoleReplaced(object) => {
                                impacts
                                    .push(SemanticMutationImpact::ObjectRole { object: *object });
                            }
                            SemanticRemoveNodeEffect::SpatialAnchorCleared(object) => {
                                impacts.push(SemanticMutationImpact::SpatialAnchorChanged {
                                    object: *object,
                                });
                            }
                        }
                    }
                }
                SemanticMutation::RemoveNode {
                    node: SemanticTransactionNodeRef::Pending(_),
                } => unreachable!("pending removal cancels allocation during preflight"),
            }
        }

        for (node, source_identity) in pending_source_assignments {
            store
                .set_source_identity(node, Some(source_identity))
                .expect("preflighted source identity must be available after terminal removals");
        }
        store.set_last_mutation_writes(written_slots.len());
        if !written_slots.is_empty() {
            store.publish_scene_revision(next_revision.expect("changed transaction preflighted"));
        }
        store.end_semantic_resource_reclamation_defer();

        (
            SemanticMutationTransactionResult {
                impacts,
                committed_nodes,
            },
            store,
        )
    }
}

fn resolve_node_ref(
    node: SemanticTransactionNodeRef,
    committed: &HashMap<SemanticLocalNodeToken, SemanticNodeId>,
) -> SemanticNodeId {
    match node {
        SemanticTransactionNodeRef::Existing(node) => node,
        SemanticTransactionNodeRef::Pending(token) => committed[&token],
    }
}
