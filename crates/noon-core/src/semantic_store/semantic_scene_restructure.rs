use std::collections::{HashMap, HashSet};

use crate::{
    PreparedSemanticMutationTransaction, SemanticMutationTransaction, SemanticNode, SemanticNodeId,
    SemanticNodeKind, SemanticSceneOperationError, SemanticStore, SemanticTransactionNodeRef,
};

mod foreground;
pub use foreground::stage_semantic_foreground_removal;
#[cfg(test)]
mod foreground_tests;

/// One atomic edit of an explicit semantic scene-root family's ordered projection.
#[derive(Clone, Copy, Debug)]
pub enum SemanticSceneMembershipRequest<'a> {
    Add(&'a [SemanticNodeId]),
    /// Add to display membership and persist at the tail of subsequent adds.
    AddForeground(&'a [SemanticNodeId]),
    /// Remove persistence only; do not detach or reorder display members.
    RemoveForeground(&'a [SemanticNodeId]),
    BringToBack(&'a [SemanticNodeId]),
    Remove(&'a [SemanticNodeId]),
    Clear,
    Replace {
        old: SemanticNodeId,
        new: SemanticNodeId,
    },
}

/// A rejected prepared-membership stage together with the still-valid prior
/// transaction. Callers that handle an operation failure can continue staging
/// later callback operations or commit the work already prepared.
pub struct PreparedSemanticMembershipError<'a> {
    prepared: Box<PreparedSemanticMutationTransaction<'a>>,
    kind: PreparedSemanticMembershipErrorKind,
}

#[derive(Debug)]
pub enum PreparedSemanticMembershipErrorKind {
    Operation(SemanticSceneOperationError),
    Transaction(crate::SemanticMutationTransactionError),
}

impl<'a> PreparedSemanticMembershipError<'a> {
    /// Recover both the still-valid prior transaction and the reason that the
    /// additional membership operation was rejected. This lets a callback
    /// boundary report the typed failure without discarding its prepared proof.
    pub fn into_parts(
        self,
    ) -> (
        PreparedSemanticMutationTransaction<'a>,
        PreparedSemanticMembershipErrorKind,
    ) {
        (*self.prepared, self.kind)
    }

    /// Recover the prior prepared transaction after a caught staging error.
    pub fn into_prepared(self) -> PreparedSemanticMutationTransaction<'a> {
        *self.prepared
    }

    /// The operation or transaction preflight error that rejected this stage.
    pub fn kind(&self) -> &PreparedSemanticMembershipErrorKind {
        &self.kind
    }
}

impl std::fmt::Debug for PreparedSemanticMembershipError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSemanticMembershipError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for PreparedSemanticMembershipErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operation(error) => error.fmt(f),
            Self::Transaction(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for PreparedSemanticMembershipErrorKind {}

impl std::fmt::Display for PreparedSemanticMembershipError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}
impl std::error::Error for PreparedSemanticMembershipError<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.kind)
    }
}

#[derive(Clone, Copy)]
struct MembershipTarget {
    family: bool,
    scene_owned: bool,
}

