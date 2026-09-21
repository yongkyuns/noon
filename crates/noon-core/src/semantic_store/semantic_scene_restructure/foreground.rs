//! Declaration policy shares the existing family projection walk, not a second graph.
use super::*;

/// Stable list update: existing occurrences are moved to the explicit tail.
/// Both inputs are already distinct, so no positional/key identity is invented.
pub(super) fn add_order(prefix: &[SemanticNodeId], tail: &[SemanticNodeId]) -> Vec<SemanticNodeId> {
    let tail_set = tail.iter().copied().collect::<HashSet<_>>();
    prefix
        .iter()
        .copied()
        .filter(|id| !tail_set.contains(id))
        .chain(tail.iter().copied())
        .collect()
}

/// Stage persistence cleanup beside an existing lifecycle/membership transaction.
///
/// This removes declarations, not objects. Removing a family also demotes its
/// declared descendants; removing a child promotes unaffected foreground branches
/// without mutating the original family. Submit all removals for a scope together
/// so the existing transaction owns one declaration assignment per scope.
pub fn stage_semantic_foreground_removal(
    store: &SemanticStore,
    scene_root: SemanticNodeId,
    removed: &[SemanticNodeId],
    transaction: &mut SemanticMutationTransaction,
) -> Result<(), SemanticSceneOperationError> {
    let root = target_node_checked(store, scene_root)?;
    if !matches!(root.kind(), SemanticNodeKind::Family(_)) {
        return Err(SemanticSceneOperationError::NotSemanticFamily(scene_root));
    }
    let removed = validated_distinct_nodes(store, removed)?;
    if root.foreground_members().is_empty() || removed.is_empty() {
        return Ok(());
    }
    let removal = downward_target_closure(store, &removed)?;
    let members = project_members(store, scene_root, &removal, None)?;
    if members != root.foreground_members() {
        transaction.set_foreground_members(scene_root, members);
    }
    Ok(())
}

pub(super) fn replace_members(
    store: &SemanticStore,
    scene_root: SemanticNodeId,
    old: SemanticNodeId,
    new: SemanticNodeId,
) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError> {
    let previous = target_node_checked(store, scene_root)?.foreground_members();
    if previous.is_empty() {
        return Ok(Vec::new());
    }
    let (affected, _) = affected_explicit_root_closure(store, scene_root, &HashSet::from([old]))?;
    let replaces_foreground = previous.iter().any(|id| affected.contains(id));
    let mut removal = downward_target_closure(store, &[old])?;
    let target_members = downward_target_closure(store, &[new])?;
    let replacement = if replaces_foreground {
        // The target takes the replaced foreground slot. Retire independently
        // declared source descendants too; their preserved family handles must
        // not cause a later add to resurrect the replaced source's contents.
        removal.extend(target_members);
        Some((old, new))
    } else {
        // A partially demoted family can still contain foreground declarations.
        // Remove those retired descendants, but keep declarations whose identity
        // survives in the target. Do not promote the whole target implicitly.
        removal.retain(|id| !target_members.contains(id));
        None
    };
    project_members(store, scene_root, &removal, replacement)
}

fn project_members(
    store: &SemanticStore,
    scene_root: SemanticNodeId,
    removal: &HashSet<SemanticNodeId>,
    replacement: Option<(SemanticNodeId, SemanticNodeId)>,
) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError> {
    let previous = target_node_checked(store, scene_root)?.foreground_members();
    let (affected, _) = affected_explicit_root_closure(store, scene_root, removal)?;
    let roots = previous.iter().copied().collect::<HashSet<_>>();
    let mut members = Vec::new();
    let mut unique = HashSet::new();
    for &root in previous {
        let mut promoted = HashSet::new();
        let mut projected = Vec::new();
        collect_root_replacements(
            store,
            root,
            root,
            removal,
            &affected,
            replacement,
            ProjectionBoundary::Declarations(&roots),
            &mut promoted,
            &mut projected,
        )?;
        for id in projected {
            if !unique.insert(id) {
                return Err(SemanticSceneOperationError::AmbiguousCrossRootAlias(id));
            }
            members.push(id);
        }
    }
    Ok(members)
}
