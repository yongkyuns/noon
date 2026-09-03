use std::collections::HashSet;

use super::{
    SemanticNode, SemanticNodeId, SemanticNodeKind, SemanticSceneOperationError, SemanticStore,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct SceneRootProjection {
    root: SemanticNodeId,
    replacements: Vec<SemanticNodeId>,
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
        SemanticNodeKind::Family => true,
        SemanticNodeKind::AuthoringObject => node.semantic_object_state().is_some(),
        SemanticNodeKind::Object(_) => false,
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
        if matches!(node.kind(), SemanticNodeKind::Family) {
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
    let mut conflict = None;
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
    promoted: &mut HashSet<SemanticNodeId>,
    output: &mut Vec<SemanticNodeId>,
) -> Result<(), SemanticSceneOperationError> {
    if remove_set.contains(&current) {
        return Ok(());
    }

    let node = target_node_checked(store, current)?;
    if current != current_root && node.is_scene_owned() {
        return Ok(());
    }

    if !affected.contains(&current) {
        if promoted.insert(current) {
            output.push(current);
        }
        return Ok(());
    }

    if matches!(node.kind(), SemanticNodeKind::Family) {
        for member in node.members() {
            collect_root_replacements(
                store,
                member,
                current_root,
                remove_set,
                affected,
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
    use crate::{SemanticMutationStats, SemanticNodeResidency, SemanticObjectState, StoredGeometry};

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
        assert_eq!(store.last_mutation_stats(), SemanticMutationStats::default());
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

        assert_eq!(store.last_mutation_stats(), SemanticMutationStats::default());
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
}
