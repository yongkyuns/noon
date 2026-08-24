use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{ObjectDefinition, ObjectId, SceneDefinition};

/// Stable semantic identity independent of execution/render dense indices.
///
/// Reusing a vacant slot increments its generation, so stale handles never
/// silently refer to a different semantic object after deletion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SemanticNodeId {
    slot: u32,
    generation: u32,
}

impl SemanticNodeId {
    pub const fn new(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }

    pub const fn slot(self) -> u32 {
        self.slot
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// Identity used to reconcile a node across source re-execution.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceIdentity {
    ExplicitKey(String),
    SourcePath {
        file: String,
        line: u32,
        column: u32,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        lexical_path: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticNodeKind {
    /// Compatibility payload while `SceneDefinition` consumers migrate.
    Object(ObjectDefinition),
    /// A semantic family/collection with no implied transform ownership.
    Family,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    id: SemanticNodeId,
    kind: SemanticNodeKind,
    source_identity: Option<SourceIdentity>,
    /// Families containing this node. Multiple parents are intentional and
    /// preserve Manim-style aliasing/reference semantics.
    parents: Vec<SemanticNodeId>,
    /// Ordered family membership. Ordering is semantic/presentation relevant,
    /// but does not imply ownership of the child's transform.
    members: Vec<SemanticNodeId>,
}

impl SemanticNode {
    pub const fn id(&self) -> SemanticNodeId {
        self.id
    }

    pub fn kind(&self) -> &SemanticNodeKind {
        &self.kind
    }

    pub fn kind_mut(&mut self) -> &mut SemanticNodeKind {
        &mut self.kind
    }

    pub fn object(&self) -> Option<&ObjectDefinition> {
        match &self.kind {
            SemanticNodeKind::Object(object) => Some(object),
            SemanticNodeKind::Family => None,
        }
    }

    pub fn object_mut(&mut self) -> Option<&mut ObjectDefinition> {
        match &mut self.kind {
            SemanticNodeKind::Object(object) => Some(object),
            SemanticNodeKind::Family => None,
        }
    }

    pub fn source_identity(&self) -> Option<&SourceIdentity> {
        self.source_identity.as_ref()
    }

    pub fn parents(&self) -> &[SemanticNodeId] {
        &self.parents
    }

    pub fn members(&self) -> &[SemanticNodeId] {
        &self.members
    }
}

#[derive(Clone, Debug)]
struct SemanticSlot {
    generation: u32,
    node: Option<SemanticNode>,
    next_free: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SemanticMutationStats {
    /// Direct slots changed by the most recent operation. This excludes nodes
    /// visited only for cycle validation.
    pub slots_written: usize,
    /// Nodes inspected while validating a family cycle.
    pub cycle_nodes_visited: usize,
}

#[derive(Clone, Debug, Default)]
pub struct SemanticStore {
    slots: Vec<SemanticSlot>,
    free_head: Option<u32>,
    live_nodes: usize,
    object_nodes: HashMap<ObjectId, SemanticNodeId>,
    source_nodes: HashMap<SourceIdentity, SemanticNodeId>,
    last_mutation: SemanticMutationStats,
}

impl SemanticStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compatibility adapter for the current flat scene model.
    pub fn from_scene_definition(scene: &SceneDefinition) -> Self {
        let mut store = Self::new();
        for object in scene.objects() {
            store.insert_object(object.clone());
        }
        store
    }

    pub fn insert_object(&mut self, object: ObjectDefinition) -> SemanticNodeId {
        let legacy_id = object.id;
        let id = self.insert_kind(SemanticNodeKind::Object(object));
        self.object_nodes.insert(legacy_id, id);
        id
    }

    pub fn insert_family(&mut self) -> SemanticNodeId {
        self.insert_kind(SemanticNodeKind::Family)
    }

    fn insert_kind(&mut self, kind: SemanticNodeKind) -> SemanticNodeId {
        let (slot_index, generation) = if let Some(slot_index) = self.free_head {
            let slot = &mut self.slots[slot_index as usize];
            self.free_head = slot.next_free.take();
            (slot_index, slot.generation)
        } else {
            let slot_index =
                u32::try_from(self.slots.len()).expect("Noon semantic node slot space exhausted");
            self.slots.push(SemanticSlot {
                generation: 0,
                node: None,
                next_free: None,
            });
            (slot_index, 0)
        };
        let id = SemanticNodeId::new(slot_index, generation);
        self.slots[slot_index as usize].node = Some(SemanticNode {
            id,
            kind,
            source_identity: None,
            parents: Vec::new(),
            members: Vec::new(),
        });
        self.live_nodes += 1;
        self.last_mutation = SemanticMutationStats {
            slots_written: 1,
            cycle_nodes_visited: 0,
        };
        id
    }

    pub fn node(&self, id: SemanticNodeId) -> Option<&SemanticNode> {
        let slot = self.slots.get(id.slot as usize)?;
        (slot.generation == id.generation)
            .then_some(slot.node.as_ref())
            .flatten()
    }

    pub fn node_mut(&mut self, id: SemanticNodeId) -> Option<&mut SemanticNode> {
        let slot = self.slots.get_mut(id.slot as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.node.as_mut()
    }

    pub fn node_for_object(&self, object: ObjectId) -> Option<SemanticNodeId> {
        self.object_nodes
            .get(&object)
            .copied()
            .filter(|id| self.node(*id).is_some())
    }

    pub fn node_for_source(&self, source: &SourceIdentity) -> Option<SemanticNodeId> {
        self.source_nodes
            .get(source)
            .copied()
            .filter(|id| self.node(*id).is_some())
    }

    pub fn set_source_identity(
        &mut self,
        id: SemanticNodeId,
        source: Option<SourceIdentity>,
    ) -> Result<(), SemanticStoreError> {
        let previous = self
            .node(id)
            .ok_or(SemanticStoreError::UnknownNode(id))?
            .source_identity
            .clone();
        if let Some(source) = &source {
            if let Some(existing) = self.node_for_source(source) {
                if existing != id {
                    return Err(SemanticStoreError::DuplicateSourceIdentity(source.clone()));
                }
            }
        }
        if let Some(previous) = previous {
            self.source_nodes.remove(&previous);
        }
        if let Some(source) = &source {
            self.source_nodes.insert(source.clone(), id);
        }
        self.node_mut(id)
            .expect("node existence validated above")
            .source_identity = source;
        self.last_mutation = SemanticMutationStats {
            slots_written: 1,
            cycle_nodes_visited: 0,
        };
        Ok(())
    }

    /// Add an ordered family edge. A member may belong to multiple families.
    pub fn add_member(
        &mut self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<(), SemanticStoreError> {
        if !matches!(
            self.node(family).map(SemanticNode::kind),
            Some(SemanticNodeKind::Family)
        ) {
            return Err(SemanticStoreError::NotFamily(family));
        }
        if self.node(member).is_none() {
            return Err(SemanticStoreError::UnknownNode(member));
        }
        if family == member {
            return Err(SemanticStoreError::FamilyCycle { family, member });
        }
        if self
            .node(family)
            .expect("family validated above")
            .members
            .contains(&member)
        {
            self.last_mutation = SemanticMutationStats::default();
            return Ok(());
        }

        let (creates_cycle, visited) = self.reaches(member, family);
        if creates_cycle {
            self.last_mutation = SemanticMutationStats {
                slots_written: 0,
                cycle_nodes_visited: visited,
            };
            return Err(SemanticStoreError::FamilyCycle { family, member });
        }

        self.node_mut(family)
            .expect("family validated above")
            .members
            .push(member);
        self.node_mut(member)
            .expect("member validated above")
            .parents
            .push(family);
        self.last_mutation = SemanticMutationStats {
            slots_written: 2,
            cycle_nodes_visited: visited,
        };
        Ok(())
    }

    pub fn remove_member(
        &mut self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<bool, SemanticStoreError> {
        if !matches!(
            self.node(family).map(SemanticNode::kind),
            Some(SemanticNodeKind::Family)
        ) {
            return Err(SemanticStoreError::NotFamily(family));
        }
        if self.node(member).is_none() {
            return Err(SemanticStoreError::UnknownNode(member));
        }
        let family_members = &mut self
            .node_mut(family)
            .expect("family validated above")
            .members;
        let Some(position) = family_members.iter().position(|id| *id == member) else {
            self.last_mutation = SemanticMutationStats::default();
            return Ok(false);
        };
        family_members.remove(position);
        let parents = &mut self
            .node_mut(member)
            .expect("member validated above")
            .parents;
        if let Some(position) = parents.iter().position(|id| *id == family) {
            parents.remove(position);
        }
        self.last_mutation = SemanticMutationStats {
            slots_written: 2,
            cycle_nodes_visited: 0,
        };
        Ok(true)
    }

    /// Remove a node without renumbering any unrelated semantic identity.
    ///
    /// Cost is proportional to this node's direct family edges, never the total
    /// number of nodes in the store.
    pub fn remove_node(&mut self, id: SemanticNodeId) -> Result<SemanticNode, SemanticStoreError> {
        let node = self
            .node(id)
            .ok_or(SemanticStoreError::UnknownNode(id))?
            .clone();
        let mut writes = 1;
        for parent in node.parents.iter().copied() {
            if let Some(parent_node) = self.node_mut(parent) {
                parent_node.members.retain(|member| *member != id);
                writes += 1;
            }
        }
        for member in node.members.iter().copied() {
            if let Some(member_node) = self.node_mut(member) {
                member_node.parents.retain(|parent| *parent != id);
                writes += 1;
            }
        }
        if let SemanticNodeKind::Object(object) = &node.kind {
            self.object_nodes.remove(&object.id);
        }
        if let Some(source) = &node.source_identity {
            self.source_nodes.remove(source);
        }

        let slot = &mut self.slots[id.slot as usize];
        let removed = slot.node.take().expect("node existence validated above");
        slot.generation = slot
            .generation
            .checked_add(1)
            .expect("Noon semantic node generation space exhausted");
        slot.next_free = self.free_head;
        self.free_head = Some(id.slot);
        self.live_nodes -= 1;
        self.last_mutation = SemanticMutationStats {
            slots_written: writes,
            cycle_nodes_visited: 0,
        };
        Ok(removed)
    }

    fn reaches(&self, start: SemanticNodeId, target: SemanticNodeId) -> (bool, usize) {
        let mut stack = vec![start];
        let mut seen = HashSet::new();
        let mut visited = 0;
        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }
            visited += 1;
            if current == target {
                return (true, visited);
            }
            if let Some(node) = self.node(current) {
                stack.extend(node.members.iter().copied());
            }
        }
        (false, visited)
    }

    pub fn len(&self) -> usize {
        self.live_nodes
    }

    pub fn is_empty(&self) -> bool {
        self.live_nodes == 0
    }

    pub fn slot_capacity(&self) -> usize {
        self.slots.len()
    }

    pub const fn last_mutation_stats(&self) -> SemanticMutationStats {
        self.last_mutation
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticStoreError {
    UnknownNode(SemanticNodeId),
    NotFamily(SemanticNodeId),
    FamilyCycle {
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    DuplicateSourceIdentity(SourceIdentity),
}

impl std::fmt::Display for SemanticStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNode(id) => write!(
                formatter,
                "unknown semantic node {}:{}",
                id.slot(),
                id.generation()
            ),
            Self::NotFamily(id) => write!(
                formatter,
                "semantic node {}:{} is not a family",
                id.slot(),
                id.generation()
            ),
            Self::FamilyCycle { family, member } => write!(
                formatter,
                "adding semantic node {}:{} to family {}:{} would create a cycle",
                member.slot(),
                member.generation(),
                family.slot(),
                family.generation()
            ),
            Self::DuplicateSourceIdentity(source) => {
                write!(formatter, "duplicate semantic source identity {source:?}")
            }
        }
    }
}

impl std::error::Error for SemanticStoreError {}

#[cfg(test)]
mod tests {
    use crate::GeometryRef;

    use super::*;

    fn object(id: u64) -> ObjectDefinition {
        ObjectDefinition::new(ObjectId::new(id), GeometryRef::circle(1.0))
    }

    #[test]
    fn deletion_does_not_renumber_unrelated_semantic_handles() {
        let mut store = SemanticStore::new();
        let ids = (0..100_000)
            .map(|index| store.insert_object(object(index)))
            .collect::<Vec<_>>();
        let tail = ids[99_999];
        store.remove_node(ids[10]).unwrap();
        assert_eq!(store.node(tail).unwrap().id(), tail);
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(store.len(), 99_999);
    }

    #[test]
    fn reused_slot_invalidates_stale_generation() {
        let mut store = SemanticStore::new();
        let first = store.insert_object(object(1));
        store.remove_node(first).unwrap();
        let second = store.insert_object(object(2));
        assert_eq!(first.slot(), second.slot());
        assert_ne!(first.generation(), second.generation());
        assert!(store.node(first).is_none());
        assert!(store.node(second).is_some());
    }

    #[test]
    fn family_membership_allows_aliasing_without_transform_ownership() {
        let mut store = SemanticStore::new();
        let first_family = store.insert_family();
        let second_family = store.insert_family();
        let child = store.insert_object(object(1));
        store.add_member(first_family, child).unwrap();
        store.add_member(second_family, child).unwrap();
        assert_eq!(
            store.node(child).unwrap().parents(),
            &[first_family, second_family]
        );
        assert_eq!(store.node(first_family).unwrap().members(), &[child]);
        assert_eq!(store.node(second_family).unwrap().members(), &[child]);
    }

    #[test]
    fn family_cycles_are_rejected() {
        let mut store = SemanticStore::new();
        let outer = store.insert_family();
        let inner = store.insert_family();
        store.add_member(outer, inner).unwrap();
        assert!(matches!(
            store.add_member(inner, outer),
            Err(SemanticStoreError::FamilyCycle { .. })
        ));
    }

    #[test]
    fn source_identity_is_unique_and_stable() {
        let mut store = SemanticStore::new();
        let first = store.insert_object(object(1));
        let second = store.insert_object(object(2));
        let source = SourceIdentity::ExplicitKey("hero".into());
        store
            .set_source_identity(first, Some(source.clone()))
            .unwrap();
        assert_eq!(store.node_for_source(&source), Some(first));
        assert!(matches!(
            store.set_source_identity(second, Some(source)),
            Err(SemanticStoreError::DuplicateSourceIdentity(_))
        ));
    }

    #[test]
    fn flat_scene_adapter_preserves_legacy_object_lookup() {
        let mut scene = SceneDefinition::new();
        let first = scene.add(GeometryRef::circle(1.0));
        let second = scene.add(GeometryRef::rectangle(2.0, 1.0));
        let store = SemanticStore::from_scene_definition(&scene);
        assert!(store.node(store.node_for_object(first).unwrap()).is_some());
        assert!(store.node(store.node_for_object(second).unwrap()).is_some());
    }
}
