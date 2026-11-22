//! Resolve the outer mounted family through the existing semantic parent edges.
//! Only the selected family identity is retained in the compiled object row.
use std::collections::{BTreeSet, HashSet};

use noon_core::{
    PreparedSemanticMutationTransaction, SemanticMutation, SemanticNodeId, SemanticStore,
    SemanticTransactionNodeRef as Node,
};

use super::{semantic_execution_object_id, SemanticExecutionReachability, SemanticLoweringError};
use crate::{ExecutionMutationTransaction, ExecutionPatch};

/// A scene-owned root can itself be a group. A Scene's private execution root
/// is a container, so the group starts at its mounted child instead.
fn outer_family(
    leaf: Node,
    has_members: bool,
    mut parents: impl FnMut(Node) -> Vec<Node>,
    mut root: impl FnMut(Node) -> Option<bool>,
) -> Option<Node> {
    let mut pending = vec![leaf];
    let mut visited = HashSet::new();
    let mut group = None;
    while let Some(node) = pending.pop() {
        if !visited.insert(node) {
            continue;
        }
        let mut is_family = root(node) == Some(true);
        for parent in parents(node) {
            if root(parent) == Some(false) {
                is_family = true;
            } else {
                pending.push(parent);
            }
        }
        if is_family && (node != leaf || has_members) {
            if group.is_some_and(|group| group != node) {
                // An aliased drawable with several outer families is ambiguous.
                return None;
            }
            group = Some(node);
        }
    }
    group
}

pub(super) fn committed(
    store: &SemanticStore,
    object: SemanticNodeId,
    is_root: impl Fn(SemanticNodeId) -> bool,
) -> Option<SemanticNodeId> {
    outer_family(
        object.into(),
        store.node(object)?.members_iter().next().is_some(),
        |node| {
            node.existing()
                .and_then(|node| store.node(node))
                .into_iter()
                .flat_map(|node| node.parents().iter().copied().map(Node::from))
                .collect()
        },
        |node| {
            let node = node.existing().filter(|&node| is_root(node))?;
            Some(store.node(node)?.is_scene_owned())
        },
    )?
    .existing()
}

pub(super) fn proposed(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    object: Node,
    reachability: &SemanticExecutionReachability,
) -> Option<SemanticNodeId> {
    let group = outer_family(
        object,
        prepared
            .family_members(object)
            .is_ok_and(|members| !members.is_empty()),
        |node| prepared.family_parents(node).unwrap_or_default(),
        |node| {
            let node = node
                .existing()
                .filter(|&node| reachability.is_execution_root(node))?;
            Some(prepared.store().node(node)?.is_scene_owned())
        },
    )?;
    prepared.planned_node_id(group)
}

/// Only descendants of an added/removed edge can change outer ownership.
/// Existing live rows are patched before removals; new rows carry the same
/// proposed metadata in their CreateObject patch.
pub(super) fn prepare(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    reachability: &SemanticExecutionReachability,
    transaction: &mut ExecutionMutationTransaction,
) -> Result<(), SemanticLoweringError> {
    let store = prepared.store();
    let mut affected = BTreeSet::new();
    let mut proposed_nodes = Vec::new();
    for mutation in prepared.candidate_mutations() {
        let member = match *mutation {
            SemanticMutation::AddMember { family, member }
            | SemanticMutation::RemoveMember { family, member }
                if family
                    .existing()
                    .is_some_and(|node| reachability.is_reachable(node)) =>
            {
                if let Some(family) = family.existing() {
                    affected.insert(family);
                }
                member
            }
            SemanticMutation::RemoveNode { node }
                if node
                    .existing()
                    .is_some_and(|node| reachability.is_reachable(node)) =>
            {
                node
            }
            _ => continue,
        };
        if let Some(node) = member.existing() {
            affected.extend(store.ordered_leaf_nodes(node)?);
        }
        proposed_nodes.push(member);
    }
    let mut visited = HashSet::new();
    while let Some(node) = proposed_nodes.pop() {
        if prepared.node_is_removed(node) || !visited.insert(node) {
            continue;
        }
        affected.extend(node.existing());
        proposed_nodes.extend(prepared.family_members(node).unwrap_or_default());
    }
    for object in affected {
        if !reachability.is_object_reachable(object)
            || prepared.node_is_removed(object)
            || prepared.object_state(object).is_err()
        {
            continue;
        }
        let old = committed(store, object, |node| reachability.is_execution_root(node));
        let group = proposed(prepared, object.into(), reachability);
        if old != group {
            transaction.push(ExecutionPatch::SetViewerGroup {
                object: semantic_execution_object_id(object),
                group,
            });
        }
    }
    Ok(())
}
