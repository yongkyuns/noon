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
    let mut roots = Vec::new();
    let mut stack = remove_set.iter().copied().collect::<Vec<_>>();

    while let Some(id) = stack.pop() {
        if !affected.insert(id) {
            continue;
        }
        let node = target_node_checked(store, id)?;
        if node.is_scene_owned() {
            roots.push(id);
        }
        stack.extend(node.parents().iter().copied());
    }

    roots.sort_unstable();
    roots.dedup();
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