/// Local relationship reads for the shared membership planner. Prepared reads
/// retain only changed family-edge adjacency; they never materialize a root.
trait MembershipView {
    fn target(&self, id: SemanticNodeId) -> Result<MembershipTarget, SemanticSceneOperationError>;
    fn contains(
        &self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<bool, SemanticSceneOperationError>;
    fn first(
        &self,
        family: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticSceneOperationError>;
    fn next(
        &self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticSceneOperationError>;
    fn previous(
        &self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticSceneOperationError>;
    fn parents(
        &self,
        node: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError>;
    fn foreground(
        &self,
        family: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError>;
}

struct StoreMembershipView<'a>(&'a SemanticStore);
impl MembershipView for StoreMembershipView<'_> {
    fn target(&self, id: SemanticNodeId) -> Result<MembershipTarget, SemanticSceneOperationError> {
        let node = target_node_checked(self.0, id)?;
        Ok(MembershipTarget {
            family: matches!(node.kind(), SemanticNodeKind::Family(_)),
            scene_owned: node.is_scene_owned(),
        })
    }
    fn contains(
        &self,
        f: SemanticNodeId,
        m: SemanticNodeId,
    ) -> Result<bool, SemanticSceneOperationError> {
        Ok(target_node_checked(self.0, f)?.contains_member(m))
    }
    fn first(
        &self,
        f: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticSceneOperationError> {
        Ok(target_node_checked(self.0, f)?.first_member())
    }
    fn next(
        &self,
        f: SemanticNodeId,
        m: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticSceneOperationError> {
        Ok(target_node_checked(self.0, f)?.next_member(m))
    }
    fn previous(
        &self,
        f: SemanticNodeId,
        m: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticSceneOperationError> {
        Ok(target_node_checked(self.0, f)?.previous_member(m))
    }
    fn parents(
        &self,
        n: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError> {
        Ok(target_node_checked(self.0, n)?.parents().to_vec())
    }
    fn foreground(
        &self,
        f: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError> {
        Ok(target_node_checked(self.0, f)?
            .foreground_members()
            .to_vec())
    }
}

struct PreparedMembershipView<'a, 'store> {
    prepared: &'a PreparedSemanticMutationTransaction<'store>,
}
impl MembershipView for PreparedMembershipView<'_, '_> {
    fn target(&self, id: SemanticNodeId) -> Result<MembershipTarget, SemanticSceneOperationError> {
        if self.prepared.node_is_removed(id) {
            return Err(SemanticSceneOperationError::UnknownNode(id));
        }
        StoreMembershipView(self.prepared.store()).target(id)
    }
    fn contains(
        &self,
        f: SemanticNodeId,
        m: SemanticNodeId,
    ) -> Result<bool, SemanticSceneOperationError> {
        self.prepared
            .family_contains_existing(f, m)
            .map_err(prepared_read_error)
    }
    fn first(
        &self,
        f: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticSceneOperationError> {
        self.prepared
            .family_first_member_existing(f)
            .map_err(prepared_read_error)
    }
    fn next(
        &self,
        f: SemanticNodeId,
        m: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticSceneOperationError> {
        self.prepared
            .family_next_member_existing(f, m)
            .map_err(prepared_read_error)
    }
    fn previous(
        &self,
        f: SemanticNodeId,
        m: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticSceneOperationError> {
        self.prepared
            .family_previous_member_existing(f, m)
            .map_err(prepared_read_error)
    }
    fn parents(
        &self,
        n: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError> {
        let mut parents = Vec::new();
        let mut seen = HashSet::new();
        for parent in StoreMembershipView(self.prepared.store()).parents(n)? {
            if self.contains(parent, n)? && seen.insert(parent) {
                parents.push(parent);
            }
        }
        for parent in self
            .prepared
            .staged_parent_additions_existing(n)
            .map_err(prepared_read_error)?
        {
            if seen.insert(parent) {
                parents.push(parent);
            }
        }
        Ok(parents)
    }
    fn foreground(
        &self,
        f: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError> {
        self.prepared
            .foreground_members(f)
            .map_err(prepared_read_error)?
            .into_iter()
            .map(|n| {
                n.existing().ok_or_else(|| {
                    SemanticSceneOperationError::InvalidPendingAdmission(match n {
                        SemanticTransactionNodeRef::Pending(t) => t,
                        SemanticTransactionNodeRef::Existing(_) => unreachable!(),
                    })
                })
            })
            .collect()
    }
}

fn prepared_read_error(error: crate::SemanticTransactionReadError) -> SemanticSceneOperationError {
    match error {
        crate::SemanticTransactionReadError::UnknownExistingNode(id)
        | crate::SemanticTransactionReadError::RemovedExistingNode(id) => {
            SemanticSceneOperationError::UnknownNode(id)
        }
        crate::SemanticTransactionReadError::NotFamily(SemanticTransactionNodeRef::Existing(
            id,
        )) => SemanticSceneOperationError::NotSemanticFamily(id),
        crate::SemanticTransactionReadError::Existing(error) => error,
        crate::SemanticTransactionReadError::PendingNodeFromDifferentTransaction(t)
        | crate::SemanticTransactionReadError::UnknownPendingNode(t)
        | crate::SemanticTransactionReadError::RemovedPendingNode(t)
        | crate::SemanticTransactionReadError::PendingMembershipAdjacency(t)
        | crate::SemanticTransactionReadError::NotFamily(SemanticTransactionNodeRef::Pending(t)) => {
            SemanticSceneOperationError::InvalidPendingAdmission(t)
        }
        crate::SemanticTransactionReadError::NotObject(SemanticTransactionNodeRef::Existing(
            id,
        ))
        | crate::SemanticTransactionReadError::NotAnimation(
            SemanticTransactionNodeRef::Existing(id),
        ) => SemanticSceneOperationError::NotSemanticAuthoringNode(id),
        crate::SemanticTransactionReadError::NotObject(SemanticTransactionNodeRef::Pending(t))
        | crate::SemanticTransactionReadError::NotAnimation(SemanticTransactionNodeRef::Pending(
            t,
        )) => SemanticSceneOperationError::InvalidPendingAdmission(t),
    }
}

/// Return whether `target` is reachable below one explicit semantic scene root.
///
/// Membership is derived from the authoritative family edges by walking only the
/// target's ancestor closure. Aliased paths are de-duplicated and no unrelated
/// scene roots or descendants are visited.
pub fn semantic_scene_root_contains(
    store: &SemanticStore,
    scene_root: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<bool, SemanticSceneOperationError> {
    let root = target_node_checked(store, scene_root)?;
    if !matches!(root.kind(), SemanticNodeKind::Family(_)) {
        return Err(SemanticSceneOperationError::NotSemanticFamily(scene_root));
    }
    target_node_checked(store, target)?;
    let mut visited = HashSet::new();
    let mut stack = vec![target];
    while let Some(node) = stack.pop() {
        if !visited.insert(node) {
            continue;
        }
        for &parent in target_node_checked(store, node)?.parents() {
            if parent == scene_root {
                return Ok(true);
            }
            stack.push(parent);
        }
    }
    Ok(false)
}

/// Plan a family-aware scene membership edit without mutating the semantic store.
///
/// Only affected root branches are removed/promoted/reordered. Family edges and
/// semantic identities remain unchanged, and callers can publish the returned
/// transaction through either detached authoring or the live execution session.
pub fn plan_semantic_scene_membership(
    store: &SemanticStore,
    scene_root: SemanticNodeId,
    request: SemanticSceneMembershipRequest<'_>,
) -> Result<SemanticMutationTransaction, SemanticSceneOperationError> {
    plan_membership_in_view(&StoreMembershipView(store), scene_root, request)
}

/// Plan one existing-handle membership operation through a prepared transaction.
/// The prepared view reads only affected family-edge adjacency and is intended for
/// ordered callback staging before one final semantic publication.
pub fn plan_prepared_semantic_scene_membership(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    scene_root: SemanticNodeId,
    request: SemanticSceneMembershipRequest<'_>,
) -> Result<SemanticMutationTransaction, SemanticSceneOperationError> {
    plan_membership_in_view(&PreparedMembershipView { prepared }, scene_root, request)
}

/// Extend one prepared transaction with an ordered existing-handle membership
/// operation. A rejected stage returns the prior unpublished proof unchanged.
pub fn stage_prepared_semantic_scene_membership<'a>(
    prepared: PreparedSemanticMutationTransaction<'a>,
    scene_root: SemanticNodeId,
    request: SemanticSceneMembershipRequest<'_>,
) -> Result<PreparedSemanticMutationTransaction<'a>, PreparedSemanticMembershipError<'a>> {
    let plan = match plan_prepared_semantic_scene_membership(&prepared, scene_root, request) {
        Ok(plan) => plan,
        Err(kind) => {
            return Err(PreparedSemanticMembershipError {
                prepared: Box::new(prepared),
                kind: PreparedSemanticMembershipErrorKind::Operation(kind),
            });
        }
    };
    match prepared.with_existing_plan(plan) {
        Ok(prepared) => Ok(prepared),
        Err((prepared, kind)) => Err(PreparedSemanticMembershipError {
            prepared,
            kind: PreparedSemanticMembershipErrorKind::Transaction(kind),
        }),
    }
}

/// Extend a prepared callback transaction with direct-root admission of newly
/// created object tokens. The same sparse projection planner validates and
/// stages the edge/order edits; rejection returns the original proof intact.
pub fn stage_prepared_semantic_scene_admission<'a>(
    prepared: PreparedSemanticMutationTransaction<'a>,
    scene_root: SemanticNodeId,
    admitted: &[SemanticTransactionNodeRef],
) -> Result<PreparedSemanticMutationTransaction<'a>, PreparedSemanticMembershipError<'a>> {
    match prepared.with_pending_scene_admission(scene_root, admitted) {
        Ok(prepared) => Ok(prepared),
        Err((prepared, kind)) => Err(PreparedSemanticMembershipError { prepared, kind }),
    }
}

fn plan_membership_in_view<V: MembershipView>(
    view: &V,
    scene_root: SemanticNodeId,
    request: SemanticSceneMembershipRequest<'_>,
) -> Result<SemanticMutationTransaction, SemanticSceneOperationError> {
    if !view.target(scene_root)?.family {
        return Err(SemanticSceneOperationError::NotSemanticFamily(scene_root));
    }
    let foreground = view.foreground(scene_root)?;
    match request {
        SemanticSceneMembershipRequest::Clear => {
            let mut transaction = SemanticMutationTransaction::new();
            let mut member = view.first(scene_root)?;
            while let Some(current) = member {
                member = view.next(scene_root, current)?;
                transaction.remove_member(scene_root, current);
            }
            if !foreground.is_empty() {
                transaction.set_foreground_members(scene_root, [] as [SemanticNodeId; 0]);
            }
            Ok(transaction)
        }
        SemanticSceneMembershipRequest::Add(ids) => {
            let explicit = validated_distinct_nodes(view, ids)?;
            if explicit.is_empty() {
                return Ok(SemanticMutationTransaction::new());
            };
            let ordered = foreground::add_order(&explicit, &foreground);
            plan_add_members(view, scene_root, &ordered)
        }
        SemanticSceneMembershipRequest::AddForeground(ids) => {
            let explicit = validated_distinct_nodes(view, ids)?;
            if explicit.is_empty() {
                return Ok(SemanticMutationTransaction::new());
            };
            let members = foreground::add_order(&foreground, &explicit);
            let mut tx = plan_add_members(view, scene_root, &members)?;
            if members != foreground {
                tx.set_foreground_members(scene_root, members);
            };
            Ok(tx)
        }
        SemanticSceneMembershipRequest::RemoveForeground(ids) => {
            let mut tx = SemanticMutationTransaction::new();
            foreground::stage_removal(view, scene_root, ids, &mut tx)?;
            Ok(tx)
        }
        SemanticSceneMembershipRequest::BringToBack(ids) => {
            let explicit = validated_distinct_nodes(view, ids)?;
            let remove = downward_target_closure(view, &explicit)?;
            let mut tx = plan_explicit_root_projection(
                view,
                scene_root,
                &remove,
                None,
                &explicit,
                ExplicitPlacement::Head,
            )?;
            foreground::stage_removal(view, scene_root, &explicit, &mut tx)?;
            Ok(tx)
        }
        SemanticSceneMembershipRequest::Remove(ids) => {
            let explicit = validated_distinct_nodes(view, ids)?;
            let remove = explicit.iter().copied().collect();
            let mut tx = plan_explicit_root_projection(
                view,
                scene_root,
                &remove,
                None,
                &[],
                ExplicitPlacement::Tail,
            )?;
            foreground::stage_removal(view, scene_root, &explicit, &mut tx)?;
            Ok(tx)
        }
        SemanticSceneMembershipRequest::Replace { old, new } => {
            view.target(old)?;
            view.target(new)?;
            if old == new {
                return Ok(SemanticMutationTransaction::new());
            };
            let occurrences = projected_root_path_count(view, scene_root, old)?;
            if occurrences == 0 {
                return Err(SemanticSceneOperationError::MissingMembershipTarget(old));
            };
            if occurrences != 1 {
                return Err(SemanticSceneOperationError::AmbiguousMembershipTarget(old));
            };
            let mut remove = downward_target_closure(view, &[new])?;
            remove.insert(old);
            let mut tx = plan_explicit_root_projection(
                view,
                scene_root,
                &remove,
                Some((old, new)),
                &[],
                ExplicitPlacement::Tail,
            )?;
            let members = foreground::replace_members(view, scene_root, old, new)?;
            if members != foreground {
                tx.set_foreground_members(scene_root, members);
            };
            Ok(tx)
        }
    }
}

fn plan_add_members<V: MembershipView>(
    view: &V,
    scene_root: SemanticNodeId,
    explicit: &[SemanticNodeId],
) -> Result<SemanticMutationTransaction, SemanticSceneOperationError> {
    let remove_set = downward_target_closure(view, explicit)?;
    plan_explicit_root_projection(
        view,
        scene_root,
        &remove_set,
        None,
        explicit,
        ExplicitPlacement::Tail,
    )
}

fn projected_root_path_count<V: MembershipView>(
    view: &V,
    scene_root: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<usize, SemanticSceneOperationError> {
    let mut count = 0usize;
    let mut stack = vec![target];
    while let Some(node) = stack.pop() {
        for parent in view.parents(node)? {
            if parent == scene_root {
                count += 1;
                if count > 1 {
                    return Ok(count);
                }
            } else {
                stack.push(parent);
            }
        }
    }
    Ok(count)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExplicitPlacement {
    Head,
    Tail,
}

/// Stage an ordered admission batch, including transaction-local new objects.
///
/// Existing objects/families use the same family projection and foreground tail
/// as Scene.add. Pending admissions must be newly created, unparented objects;
/// pending family restructuring is not supported. Token identity/allocation and
/// final publication remain owned by the caller's ordinary semantic transaction.
/// Work visits the admitted/foreground closures, plus the current transaction only
/// when it contains pending admissions; unrelated scene roots are not enumerated.
pub fn stage_semantic_scene_admission(
    store: &SemanticStore,
    scene_root: SemanticNodeId,
    admitted: &[SemanticTransactionNodeRef],
    transaction: &mut SemanticMutationTransaction,
) -> Result<(), SemanticSceneOperationError> {
    use crate::{SemanticMutation, SemanticNodeCreation};
    let root = target_node_checked(store, scene_root)?;
    if !matches!(root.kind(), SemanticNodeKind::Family(_)) {
        return Err(SemanticSceneOperationError::NotSemanticFamily(scene_root));
    }
    if admitted.is_empty() {
        return Ok(());
    }
    let existing = admitted
        .iter()
        .filter_map(|id| id.existing())
        .collect::<Vec<_>>();
    let view = StoreMembershipView(store);
    validated_distinct_nodes(&view, &existing)?;
    let mut pending = HashSet::new();
    for &id in admitted {
        if let SemanticTransactionNodeRef::Pending(token) = id {
            if !pending.insert(token) {
                return Err(SemanticSceneOperationError::InvalidPendingAdmission(token));
            }
        }
    }
    if !pending.is_empty() {
        let mut created = HashSet::new();
        for mutation in transaction.mutations() {
            match mutation {
                SemanticMutation::AddNode {
                    token,
                    creation:
                        SemanticNodeCreation::Object { .. }
                        | SemanticNodeCreation::PendingPathObject { .. },
                } if pending.contains(token) => {
                    created.insert(*token);
                }
                SemanticMutation::AddMember {
                    member: SemanticTransactionNodeRef::Pending(token),
                    ..
                } if pending.contains(token) => {
                    return Err(SemanticSceneOperationError::InvalidPendingAdmission(*token));
                }
                _ => {}
            }
        }
        if let Some(token) = pending.difference(&created).next() {
            return Err(SemanticSceneOperationError::InvalidPendingAdmission(*token));
        }
    }
    let foreground = root
        .foreground_members()
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let ordered = admitted
        .iter()
        .copied()
        .filter(|id| id.existing().is_none_or(|id| !foreground.contains(&id)))
        .chain(root.foreground_members().iter().copied().map(Into::into))
        .collect::<Vec<SemanticTransactionNodeRef>>();
    let existing = ordered
        .iter()
        .filter_map(|id| id.existing())
        .collect::<Vec<_>>();
    let removal = downward_target_closure(&view, &existing)?;
    stage_explicit_root_projection(
        &view,
        scene_root,
        &removal,
        None,
        &existing,
        ExplicitPlacement::Tail,
        transaction,
    )?;
    // Fill the pending-object gaps in the projected existing order. These are
    // ordinary provisional edge/order edits, not synthetic semantic identities.
    let mut before = None;
    for &id in ordered.iter().rev() {
        if matches!(id, SemanticTransactionNodeRef::Pending(_)) {
            transaction.add_member(scene_root, id);
            transaction.reorder_member_ref(scene_root, id, before);
        }
        before = Some(id);
    }
    Ok(())
}

/// Stage one lifecycle boundary's removals followed by foreground-aware admission.
///
/// All targets for a scope are planned together so membership restructuring,
/// persistence cleanup and painter order publish in the caller's single transaction.
/// Removal-only boundaries preserve the order of surviving display members. An
/// admission uses the same family projection as Scene.add, with only the surviving
/// foreground declarations at its tail. No temporary store or second order is owned.
pub fn stage_semantic_scene_lifecycle_membership(
    store: &SemanticStore,
    scene_root: SemanticNodeId,
    removed: &[SemanticNodeId],
    added: &[SemanticNodeId],
    transaction: &mut SemanticMutationTransaction,
) -> Result<(), SemanticSceneOperationError> {
    let root = target_node_checked(store, scene_root)?;
    if !matches!(root.kind(), SemanticNodeKind::Family(_)) {
        return Err(SemanticSceneOperationError::NotSemanticFamily(scene_root));
    }
    let view = StoreMembershipView(store);
    let removed = validated_distinct_nodes(&view, removed)?;
    let added = validated_distinct_nodes(&view, added)?;
    if removed.is_empty() && added.is_empty() {
        return Ok(());
    }
    let members = if root.foreground_members().is_empty() || removed.is_empty() {
        root.foreground_members().to_vec()
    } else {
        let removal = downward_target_closure(&view, &removed)?;
        foreground::project_members(&view, scene_root, &removal, None)?
    };
    let explicit = if added.is_empty() {
        Vec::new()
    } else {
        foreground::add_order(&added, &members)
    };
    let mut remove_set = downward_target_closure(&view, &explicit)?;
    remove_set.extend(removed);
    stage_explicit_root_projection(
        &view,
        scene_root,
        &remove_set,
        None,
        &explicit,
        ExplicitPlacement::Tail,
        transaction,
    )?;
    if members != root.foreground_members() {
        transaction.set_foreground_members(scene_root, members);
    }
    Ok(())
}

fn plan_explicit_root_projection<V: MembershipView>(
    view: &V,
    scene_root: SemanticNodeId,
    remove_set: &HashSet<SemanticNodeId>,
    replacement: Option<(SemanticNodeId, SemanticNodeId)>,
    explicit: &[SemanticNodeId],
    placement: ExplicitPlacement,
) -> Result<SemanticMutationTransaction, SemanticSceneOperationError> {
    let mut transaction = SemanticMutationTransaction::new();
    stage_explicit_root_projection(
        view,
        scene_root,
        remove_set,
        replacement,
        explicit,
        placement,
        &mut transaction,
    )?;
    Ok(transaction)
}

fn stage_explicit_root_projection<V: MembershipView>(
    view: &V,
    scene_root: SemanticNodeId,
    remove_set: &HashSet<SemanticNodeId>,
    replacement: Option<(SemanticNodeId, SemanticNodeId)>,
    explicit: &[SemanticNodeId],
    placement: ExplicitPlacement,
    transaction: &mut SemanticMutationTransaction,
) -> Result<(), SemanticSceneOperationError> {
    let (affected, affected_roots) = affected_explicit_root_closure(view, scene_root, remove_set)?;
    let mut run_heads = Vec::new();
    for &root in &affected_roots {
        if view
            .previous(scene_root, root)?
            .is_none_or(|previous| !affected_roots.contains(&previous))
        {
            run_heads.push(root);
        }
    }

    let boundary = ProjectionBoundary::Family(scene_root);
    let mut plans = Vec::with_capacity(affected_roots.len());
    let mut globally_promoted = HashSet::new();
    for run_head in run_heads {
        let mut before = Some(run_head);
        while before.is_some_and(|candidate| affected_roots.contains(&candidate)) {
            before = match before {
                Some(candidate) => view.next(scene_root, candidate)?,
                None => None,
            };
        }
        let mut root = Some(run_head);
        while let Some(current) = root.filter(|root| affected_roots.contains(root)) {
            let mut promoted = HashSet::new();
            let mut replacements = Vec::new();
            collect_root_replacements(
                view,
                current,
                current,
                remove_set,
                &affected,
                replacement,
                boundary,
                &mut promoted,
                &mut replacements,
            )?;
            for &promoted in &replacements {
                if !globally_promoted.insert(promoted) {
                    return Err(SemanticSceneOperationError::AmbiguousCrossRootAlias(
                        promoted,
                    ));
                }
            }
            plans.push((current, replacements, before));
            root = view.next(scene_root, current)?;
        }
    }

    let head_anchor = if placement == ExplicitPlacement::Head {
        let first_replacement_by_root = plans
            .iter()
            .map(|(root, replacements, _)| (*root, replacements.first().copied()))
            .collect::<HashMap<_, _>>();
        first_projected_root_after_restructure(
            view,
            scene_root,
            &affected_roots,
            &first_replacement_by_root,
        )?
    } else {
        None
    };
    let mut retained_roots = HashSet::new();
    for &member in explicit {
        if view.contains(scene_root, member)? {
            retained_roots.insert(member);
        }
    }
    // If the replacement target is already a direct root, keep that edge and
    // move it into the source slot instead of staging remove+add for the same
    // family edge. Semantic transactions deliberately reject duplicate edge
    // mutations, and a visible target already has the identity we need.
    if let Some((_, replacement)) = replacement {
        if view.contains(scene_root, replacement)? {
            retained_roots.insert(replacement);
        }
    }
    for (root, _, _) in &plans {
        if !retained_roots.contains(root) {
            transaction.remove_member(scene_root, *root);
        }
    }
    for (_, replacements, _) in &plans {
        for replacement in replacements {
            transaction.add_member(scene_root, *replacement);
        }
    }
    for member in explicit {
        if !retained_roots.contains(member) {
            transaction.add_member(scene_root, *member);
        }
    }
    // Plans are in authoritative order within each affected run. Moving each
    // replacement block before the run's first surviving successor preserves it.
    for (_, replacements, before) in &plans {
        let mut anchor = *before;
        for replacement in replacements.iter().rev() {
            transaction.reorder_member(scene_root, *replacement, anchor);
            anchor = Some(*replacement);
        }
    }
    let mut anchor = match placement {
        ExplicitPlacement::Tail => None,
        ExplicitPlacement::Head => head_anchor,
    };
    for member in explicit.iter().rev() {
        transaction.reorder_member(scene_root, *member, anchor);
        anchor = Some(*member);
    }
    Ok(())
}

fn first_projected_root_after_restructure<V: MembershipView>(
    view: &V,
    scene_root: SemanticNodeId,
    affected_roots: &HashSet<SemanticNodeId>,
    first_replacement_by_root: &HashMap<SemanticNodeId, Option<SemanticNodeId>>,
) -> Result<Option<SemanticNodeId>, SemanticSceneOperationError> {
    let mut current = view.first(scene_root)?;
    while let Some(member) = current {
        if !affected_roots.contains(&member) {
            return Ok(Some(member));
        }
        if let Some(Some(first)) = first_replacement_by_root.get(&member) {
            return Ok(Some(*first));
        }
        current = view.next(scene_root, member)?;
    }
    Ok(None)
}

fn affected_explicit_root_closure<V: MembershipView>(
    view: &V,
    scene_root: SemanticNodeId,
    remove_set: &HashSet<SemanticNodeId>,
) -> Result<(HashSet<SemanticNodeId>, HashSet<SemanticNodeId>), SemanticSceneOperationError> {
    let mut affected = HashSet::new();
    let mut roots = HashSet::new();
    let mut stack = remove_set.iter().copied().collect::<Vec<_>>();
    while let Some(node) = stack.pop() {
        if !affected.insert(node) {
            continue;
        }
        for parent in view.parents(node)? {
            if parent == scene_root {
                roots.insert(node);
            } else {
                stack.push(parent);
            }
        }
    }
    Ok((affected, roots))
}

fn validated_distinct_nodes<V: MembershipView>(
    view: &V,
    ids: &[SemanticNodeId],
) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError> {
    let mut seen = HashSet::with_capacity(ids.len());
    for &id in ids {
        view.target(id)?;
        if !seen.insert(id) {
            return Err(SemanticSceneOperationError::DuplicateMembershipTarget(id));
        }
    }
    Ok(ids.to_vec())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SceneRootProjection {
    root: SemanticNodeId,
    replacements: Vec<SemanticNodeId>,
}

#[derive(Clone, Copy)]
enum ProjectionBoundary<'a> {
    SceneRoots,
    Family(SemanticNodeId),
    Declarations(&'a HashSet<SemanticNodeId>),
}

impl ProjectionBoundary<'_> {
    fn contains<V: MembershipView>(
        self,
        view: &V,
        current: SemanticNodeId,
    ) -> Result<bool, SemanticSceneOperationError> {
        Ok(match self {
            Self::Declarations(roots) => roots.contains(&current),
            Self::SceneRoots => view.target(current)?.scene_owned,
            Self::Family(root) => view.contains(root, current)?,
        })
    }
}

impl SemanticStore {
    /// Add target semantic objects/families with family-aware top-level restructuring.
    ///
    /// The whole batch is validated and planned before any scene membership changes.
    /// Existing family edges are not mutated. Descendants already represented by
    /// an affected family root are removed from that root projection, surviving
    /// siblings are promoted in place, and the explicit inputs are then appended
    /// in caller order.
    pub fn add_semantic_scene_nodes(
        &mut self,
        ids: &[SemanticNodeId],
    ) -> Result<(), SemanticSceneOperationError> {
        self.set_last_mutation_writes(0);
        let explicit = validated_unique_nodes(self, ids)?;
        if explicit.is_empty() {
            return Ok(());
        }

        let remove_set = downward_target_closure(&StoreMembershipView(self), &explicit)?;
        let plans = plan_scene_restructure(self, &remove_set)?;

        let mut writes = 0;
        for plan in plans {
            writes += self.replace_scene_root_with_detached(plan.root, &plan.replacements);
        }
        for id in explicit {
            let attached = self.attach_to_scene(id)?;
            assert!(
                attached,
                "family-aware add planning must leave explicit nodes detached"
            );
            writes += self.last_mutation_stats().slots_written;
        }
        self.set_last_mutation_writes(writes);
        Ok(())
    }

    /// Remove target semantic objects/families from the top-level scene projection.
    ///
    /// Removing a family removes that whole projected branch. Removing one of its
    /// descendants dissolves only affected projected family roots and promotes the
    /// surviving branches at the exact former root position. Family relationships
    /// themselves remain unchanged.
    pub fn remove_semantic_scene_nodes(
        &mut self,
        ids: &[SemanticNodeId],
    ) -> Result<(), SemanticSceneOperationError> {
        self.set_last_mutation_writes(0);
        let remove_set = validated_unique_nodes(self, ids)?
            .into_iter()
            .collect::<HashSet<_>>();
        if remove_set.is_empty() {
            return Ok(());
        }

        let plans = plan_scene_restructure(self, &remove_set)?;
        let mut writes = 0;
        for plan in plans {
            writes += self.replace_scene_root_with_detached(plan.root, &plan.replacements);
        }
        self.set_last_mutation_writes(writes);
        Ok(())
    }
}

fn target_node_checked(
    store: &SemanticStore,
    id: SemanticNodeId,
) -> Result<&SemanticNode, SemanticSceneOperationError> {
    let node = store
        .node(id)
        .ok_or(SemanticSceneOperationError::UnknownNode(id))?;
    let is_target = match node.kind() {
        SemanticNodeKind::Family(_) => true,
        SemanticNodeKind::AuthoringObject => node.semantic_object_state().is_some(),
        SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => false,
    };
    if !is_target {
        return Err(SemanticSceneOperationError::NotSemanticAuthoringNode(id));
    }
    Ok(node)
}

fn validated_unique_nodes(
    store: &SemanticStore,
    ids: &[SemanticNodeId],
) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError> {
    let mut seen = HashSet::with_capacity(ids.len());
    let mut unique = Vec::with_capacity(ids.len());
    for id in ids.iter().copied() {
        target_node_checked(store, id)?;
        if seen.insert(id) {
            unique.push(id);
        }
    }
    Ok(unique)
}

fn downward_target_closure<V: MembershipView>(
    view: &V,
    roots: &[SemanticNodeId],
) -> Result<HashSet<SemanticNodeId>, SemanticSceneOperationError> {
    let mut closure = HashSet::new();
    let mut stack = roots.to_vec();
    while let Some(id) = stack.pop() {
        if !closure.insert(id) {
            continue;
        }
        if view.target(id)?.family {
            let mut member = view.first(id)?;
            while let Some(current) = member {
                member = view.next(id, current)?;
                stack.push(current);
            }
        }
    }
    Ok(closure)
}

fn affected_ancestor_closure(
    store: &SemanticStore,
    remove_set: &HashSet<SemanticNodeId>,
) -> Result<(HashSet<SemanticNodeId>, Vec<SemanticNodeId>), SemanticSceneOperationError> {
    let mut affected = HashSet::new();
    let mut root_set = HashSet::new();
    let mut roots = Vec::new();
    let mut stack = remove_set.iter().copied().collect::<Vec<_>>();

    while let Some(id) = stack.pop() {
        if !affected.insert(id) {
            continue;
        }
        let node = target_node_checked(store, id)?;
        if node.is_scene_owned() && root_set.insert(id) {
            roots.push(id);
        }
        stack.extend(node.parents().iter().copied());
    }

    Ok((affected, roots))
}

fn plan_scene_restructure(
    store: &SemanticStore,
    remove_set: &HashSet<SemanticNodeId>,
) -> Result<Vec<SceneRootProjection>, SemanticSceneOperationError> {
    let (affected, affected_roots) = affected_ancestor_closure(store, remove_set)?;
    let mut plans = Vec::with_capacity(affected_roots.len());

    for root in affected_roots {
        // First-occurrence de-duplication within one semantic root follows that
        // root's authoritative family order and requires no unrelated scene scan.
        let mut promoted = HashSet::new();
        let mut replacements = Vec::new();
        collect_root_replacements(
            &StoreMembershipView(store),
            root,
            root,
            remove_set,
            &affected,
            None,
            ProjectionBoundary::SceneRoots,
            &mut promoted,
            &mut replacements,
        )?;
        plans.push(SceneRootProjection { root, replacements });
    }

    // A node promoted from multiple attached roots needs the relative authored
    // order of those roots to choose the globally first occurrence. The current
    // intrusive root list has no local comparison primitive, and walking it would
    // make a tiny edit O(total scene roots). Reject before commit instead. A future
    // local order-maintenance primitive can remove this temporary restriction.
    let mut globally_promoted = HashSet::new();
    let mut conflict: Option<SemanticNodeId> = None;
    for plan in &plans {
        for replacement in plan.replacements.iter().copied() {
            if !globally_promoted.insert(replacement) {
                conflict = Some(match conflict {
                    Some(current) => current.min(replacement),
                    None => replacement,
                });
            }
        }
    }
    if let Some(alias) = conflict {
        return Err(SemanticSceneOperationError::AmbiguousCrossRootAlias(alias));
    }

    Ok(plans)
}

#[allow(clippy::too_many_arguments)]
fn collect_root_replacements<V: MembershipView>(
    view: &V,
    current: SemanticNodeId,
    current_root: SemanticNodeId,
    remove_set: &HashSet<SemanticNodeId>,
    affected: &HashSet<SemanticNodeId>,
    replacement: Option<(SemanticNodeId, SemanticNodeId)>,
    boundary: ProjectionBoundary<'_>,
    promoted: &mut HashSet<SemanticNodeId>,
    output: &mut Vec<SemanticNodeId>,
) -> Result<(), SemanticSceneOperationError> {
    if replacement.is_some_and(|(old, _)| current == old) {
        let new = replacement.expect("replacement checked above").1;
        if promoted.insert(new) {
            output.push(new);
        }
        return Ok(());
    }
    if remove_set.contains(&current) {
        return Ok(());
    }

    let target = view.target(current)?;
    if current != current_root && boundary.contains(view, current)? {
        return Ok(());
    }

    if !affected.contains(&current) {
        if promoted.insert(current) {
            output.push(current);
        }
        return Ok(());
    }

    if target.family {
        let mut member = view.first(current)?;
        while let Some(next) = member {
            member = view.next(current, next)?;
            collect_root_replacements(
                view,
                next,
                current_root,
                remove_set,
                affected,
                replacement,
                boundary,
                promoted,
                output,
            )?;
        }
    } else if promoted.insert(current) {
        output.push(current);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SemanticMutationStats, SemanticNodeCreation, SemanticNodeResidency, SemanticObjectState,
        StoredGeometry,
    };

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    #[test]
    fn prepared_membership_stages_promoted_sibling_without_root_scan() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let child = object(&mut store, 1.0);
        let sibling = object(&mut store, 2.0);
        let family = store.insert_family();
        store.add_semantic_family_member(family, child).unwrap();
        store.add_semantic_family_member(family, sibling).unwrap();
        let transaction = plan_semantic_scene_membership(
            &store,
            root,
            SemanticSceneMembershipRequest::Add(&[family]),
        )
        .unwrap();
        let prepared = transaction.prepare(&mut store).unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Remove(&[child]),
        )
        .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Add(&[sibling]),
        )
        .unwrap();
        prepared.commit();
        assert_eq!(
            store.semantic_family_members_checked(root).unwrap(),
            vec![sibling]
        );
        assert_eq!(
            store.semantic_family_members_checked(family).unwrap(),
            vec![child, sibling]
        );
    }

    #[test]
    fn caught_prepared_stage_error_retains_prior_callback_overlay() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let family = store.insert_family();
        let child = object(&mut store, 1.0);
        store.add_semantic_family_member(family, child).unwrap();

        let prepared = SemanticMutationTransaction::new()
            .prepare(&mut store)
            .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Add(&[family]),
        )
        .unwrap();
        let error = match stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Add(&[family, family]),
        ) {
            Ok(_) => panic!("duplicate callback stage unexpectedly succeeded"),
            Err(error) => error,
        };
        assert!(matches!(
            error.kind(),
            PreparedSemanticMembershipErrorKind::Operation(
                SemanticSceneOperationError::DuplicateMembershipTarget(id)
            ) if *id == family
        ));

        let prepared = error.into_prepared();
        assert_eq!(
            prepared.family_first_member_existing(root).unwrap(),
            Some(family)
        );
        prepared.commit();
        assert_eq!(
            store.semantic_family_members_checked(root).unwrap(),
            vec![family]
        );
    }

    #[test]
    fn prepared_membership_view_merges_staged_nested_reparent_parents() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let outer = store.insert_family();
        let nested = store.insert_family();
        let child = object(&mut store, 1.0);
        store.add_semantic_family_member(root, outer).unwrap();
        store.add_semantic_family_member(outer, nested).unwrap();
        store.add_semantic_family_member(nested, child).unwrap();

        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_member(root, nested);
        let prepared = transaction.prepare(&mut store).unwrap();
        let view = PreparedMembershipView {
            prepared: &prepared,
        };

        assert_eq!(view.parents(nested).unwrap(), vec![outer, root]);
    }

    #[test]
    fn prepared_existing_membership_rejects_pending_order_neighbor_without_losing_proof() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let a = object(&mut store, 1.0);
        let b = object(&mut store, 2.0);
        store.add_semantic_family_member(root, a).unwrap();
        store.add_semantic_family_member(root, b).unwrap();

        let mut transaction = SemanticMutationTransaction::new();
        let pending = transaction.create_node(SemanticNodeCreation::object(
            SemanticObjectState::new(StoredGeometry::Circle { radius: 3.0 }),
        ));
        transaction.add_member(root, pending);
        transaction.reorder_member_ref(root, pending, Some(b.into()));
        let prepared = transaction.prepare(&mut store).unwrap();
        let pending_id = prepared.planned_node_id(pending).unwrap();

        let error = match stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Clear,
        ) {
            Ok(_) => panic!("existing-handle planner accepted a pending order neighbor"),
            Err(error) => error,
        };
        assert!(matches!(
            error.kind(),
            PreparedSemanticMembershipErrorKind::Operation(
                SemanticSceneOperationError::InvalidPendingAdmission(token)
            ) if *token == pending
        ));

        error.into_prepared().commit();
        assert_eq!(
            store.semantic_family_members_checked(root).unwrap(),
            vec![a, pending_id, b]
        );
    }

    fn ordered_root_store() -> (
        SemanticStore,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
    ) {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let a = object(&mut store, 1.0);
        let b = object(&mut store, 2.0);
        let c = object(&mut store, 3.0);
        let d = object(&mut store, 4.0);
        plan_semantic_scene_membership(
            &store,
            root,
            SemanticSceneMembershipRequest::Add(&[a, b, c]),
        )
        .unwrap()
        .apply(&mut store)
        .unwrap();
        (store, root, a, b, c, d)
    }

    fn apply_membership_request(
        store: &mut SemanticStore,
        root: SemanticNodeId,
        request: SemanticSceneMembershipRequest<'_>,
    ) {
        plan_semantic_scene_membership(store, root, request)
            .unwrap()
            .apply(store)
            .unwrap();
    }

    #[test]
    fn prepared_remove_then_add_matches_sequential_tail_move() {
        let (mut staged_store, root, a, b, c, _) = ordered_root_store();
        let prepared = SemanticMutationTransaction::new()
            .prepare(&mut staged_store)
            .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Remove(&[a]),
        )
        .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Add(&[a]),
        )
        .unwrap();
        prepared.commit();

        let (mut sequential_store, sequential_root, sequential_a, sequential_b, sequential_c, _) =
            ordered_root_store();
        apply_membership_request(
            &mut sequential_store,
            sequential_root,
            SemanticSceneMembershipRequest::Remove(&[sequential_a]),
        );
        apply_membership_request(
            &mut sequential_store,
            sequential_root,
            SemanticSceneMembershipRequest::Add(&[sequential_a]),
        );

        assert_eq!(
            staged_store.semantic_family_members_checked(root).unwrap(),
            vec![b, c, a]
        );
        assert_eq!(
            staged_store.semantic_family_members_checked(root).unwrap(),
            sequential_store
                .semantic_family_members_checked(sequential_root)
                .unwrap()
        );
        assert_eq!(
            sequential_store
                .semantic_family_members_checked(sequential_root)
                .unwrap(),
            vec![sequential_b, sequential_c, sequential_a]
        );
    }

    #[test]
    fn prepared_add_remove_add_matches_sequential_reentry() {
        let (mut staged_store, root, a, b, c, d) = ordered_root_store();
        let prepared = SemanticMutationTransaction::new()
            .prepare(&mut staged_store)
            .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Add(&[d]),
        )
        .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Remove(&[d]),
        )
        .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Add(&[d]),
        )
        .unwrap();
        prepared.commit();

        let (
            mut sequential_store,
            sequential_root,
            sequential_a,
            sequential_b,
            sequential_c,
            sequential_d,
        ) = ordered_root_store();
        apply_membership_request(
            &mut sequential_store,
            sequential_root,
            SemanticSceneMembershipRequest::Add(&[sequential_d]),
        );
        apply_membership_request(
            &mut sequential_store,
            sequential_root,
            SemanticSceneMembershipRequest::Remove(&[sequential_d]),
        );
        apply_membership_request(
            &mut sequential_store,
            sequential_root,
            SemanticSceneMembershipRequest::Add(&[sequential_d]),
        );

        assert_eq!(
            staged_store.semantic_family_members_checked(root).unwrap(),
            vec![a, b, c, d]
        );
        assert_eq!(
            staged_store.semantic_family_members_checked(root).unwrap(),
            sequential_store
                .semantic_family_members_checked(sequential_root)
                .unwrap()
        );
        assert_eq!(
            sequential_store
                .semantic_family_members_checked(sequential_root)
                .unwrap(),
            vec![sequential_a, sequential_b, sequential_c, sequential_d]
        );
    }

    #[test]
    fn prepared_alternating_root_reorders_match_sequential_plans() {
        let (mut staged_store, root, a, b, c, _) = ordered_root_store();
        let prepared = SemanticMutationTransaction::new()
            .prepare(&mut staged_store)
            .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::BringToBack(&[c]),
        )
        .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::BringToBack(&[b]),
        )
        .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::BringToBack(&[a]),
        )
        .unwrap();
        prepared.commit();

        let (mut sequential_store, sequential_root, sequential_a, sequential_b, sequential_c, _) =
            ordered_root_store();
        apply_membership_request(
            &mut sequential_store,
            sequential_root,
            SemanticSceneMembershipRequest::BringToBack(&[sequential_c]),
        );
        apply_membership_request(
            &mut sequential_store,
            sequential_root,
            SemanticSceneMembershipRequest::BringToBack(&[sequential_b]),
        );
        apply_membership_request(
            &mut sequential_store,
            sequential_root,
            SemanticSceneMembershipRequest::BringToBack(&[sequential_a]),
        );

        assert_eq!(
            staged_store.semantic_family_members_checked(root).unwrap(),
            sequential_store
                .semantic_family_members_checked(sequential_root)
                .unwrap()
        );
    }

    #[test]
    fn prepared_removal_after_reorder_anchor_matches_sequential_plan() {
        let (mut staged_store, root, _, b, c, _) = ordered_root_store();
        let prepared = SemanticMutationTransaction::new()
            .prepare(&mut staged_store)
            .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::BringToBack(&[c]),
        )
        .unwrap();
        let prepared = stage_prepared_semantic_scene_membership(
            prepared,
            root,
            SemanticSceneMembershipRequest::Remove(&[b]),
        )
        .unwrap();
        prepared.commit();

        let (mut sequential_store, sequential_root, _, sequential_a, sequential_c, _) =
            ordered_root_store();
        apply_membership_request(
            &mut sequential_store,
            sequential_root,
            SemanticSceneMembershipRequest::BringToBack(&[sequential_c]),
        );
        apply_membership_request(
            &mut sequential_store,
            sequential_root,
            SemanticSceneMembershipRequest::Remove(&[sequential_a]),
        );

        assert_eq!(
            staged_store.semantic_family_members_checked(root).unwrap(),
            sequential_store
                .semantic_family_members_checked(sequential_root)
                .unwrap()
        );
    }

    #[test]
    fn adding_family_collapses_existing_descendant_roots_and_appends_family() {
        let mut store = SemanticStore::new();
        let left = object(&mut store, 0.5);
        let first = object(&mut store, 1.0);
        let second = object(&mut store, 2.0);
        let right = object(&mut store, 3.0);
        let family = store.insert_family();
        store.add_semantic_family_member(family, first).unwrap();
        store.add_semantic_family_member(family, second).unwrap();

        store.attach_semantic_object(left).unwrap();
        store.attach_semantic_object(first).unwrap();
        store.attach_semantic_object(second).unwrap();
        store.attach_semantic_object(right).unwrap();

        store.add_semantic_scene_nodes(&[family]).unwrap();

        assert_eq!(
            store.scene_roots().collect::<Vec<_>>(),
            vec![left, right, family]
        );
        assert_eq!(
            store.node(first).unwrap().residency(),
            SemanticNodeResidency::Detached
        );
        assert_eq!(
            store.node(second).unwrap().residency(),
            SemanticNodeResidency::Detached
        );
        assert_eq!(
            store.semantic_family_members_checked(family).unwrap(),
            vec![first, second]
        );
    }

    #[test]
    fn removing_descendant_promotes_surviving_sibling_at_family_root_position() {
        let mut store = SemanticStore::new();
        let left = object(&mut store, 0.5);
        let removed = object(&mut store, 1.0);
        let survivor = object(&mut store, 2.0);
        let right = object(&mut store, 3.0);
        let family = store.insert_family();
        store.add_semantic_family_member(family, removed).unwrap();
        store.add_semantic_family_member(family, survivor).unwrap();

        store.attach_semantic_object(left).unwrap();
        store.add_semantic_scene_nodes(&[family]).unwrap();
        store.attach_semantic_object(right).unwrap();
        assert_eq!(
            store.scene_roots().collect::<Vec<_>>(),
            vec![left, family, right]
        );

        store.remove_semantic_scene_nodes(&[removed]).unwrap();

        assert_eq!(
            store.scene_roots().collect::<Vec<_>>(),
            vec![left, survivor, right]
        );
        assert_eq!(
            store.node(family).unwrap().residency(),
            SemanticNodeResidency::Detached
        );
        assert_eq!(
            store.semantic_family_members_checked(family).unwrap(),
            vec![removed, survivor]
        );
    }

    #[test]
    fn batch_validation_happens_before_any_scene_membership_change() {
        let mut store = SemanticStore::new();
        let existing = object(&mut store, 0.5);
        let valid = object(&mut store, 1.0);
        let identity_only = store.insert_authoring_object();
        store.attach_semantic_object(existing).unwrap();

        assert_eq!(
            store.add_semantic_scene_nodes(&[valid, identity_only]),
            Err(SemanticSceneOperationError::NotSemanticAuthoringNode(
                identity_only
            ))
        );
        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );
        assert_eq!(store.scene_roots().collect::<Vec<_>>(), vec![existing]);
        assert_eq!(
            store.node(valid).unwrap().residency(),
            SemanticNodeResidency::Detached
        );
    }

    #[test]
    fn disjoint_multi_root_restructure_stays_local_with_many_unrelated_roots() {
        let mut store = SemanticStore::new();
        let removed_left = object(&mut store, 1.0);
        let survivor_left = object(&mut store, 2.0);
        let removed_right = object(&mut store, 3.0);
        let survivor_right = object(&mut store, 4.0);
        let left_family = store.insert_family();
        let right_family = store.insert_family();
        store
            .add_semantic_family_member(left_family, removed_left)
            .unwrap();
        store
            .add_semantic_family_member(left_family, survivor_left)
            .unwrap();
        store
            .add_semantic_family_member(right_family, removed_right)
            .unwrap();
        store
            .add_semantic_family_member(right_family, survivor_right)
            .unwrap();

        store.attach_to_scene(left_family).unwrap();
        for index in 0..10_000 {
            let unrelated = object(&mut store, 10.0 + index as f32);
            store.attach_semantic_object(unrelated).unwrap();
        }
        store.attach_to_scene(right_family).unwrap();

        store
            .remove_semantic_scene_nodes(&[removed_left, removed_right])
            .unwrap();

        assert_eq!(store.scene_root_count(), 10_002);
        let roots = store.scene_roots().collect::<Vec<_>>();
        assert_eq!(roots.first(), Some(&survivor_left));
        assert_eq!(roots.last(), Some(&survivor_right));
        assert_eq!(store.last_mutation_stats().slots_written, 6);
    }

    #[test]
    fn cross_root_alias_conflict_fails_atomically_without_scene_order_scan() {
        let mut store = SemanticStore::new();
        let removed = object(&mut store, 1.0);
        let shared = object(&mut store, 2.0);
        let older_family = store.insert_family();
        let newer_family = store.insert_family();

        for family in [older_family, newer_family] {
            store.add_semantic_family_member(family, removed).unwrap();
            store.add_semantic_family_member(family, shared).unwrap();
        }

        store.attach_to_scene(older_family).unwrap();
        for index in 0..10_000 {
            let unrelated = object(&mut store, 10.0 + index as f32);
            store.attach_semantic_object(unrelated).unwrap();
        }
        store.attach_to_scene(newer_family).unwrap();
        let before = store.scene_roots().collect::<Vec<_>>();

        assert_eq!(
            store.remove_semantic_scene_nodes(&[removed]),
            Err(SemanticSceneOperationError::AmbiguousCrossRootAlias(shared))
        );

        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );
        assert_eq!(store.scene_roots().collect::<Vec<_>>(), before);
        assert_eq!(
            store.node(older_family).unwrap().residency(),
            SemanticNodeResidency::SceneOwned
        );
        assert_eq!(
            store.node(newer_family).unwrap().residency(),
            SemanticNodeResidency::SceneOwned
        );
        assert_eq!(
            store.node(shared).unwrap().residency(),
            SemanticNodeResidency::Detached
        );
    }

    #[test]
    fn explicit_root_planner_preserves_family_promotion_and_replace_slot() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let left = object(&mut store, 0.5);
        let first = object(&mut store, 1.0);
        let second = object(&mut store, 2.0);
        let replacement = object(&mut store, 3.0);
        let right = object(&mut store, 4.0);
        let family = store.insert_family();
        store.add_semantic_family_member(family, first).unwrap();
        store.add_semantic_family_member(family, second).unwrap();
        for member in [left, first, second, right] {
            store.add_semantic_family_member(root, member).unwrap();
        }

        plan_semantic_scene_membership(
            &store,
            root,
            SemanticSceneMembershipRequest::Add(&[family]),
        )
        .unwrap()
        .apply(&mut store)
        .unwrap();
        assert_eq!(store.node(root).unwrap().members(), &[left, right, family]);

        plan_semantic_scene_membership(
            &store,
            root,
            SemanticSceneMembershipRequest::Replace {
                old: second,
                new: replacement,
            },
        )
        .unwrap()
        .apply(&mut store)
        .unwrap();
        assert_eq!(
            store.node(root).unwrap().members(),
            &[left, right, first, replacement]
        );
        assert_eq!(
            store.semantic_family_members_checked(family).unwrap(),
            vec![first, second]
        );
    }

    #[test]
    fn explicit_root_bring_to_back_prepends_after_family_promotion_in_caller_order() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let first = object(&mut store, 1.0);
        let survivor = object(&mut store, 2.0);
        let middle = object(&mut store, 3.0);
        let last = object(&mut store, 4.0);
        let family = store.insert_family();
        store.add_semantic_family_member(family, first).unwrap();
        store.add_semantic_family_member(family, survivor).unwrap();
        for member in [family, middle, last] {
            store.add_semantic_family_member(root, member).unwrap();
        }

        let revision = store.scene_revision();
        plan_semantic_scene_membership(
            &store,
            root,
            SemanticSceneMembershipRequest::BringToBack(&[first, last]),
        )
        .unwrap()
        .apply(&mut store)
        .unwrap();

        assert_eq!(
            store.node(root).unwrap().members(),
            &[first, last, survivor, middle]
        );
        assert_eq!(
            store.scene_revision(),
            revision
                .checked_next()
                .expect("revision should advance once")
        );
        assert_eq!(
            store.semantic_family_members_checked(family).unwrap(),
            vec![first, survivor]
        );
    }

    #[test]
    fn explicit_root_planner_rejects_duplicate_batch_before_staging() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let member = object(&mut store, 1.0);
        let revision = store.scene_revision();
        assert_eq!(
            plan_semantic_scene_membership(
                &store,
                root,
                SemanticSceneMembershipRequest::Add(&[member, member]),
            ),
            Err(SemanticSceneOperationError::DuplicateMembershipTarget(
                member
            ))
        );
        assert_eq!(store.scene_revision(), revision);
        assert!(store.node(root).unwrap().members().is_empty());
    }

    #[test]
    fn explicit_root_planner_preserves_adjacent_replacement_blocks() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let removed_left = object(&mut store, 1.0);
        let survivor_left = object(&mut store, 2.0);
        let removed_right = object(&mut store, 3.0);
        let survivor_right = object(&mut store, 4.0);
        let tail = object(&mut store, 5.0);
        let left_family = store.insert_family();
        let right_family = store.insert_family();
        for (family, members) in [
            (left_family, [removed_left, survivor_left]),
            (right_family, [removed_right, survivor_right]),
        ] {
            for member in members {
                store.add_semantic_family_member(family, member).unwrap();
            }
        }
        for member in [left_family, right_family, tail] {
            store.add_semantic_family_member(root, member).unwrap();
        }

        plan_semantic_scene_membership(
            &store,
            root,
            SemanticSceneMembershipRequest::Remove(&[removed_left, removed_right]),
        )
        .unwrap()
        .apply(&mut store)
        .unwrap();

        assert_eq!(
            store.node(root).unwrap().members(),
            &[survivor_left, survivor_right, tail]
        );
    }

    #[test]
    fn explicit_root_remove_plans_only_the_affected_branch() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let removed = object(&mut store, 1.0);
        let survivor = object(&mut store, 2.0);
        let family = store.insert_family();
        store.add_semantic_family_member(family, removed).unwrap();
        store.add_semantic_family_member(family, survivor).unwrap();
        store.add_semantic_family_member(root, family).unwrap();
        for index in 0..10_000 {
            let unrelated = object(&mut store, 10.0 + index as f32);
            store.add_semantic_family_member(root, unrelated).unwrap();
        }

        let transaction = plan_semantic_scene_membership(
            &store,
            root,
            SemanticSceneMembershipRequest::Remove(&[removed]),
        )
        .unwrap();
        // Remove the affected root, add its survivor, and place that survivor
        // before the root's existing O(1)-resolved successor.
        assert_eq!(transaction.mutations().len(), 3);
        transaction.apply(&mut store).unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 3);
        assert_eq!(store.node(root).unwrap().first_member(), Some(survivor));
        assert_eq!(store.node(root).unwrap().member_count(), 10_001);
    }

    #[test]
    fn root_contains_follows_alias_ancestors_and_observes_detach() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let left = store.insert_family();
        let right = store.insert_family();
        let leaf = object(&mut store, 1.0);
        store.add_semantic_family_member(left, leaf).unwrap();
        store.add_semantic_family_member(right, leaf).unwrap();
        store.add_semantic_family_member(root, left).unwrap();
        store.add_semantic_family_member(root, right).unwrap();

        assert!(semantic_scene_root_contains(&store, root, leaf).unwrap());
        store.remove_semantic_family_member(root, left).unwrap();
        assert!(semantic_scene_root_contains(&store, root, leaf).unwrap());
        store.remove_semantic_family_member(root, right).unwrap();
        assert!(!semantic_scene_root_contains(&store, root, leaf).unwrap());
    }
}
