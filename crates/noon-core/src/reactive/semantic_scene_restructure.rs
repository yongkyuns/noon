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
        let explicit = validated_unique_nodes(self, ids)?;
        if explicit.is_empty() {
            self.set_last_mutation_writes(0);
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
        let remove_set = validated_unique_nodes(self, ids)?
            .into_iter()
            .collect::<HashSet<_>>();
        if remove_set.is_empty() {
            self.set_last_mutation_writes(0);
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
    let mut stack = remove_set.iter().copied().collect::<Vec<_>>();

    while let Some(id) = stack.pop() {
        if !affected.insert(id) {
            continue;
        }
        let node = target_node_checked(store, id)?;
        if node.is_scene_owned() {
            root_set.insert(id);
        }
        stack.extend(node.parents().iter().copied());
    }

    // Cross-root aliases require one deterministic first occurrence. The linked
    // authored root list is the ordering authority, so NodeId allocation order is
    // never used as a proxy for current scene order. Most operations affect at most
    // one root and avoid this scan entirely.
    let roots = if root_set.len() <= 1 {
        root_set.iter().copied().collect::<Vec<_>>()
    } else {
        store
            .scene_roots()
            .filter(|id| root_set.contains(id))
            .collect::<Vec<_>>()
    };
    debug_assert_eq!(roots.len(), root_set.len());
    Ok((affected, roots))
}

fn plan_scene_restructure(
    store: &SemanticStore,
    remove_set: &HashSet<SemanticNodeId>,
) -> Result<Vec<SceneRootProjection>, SemanticSceneOperationError> {
    let (affected, affected_roots) = affected_ancestor_closure(store, remove_set)?;
    let mut promoted = HashSet::new();
    let mut plans = Vec::with_capacity(affected_roots.len());

    for root in affected_roots {
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
    use crate::{SemanticNodeResidency, SemanticObjectState, StoredGeometry};

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
        assert_eq!(store.scene_roots().collect::<Vec<_>>(), vec![existing]);
        assert_eq!(
            store.node(valid).unwrap().residency(),
            SemanticNodeResidency::Detached
        );
    }

    #[test]
    fn cross_root_alias_dedup_uses_current_scene_order_not_node_id_order() {
        let mut store = SemanticStore::new();
        let removed = object(&mut store, 1.0);
        let shared = object(&mut store, 2.0);
        let tail = object(&mut store, 3.0);
        let older_family = store.insert_family();
        let newer_family = store.insert_family();

        for family in [older_family, newer_family] {
            store.add_semantic_family_member(family, removed).unwrap();
            store.add_semantic_family_member(family, shared).unwrap();
        }

        // Storage primitives can represent aliased family roots. Re-attach the
        // lower-NodeId family last so current authored root order intentionally
        // disagrees with allocation order.
        store.attach_to_scene(older_family).unwrap();
        store.attach_to_scene(newer_family).unwrap();
        store.attach_semantic_object(tail).unwrap();
        store.detach_from_scene(older_family).unwrap();
        store.attach_to_scene(older_family).unwrap();
        assert!(older_family < newer_family);
        assert_eq!(
            store.scene_roots().collect::<Vec<_>>(),
            vec![newer_family, tail, older_family]
        );

        store.remove_semantic_scene_nodes(&[removed]).unwrap();

        assert_eq!(
            store.scene_roots().collect::<Vec<_>>(),
            vec![shared, tail]
        );
    }
}
