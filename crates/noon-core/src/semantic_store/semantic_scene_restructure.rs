use std::collections::{HashMap, HashSet};

use crate::{
    SemanticMutationTransaction, SemanticNode, SemanticNodeId, SemanticNodeKind,
    SemanticSceneOperationError, SemanticStore,
};

/// One atomic edit of an explicit semantic scene-root family's ordered projection.
#[derive(Clone, Copy, Debug)]
pub enum SemanticSceneMembershipRequest<'a> {
    Add(&'a [SemanticNodeId]),
    BringToBack(&'a [SemanticNodeId]),
    Remove(&'a [SemanticNodeId]),
    Clear,
    Replace {
        old: SemanticNodeId,
        new: SemanticNodeId,
    },
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
    let root = store
        .node(scene_root)
        .ok_or(SemanticSceneOperationError::UnknownNode(scene_root))?;
    if !matches!(root.kind(), SemanticNodeKind::Family(_)) {
        return Err(SemanticSceneOperationError::NotSemanticFamily(scene_root));
    }
    match request {
        SemanticSceneMembershipRequest::Clear => {
            let mut transaction = SemanticMutationTransaction::new();
            let mut member = root.first_member();
            while let Some(current) = member {
                member = root.next_member(current);
                transaction.remove_member(scene_root, current);
            }
            Ok(transaction)
        }
        SemanticSceneMembershipRequest::Add(ids) => {
            let explicit = validated_distinct_nodes(store, ids)?;
            let remove_set = downward_target_closure(store, &explicit)?;
            plan_explicit_root_projection(
                store,
                scene_root,
                &remove_set,
                None,
                &explicit,
                ExplicitPlacement::Tail,
            )
        }
        SemanticSceneMembershipRequest::BringToBack(ids) => {
            let explicit = validated_distinct_nodes(store, ids)?;
            let remove_set = downward_target_closure(store, &explicit)?;
            plan_explicit_root_projection(
                store,
                scene_root,
                &remove_set,
                None,
                &explicit,
                ExplicitPlacement::Head,
            )
        }
        SemanticSceneMembershipRequest::Remove(ids) => {
            let explicit = validated_distinct_nodes(store, ids)?;
            let remove_set = explicit.into_iter().collect();
            plan_explicit_root_projection(
                store,
                scene_root,
                &remove_set,
                None,
                &[],
                ExplicitPlacement::Tail,
            )
        }
        SemanticSceneMembershipRequest::Replace { old, new } => {
            target_node_checked(store, old)?;
            target_node_checked(store, new)?;
            if old == new {
                return Ok(SemanticMutationTransaction::new());
            }
            let occurrences = projected_root_path_count(store, scene_root, old)?;
            if occurrences == 0 {
                return Err(SemanticSceneOperationError::MissingMembershipTarget(old));
            }
            if occurrences != 1 {
                return Err(SemanticSceneOperationError::AmbiguousMembershipTarget(old));
            }
            let mut remove_set = downward_target_closure(store, &[new])?;
            remove_set.insert(old);
            plan_explicit_root_projection(
                store,
                scene_root,
                &remove_set,
                Some((old, new)),
                &[],
                ExplicitPlacement::Tail,
            )
        }
    }
}

fn projected_root_path_count(
    store: &SemanticStore,
    scene_root: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<usize, SemanticSceneOperationError> {
    let mut count = 0usize;
    let mut stack = vec![target];
    while let Some(node) = stack.pop() {
        for &parent in target_node_checked(store, node)?.parents() {
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

fn plan_explicit_root_projection(
    store: &SemanticStore,
    scene_root: SemanticNodeId,
    remove_set: &HashSet<SemanticNodeId>,
    replacement: Option<(SemanticNodeId, SemanticNodeId)>,
    explicit: &[SemanticNodeId],
    placement: ExplicitPlacement,
) -> Result<SemanticMutationTransaction, SemanticSceneOperationError> {
    let (affected, affected_roots) = affected_explicit_root_closure(store, scene_root, remove_set)?;
    let root_node = target_node_checked(store, scene_root)?;
    let mut run_heads = Vec::new();
    for &root in &affected_roots {
        if root_node
            .previous_member(root)
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
            before = before.and_then(|candidate| root_node.next_member(candidate));
        }
        let mut root = Some(run_head);
        while let Some(current) = root.filter(|root| affected_roots.contains(root)) {
            let mut promoted = HashSet::new();
            let mut replacements = Vec::new();
            collect_root_replacements(
                store,
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
            root = root_node.next_member(current);
        }
    }

    let head_anchor = if placement == ExplicitPlacement::Head {
        let first_replacement_by_root = plans
            .iter()
            .map(|(root, replacements, _)| (*root, replacements.first().copied()))
            .collect::<HashMap<_, _>>();
        first_projected_root_after_restructure(
            root_node,
            &affected_roots,
            &first_replacement_by_root,
        )
    } else {
        None
    };
    let retained_roots: HashSet<_> = explicit
        .iter()
        .copied()
        .filter(|member| root_node.contains_member(*member))
        .collect();
    let mut transaction = SemanticMutationTransaction::new();
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
    Ok(transaction)
}

fn first_projected_root_after_restructure(
    root: &SemanticNode,
    affected_roots: &HashSet<SemanticNodeId>,
    first_replacement_by_root: &HashMap<SemanticNodeId, Option<SemanticNodeId>>,
) -> Option<SemanticNodeId> {
    let mut current = root.first_member();
    while let Some(member) = current {
        if !affected_roots.contains(&member) {
            return Some(member);
        }
        if let Some(Some(first)) = first_replacement_by_root.get(&member) {
            return Some(*first);
        }
        current = root.next_member(member);
    }
    None
}

fn affected_explicit_root_closure(
    store: &SemanticStore,
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
        for &parent in target_node_checked(store, node)?.parents() {
            if parent == scene_root {
                roots.insert(node);
            } else {
                stack.push(parent);
            }
        }
    }
    Ok((affected, roots))
}

fn validated_distinct_nodes(
    store: &SemanticStore,
    ids: &[SemanticNodeId],
) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError> {
    let mut seen = HashSet::with_capacity(ids.len());
    for &id in ids {
        target_node_checked(store, id)?;
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
enum ProjectionBoundary {
    SceneRoots,
    Family(SemanticNodeId),
}

impl ProjectionBoundary {
    fn contains(
        self,
        store: &SemanticStore,
        current: SemanticNodeId,
    ) -> Result<bool, SemanticSceneOperationError> {
        Ok(match self {
            Self::SceneRoots => target_node_checked(store, current)?.is_scene_owned(),
            Self::Family(root) => target_node_checked(store, root)?.contains_member(current),
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

        let remove_set = downward_target_closure(self, &explicit)?;
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

fn downward_target_closure(
    store: &SemanticStore,
    roots: &[SemanticNodeId],
) -> Result<HashSet<SemanticNodeId>, SemanticSceneOperationError> {
    let mut closure = HashSet::new();
    let mut stack = roots.to_vec();
    while let Some(id) = stack.pop() {
        if !closure.insert(id) {
            continue;
        }
        let node = target_node_checked(store, id)?;
        if matches!(node.kind(), SemanticNodeKind::Family(_)) {
            stack.extend(node.members());
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
            store,
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
fn collect_root_replacements(
    store: &SemanticStore,
    current: SemanticNodeId,
    current_root: SemanticNodeId,
    remove_set: &HashSet<SemanticNodeId>,
    affected: &HashSet<SemanticNodeId>,
    replacement: Option<(SemanticNodeId, SemanticNodeId)>,
    boundary: ProjectionBoundary,
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

    let node = target_node_checked(store, current)?;
    if current != current_root && boundary.contains(store, current)? {
        return Ok(());
    }

    if !affected.contains(&current) {
        if promoted.insert(current) {
            output.push(current);
        }
        return Ok(());
    }

    if matches!(node.kind(), SemanticNodeKind::Family(_)) {
        for member in node.members_iter() {
            collect_root_replacements(
                store,
                member,
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
        SemanticMutationStats, SemanticNodeResidency, SemanticObjectState, StoredGeometry,
    };

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
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
