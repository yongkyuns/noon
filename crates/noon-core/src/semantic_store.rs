use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::SemanticObjectState;
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

/// Whether a semantic node currently participates in authored top-level scene membership.
///
/// This is the semantic view only. Intrusive ordering links remain private storage
/// detail so they cannot become an accidental frontend or serialization contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticNodeResidency {
    #[default]
    Detached,
    SceneOwned,
}

/// Authoritative storage for top-level scene membership and order.
///
/// Membership and ordering are one representation: an attached node carries its
/// previous/next links; a detached node carries no scene-order state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SemanticSceneMembership {
    #[default]
    Detached,
    Attached {
        previous: Option<SemanticNodeId>,
        next: Option<SemanticNodeId>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FamilyMemberLink {
    previous: Option<SemanticNodeId>,
    next: Option<SemanticNodeId>,
}

/// Ordered family membership with local lookup/add/remove.
///
/// The hash map is identity lookup only; deterministic semantic order follows
/// the private previous/next links from `head` to `tail`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OrderedFamilyMembers {
    head: Option<SemanticNodeId>,
    tail: Option<SemanticNodeId>,
    links: HashMap<SemanticNodeId, FamilyMemberLink>,
}

impl OrderedFamilyMembers {
    fn contains(&self, member: SemanticNodeId) -> bool {
        self.links.contains_key(&member)
    }

    fn push(&mut self, member: SemanticNodeId) -> bool {
        if self.contains(member) {
            return false;
        }
        let previous = self.tail;
        if let Some(previous_id) = previous {
            self.links
                .get_mut(&previous_id)
                .expect("family tail must have a membership link")
                .next = Some(member);
        } else {
            debug_assert!(self.head.is_none());
            self.head = Some(member);
        }
        self.links.insert(
            member,
            FamilyMemberLink {
                previous,
                next: None,
            },
        );
        self.tail = Some(member);
        true
    }

    fn remove(&mut self, member: SemanticNodeId) -> bool {
        let Some(link) = self.links.remove(&member) else {
            return false;
        };
        if let Some(previous_id) = link.previous {
            self.links
                .get_mut(&previous_id)
                .expect("family previous link must exist")
                .next = link.next;
        } else {
            debug_assert_eq!(self.head, Some(member));
            self.head = link.next;
        }
        if let Some(next_id) = link.next {
            self.links
                .get_mut(&next_id)
                .expect("family next link must exist")
                .previous = link.previous;
        } else {
            debug_assert_eq!(self.tail, Some(member));
            self.tail = link.previous;
        }
        if self.is_empty() {
            debug_assert!(self.head.is_none());
            debug_assert!(self.tail.is_none());
        }
        true
    }

    fn iter(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        std::iter::successors(self.head, move |id| {
            self.links.get(id).and_then(|link| link.next)
        })
    }

    fn len(&self) -> usize {
        self.links.len()
    }

    fn is_empty(&self) -> bool {
        self.links.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticNodeKind {
    /// Compatibility payload while `SceneDefinition` consumers migrate.
    Object(ObjectDefinition),
    /// Semantic object identity. Target objects carry `SemanticObjectState`
    /// directly on the node. State-less instances exist only for the temporary
    /// frontend identity seam and are owned for migration by #61/#959.
    AuthoringObject,
    /// A semantic family/collection with no implied transform ownership.
    Family,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    id: SemanticNodeId,
    kind: SemanticNodeKind,
    /// Authoritative authored object payload for target semantic objects.
    ///
    /// `None` is valid for families, legacy compatibility objects, and the
    /// temporary state-less frontend identity seam only.
    object_state: Option<SemanticObjectState>,
    source_identity: Option<SourceIdentity>,
    scene_membership: SemanticSceneMembership,
    /// Families containing this node. Multiple parents are intentional and
    /// preserve Manim-style aliasing/reference semantics. This list contains only
    /// direct relationships, so scans are proportional to this node's own degree.
    parents: Vec<SemanticNodeId>,
    /// Ordered direct family membership with O(1) identity removal and append.
    members: OrderedFamilyMembers,
}

impl SemanticNode {
    pub const fn id(&self) -> SemanticNodeId {
        self.id
    }

    pub fn kind(&self) -> &SemanticNodeKind {
        &self.kind
    }

    pub fn object(&self) -> Option<&ObjectDefinition> {
        match &self.kind {
            SemanticNodeKind::Object(object) => Some(object),
            SemanticNodeKind::AuthoringObject | SemanticNodeKind::Family => None,
        }
    }

    pub fn object_mut(&mut self) -> Option<&mut ObjectDefinition> {
        match &mut self.kind {
            SemanticNodeKind::Object(object) => Some(object),
            SemanticNodeKind::AuthoringObject | SemanticNodeKind::Family => None,
        }
    }

    pub fn semantic_object_state(&self) -> Option<&SemanticObjectState> {
        self.object_state.as_ref()
    }

    pub fn semantic_object_state_mut(&mut self) -> Option<&mut SemanticObjectState> {
        self.object_state.as_mut()
    }

    pub fn source_identity(&self) -> Option<&SourceIdentity> {
        self.source_identity.as_ref()
    }

    pub const fn residency(&self) -> SemanticNodeResidency {
        match self.scene_membership {
            SemanticSceneMembership::Detached => SemanticNodeResidency::Detached,
            SemanticSceneMembership::Attached { .. } => SemanticNodeResidency::SceneOwned,
        }
    }

    pub const fn is_scene_owned(&self) -> bool {
        matches!(
            self.scene_membership,
            SemanticSceneMembership::Attached { .. }
        )
    }

    const fn scene_next(&self) -> Option<SemanticNodeId> {
        match self.scene_membership {
            SemanticSceneMembership::Detached => None,
            SemanticSceneMembership::Attached { next, .. } => next,
        }
    }

    pub fn parents(&self) -> &[SemanticNodeId] {
        &self.parents
    }

    /// Snapshot direct members in deterministic family order.
    ///
    /// The allocation is query-only; mutation storage remains linked and local.
    pub fn members(&self) -> Vec<SemanticNodeId> {
        self.members.iter().collect()
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
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
    /// Direct semantic slots changed by the most recent operation. This excludes
    /// store-level head/tail/count metadata and nodes visited only for validation.
    pub slots_written: usize,
    /// Nodes inspected while validating a family cycle.
    pub cycle_nodes_visited: usize,
}

#[derive(Clone, Debug, Default)]
pub struct SemanticStore {
    slots: Vec<SemanticSlot>,
    free_head: Option<u32>,
    live_nodes: usize,
    scene_head: Option<SemanticNodeId>,
    scene_tail: Option<SemanticNodeId>,
    scene_nodes: usize,
    next_insertion_order: u64,
    object_nodes: HashMap<ObjectId, SemanticNodeId>,
    source_nodes: HashMap<SourceIdentity, SemanticNodeId>,
    last_mutation: SemanticMutationStats,
}

impl SemanticStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compatibility adapter for the current flat scene model.
    ///
    /// This adapter is migration-only and is owned for deletion by #959/A4.
    /// Objects are attached in legacy authored order so current callers remain
    /// coherent while the authoritative semantic authoring path replaces it.
    pub fn from_scene_definition(scene: &SceneDefinition) -> Self {
        let mut store = Self::new();
        for object in scene.objects() {
            let id = store.insert_object(object.clone());
            store
                .attach_to_scene(id)
                .expect("newly inserted compatibility node exists");
        }
        store
    }

    pub fn insert_object(&mut self, object: ObjectDefinition) -> SemanticNodeId {
        let legacy_id = object.id;
        let id = self.insert_kind(SemanticNodeKind::Object(object));
        self.object_nodes.insert(legacy_id, id);
        id
    }

    /// Insert a target semantic object whose authored payload is owned directly by
    /// the authoritative node. Stable painter insertion order is assigned here so
    /// frontends cannot manufacture conflicting tie-break values.
    pub fn insert_semantic_object(&mut self, mut state: SemanticObjectState) -> SemanticNodeId {
        let insertion_order = self.next_insertion_order;
        self.next_insertion_order = self
            .next_insertion_order
            .checked_add(1)
            .expect("Noon semantic insertion-order space exhausted");
        state.assign_insertion_order(insertion_order);
        let id = self.insert_kind(SemanticNodeKind::AuthoringObject);
        self.node_mut(id)
            .expect("newly inserted semantic node exists")
            .object_state = Some(state);
        id
    }

    /// Temporary identity-only frontend seam. New target object authoring should
    /// use `insert_semantic_object`; #61/#959 own migration/removal of this path.
    pub fn insert_authoring_object(&mut self) -> SemanticNodeId {
        self.insert_kind(SemanticNodeKind::AuthoringObject)
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
            object_state: None,
            source_identity: None,
            scene_membership: SemanticSceneMembership::Detached,
            parents: Vec::new(),
            members: OrderedFamilyMembers::default(),
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

    /// Attach an existing semantic node as the last authored top-level scene root.
    ///
    /// Identity, source identity and family relationships are preserved. The
    /// operation writes only this node and the former tail node, if any.
    pub fn attach_to_scene(&mut self, id: SemanticNodeId) -> Result<bool, SemanticStoreError> {
        let membership = self
            .node(id)
            .ok_or(SemanticStoreError::UnknownNode(id))?
            .scene_membership;
        if !matches!(membership, SemanticSceneMembership::Detached) {
            self.last_mutation = SemanticMutationStats::default();
            return Ok(false);
        }

        let previous = self.scene_tail;
        if let Some(previous_id) = previous {
            let previous_node = self
                .node_mut(previous_id)
                .expect("scene tail must reference a live semantic node");
            match &mut previous_node.scene_membership {
                SemanticSceneMembership::Attached { next, .. } => {
                    debug_assert!(next.is_none());
                    *next = Some(id);
                }
                SemanticSceneMembership::Detached => {
                    unreachable!("scene tail cannot reference a detached semantic node")
                }
            }
        } else {
            debug_assert!(self.scene_head.is_none());
            self.scene_head = Some(id);
        }

        self.node_mut(id)
            .expect("node existence validated above")
            .scene_membership = SemanticSceneMembership::Attached {
            previous,
            next: None,
        };
        self.scene_tail = Some(id);
        self.scene_nodes += 1;
        self.last_mutation = SemanticMutationStats {
            slots_written: 1 + (previous.is_some() as usize),
            cycle_nodes_visited: 0,
        };
        Ok(true)
    }

    /// Detach an existing semantic node from authored top-level scene membership.
    ///
    /// The handle remains valid and can later be re-attached. Only the node and
    /// its immediate root-order neighbors are written.
    pub fn detach_from_scene(&mut self, id: SemanticNodeId) -> Result<bool, SemanticStoreError> {
        let membership = self
            .node(id)
            .ok_or(SemanticStoreError::UnknownNode(id))?
            .scene_membership;
        let SemanticSceneMembership::Attached { previous, next } = membership else {
            self.last_mutation = SemanticMutationStats::default();
            return Ok(false);
        };

        if let Some(previous_id) = previous {
            let previous_node = self
                .node_mut(previous_id)
                .expect("scene previous link must reference a live semantic node");
            match &mut previous_node.scene_membership {
                SemanticSceneMembership::Attached {
                    next: previous_next,
                    ..
                } => {
                    debug_assert_eq!(*previous_next, Some(id));
                    *previous_next = next;
                }
                SemanticSceneMembership::Detached => {
                    unreachable!("attached node cannot have a detached previous root")
                }
            }
        } else {
            debug_assert_eq!(self.scene_head, Some(id));
            self.scene_head = next;
        }

        if let Some(next_id) = next {
            let next_node = self
                .node_mut(next_id)
                .expect("scene next link must reference a live semantic node");
            match &mut next_node.scene_membership {
                SemanticSceneMembership::Attached {
                    previous: next_previous,
                    ..
                } => {
                    debug_assert_eq!(*next_previous, Some(id));
                    *next_previous = previous;
                }
                SemanticSceneMembership::Detached => {
                    unreachable!("attached node cannot have a detached next root")
                }
            }
        } else {
            debug_assert_eq!(self.scene_tail, Some(id));
            self.scene_tail = previous;
        }

        self.node_mut(id)
            .expect("node existence validated above")
            .scene_membership = SemanticSceneMembership::Detached;
        self.scene_nodes -= 1;
        if self.scene_nodes == 0 {
            debug_assert!(self.scene_head.is_none());
            debug_assert!(self.scene_tail.is_none());
        }
        self.last_mutation = SemanticMutationStats {
            slots_written: 1 + (previous.is_some() as usize) + (next.is_some() as usize),
            cycle_nodes_visited: 0,
        };
        Ok(true)
    }

    /// Iterate current top-level authored scene roots in deterministic authored order.
    pub fn scene_roots(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        std::iter::successors(self.scene_head, move |id| {
            self.node(*id).and_then(SemanticNode::scene_next)
        })
    }

    pub const fn scene_root_count(&self) -> usize {
        self.scene_nodes
    }

    /// Add an ordered family edge. A member may belong to multiple families.
    ///
    /// Direct duplicate lookup and ordered append are O(1); cycle validation is
    /// proportional to the affected reachable family subgraph.
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
            .contains(member)
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

        let inserted = self
            .node_mut(family)
            .expect("family validated above")
            .members
            .push(member);
        debug_assert!(inserted);
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

    /// Remove one direct family edge without scanning unrelated siblings.
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
        let removed = self
            .node_mut(family)
            .expect("family validated above")
            .members
            .remove(member);
        if !removed {
            self.last_mutation = SemanticMutationStats::default();
            return Ok(false);
        }
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
    /// Attached root membership is removed locally first. Remaining work is
    /// proportional to this node's direct parent/member relationships plus free-list
    /// bookkeeping; family sibling lists are never scanned for this removal.
    pub fn remove_node(&mut self, id: SemanticNodeId) -> Result<SemanticNode, SemanticStoreError> {
        let node = self
            .node(id)
            .ok_or(SemanticStoreError::UnknownNode(id))?
            .clone();

        let mut writes = 0;
        if node.is_scene_owned() {
            self.detach_from_scene(id)?;
            writes += self.last_mutation.slots_written;
        }

        for parent in node.parents.iter().copied() {
            if let Some(parent_node) = self.node_mut(parent) {
                let removed = parent_node.members.remove(id);
                debug_assert!(removed);
                writes += 1;
            }
        }
        for member in node.members.iter() {
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
        writes += 1;
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
                stack.extend(node.members.iter());
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
    fn authoring_objects_have_stable_shared_family_identity() {
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let family = store.insert_family();
        let alias = store.insert_family();

        store.add_member(family, first).unwrap();
        store.add_member(family, second).unwrap();
        store.add_member(alias, first).unwrap();
        assert_eq!(store.node(family).unwrap().members(), vec![first, second]);
        assert_eq!(store.node(first).unwrap().parents(), &[family, alias]);

        store.add_member(family, first).unwrap();
        assert_eq!(store.node(family).unwrap().members(), vec![first, second]);
        assert_eq!(store.node(first).unwrap().parents(), &[family, alias]);

        assert!(store.remove_member(family, first).unwrap());
        assert_eq!(store.node(family).unwrap().members(), vec![second]);
        assert_eq!(store.node(first).unwrap().parents(), &[alias]);
        assert!(!store.remove_member(family, first).unwrap());
    }

    #[test]
    fn scene_membership_order_is_authoritative_and_local() {
        let mut store = SemanticStore::new();
        let a = store.insert_authoring_object();
        let b = store.insert_authoring_object();
        let c = store.insert_authoring_object();

        assert_eq!(store.scene_root_count(), 0);
        assert!(store.scene_roots().next().is_none());

        assert!(store.attach_to_scene(a).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert!(store.attach_to_scene(b).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert!(store.attach_to_scene(c).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert_eq!(store.scene_roots().collect::<Vec<_>>(), vec![a, b, c]);

        assert!(store.detach_from_scene(b).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 3);
        assert_eq!(store.scene_roots().collect::<Vec<_>>(), vec![a, c]);

        assert!(store.attach_to_scene(b).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert_eq!(store.scene_roots().collect::<Vec<_>>(), vec![a, c, b]);
        assert_eq!(store.node(b).unwrap().id(), b);

        assert!(!store.attach_to_scene(b).unwrap());
        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );
        let detached = store.insert_authoring_object();
        assert!(!store.detach_from_scene(detached).unwrap());
        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );
    }

    #[test]
    fn detached_scene_lifecycle_preserves_semantic_identity_and_family_links() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let child = store.insert_authoring_object();
        let source = SourceIdentity::ExplicitKey("child".into());
        store
            .set_source_identity(child, Some(source.clone()))
            .unwrap();
        store.add_member(family, child).unwrap();

        assert_eq!(
            store.node(child).unwrap().residency(),
            SemanticNodeResidency::Detached
        );
        assert!(store.attach_to_scene(child).unwrap());
        assert_eq!(
            store.node(child).unwrap().residency(),
            SemanticNodeResidency::SceneOwned
        );

        assert!(store.detach_from_scene(child).unwrap());
        assert_eq!(store.node(child).unwrap().id(), child);
        assert_eq!(store.node(child).unwrap().parents(), &[family]);
        assert_eq!(store.node(family).unwrap().members(), vec![child]);
        assert_eq!(store.node_for_source(&source), Some(child));

        assert!(store.attach_to_scene(child).unwrap());
        assert_eq!(store.node(child).unwrap().id(), child);
        assert_eq!(store.scene_roots().collect::<Vec<_>>(), vec![child]);
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
    fn attach_detach_cost_does_not_scale_with_unrelated_nodes() {
        let mut store = SemanticStore::new();
        let roots = (0..10_000)
            .map(|_| store.insert_authoring_object())
            .collect::<Vec<_>>();
        let target = roots[5_000];

        store.attach_to_scene(roots[0]).unwrap();
        store.attach_to_scene(target).unwrap();
        store.attach_to_scene(roots[9_999]).unwrap();

        assert!(store.detach_from_scene(target).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 3);
        assert!(store.attach_to_scene(target).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 2);
    }

    #[test]
    fn family_member_removal_does_not_scan_unrelated_siblings() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let members = (0..10_000)
            .map(|_| store.insert_authoring_object())
            .collect::<Vec<_>>();
        for member in members.iter().copied() {
            store.add_member(family, member).unwrap();
        }
        let target = members[5_000];

        assert!(store.remove_member(family, target).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert_eq!(store.node(family).unwrap().member_count(), 9_999);
        assert!(!store.node(family).unwrap().members().contains(&target));

        store.add_member(family, target).unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        let ordered = store.node(family).unwrap().members();
        assert_eq!(ordered.last(), Some(&target));
    }

    #[test]
    fn deleting_member_from_large_family_is_local() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let members = (0..10_000)
            .map(|_| store.insert_authoring_object())
            .collect::<Vec<_>>();
        for member in members.iter().copied() {
            store.add_member(family, member).unwrap();
        }
        let target = members[5_000];

        store.remove_node(target).unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert_eq!(store.node(family).unwrap().member_count(), 9_999);
        assert!(store.node(target).is_none());
        assert!(!store.node(family).unwrap().members().contains(&target));
    }

    #[test]
    fn reused_slot_invalidates_stale_generation_for_all_lifecycle_operations() {
        let mut store = SemanticStore::new();
        let first = store.insert_object(object(1));
        store.attach_to_scene(first).unwrap();
        store.remove_node(first).unwrap();
        let second = store.insert_object(object(2));

        assert_eq!(first.slot(), second.slot());
        assert_ne!(first.generation(), second.generation());
        assert!(store.node(first).is_none());
        assert!(store.node(second).is_some());
        assert!(matches!(
            store.attach_to_scene(first),
            Err(SemanticStoreError::UnknownNode(id)) if id == first
        ));
        assert!(matches!(
            store.detach_from_scene(first),
            Err(SemanticStoreError::UnknownNode(id)) if id == first
        ));
        assert!(matches!(
            store.remove_node(first),
            Err(SemanticStoreError::UnknownNode(id)) if id == first
        ));
    }

    #[test]
    fn deleting_attached_root_repairs_order_without_touching_unrelated_roots() {
        let mut store = SemanticStore::new();
        let a = store.insert_authoring_object();
        let b = store.insert_authoring_object();
        let c = store.insert_authoring_object();
        store.attach_to_scene(a).unwrap();
        store.attach_to_scene(b).unwrap();
        store.attach_to_scene(c).unwrap();

        store.remove_node(b).unwrap();
        assert_eq!(store.scene_roots().collect::<Vec<_>>(), vec![a, c]);
        assert_eq!(store.scene_root_count(), 2);
        assert_eq!(store.node(a).unwrap().id(), a);
        assert_eq!(store.node(c).unwrap().id(), c);
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
        assert_eq!(store.node(first_family).unwrap().members(), vec![child]);
        assert_eq!(store.node(second_family).unwrap().members(), vec![child]);

        store.attach_to_scene(child).unwrap();
        store.detach_from_scene(child).unwrap();
        assert_eq!(
            store.node(child).unwrap().parents(),
            &[first_family, second_family]
        );
    }

    #[test]
    fn deleting_node_cleans_direct_family_edges() {
        let mut store = SemanticStore::new();
        let first_family = store.insert_family();
        let second_family = store.insert_family();
        let child = store.insert_authoring_object();
        store.add_member(first_family, child).unwrap();
        store.add_member(second_family, child).unwrap();

        store.remove_node(child).unwrap();
        assert!(store.node(first_family).unwrap().members().is_empty());
        assert!(store.node(second_family).unwrap().members().is_empty());
        assert_eq!(store.last_mutation_stats().slots_written, 3);
    }

    #[test]
    fn deleting_family_cleans_member_parent_edges_in_order() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        store.add_member(family, first).unwrap();
        store.add_member(family, second).unwrap();

        store.remove_node(family).unwrap();
        assert!(store.node(first).unwrap().parents().is_empty());
        assert!(store.node(second).unwrap().parents().is_empty());
        assert_eq!(store.last_mutation_stats().slots_written, 3);
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
    fn source_identity_is_unique_stable_and_released_on_delete() {
        let mut store = SemanticStore::new();
        let first = store.insert_object(object(1));
        let second = store.insert_object(object(2));
        let source = SourceIdentity::ExplicitKey("hero".into());

        store
            .set_source_identity(first, Some(source.clone()))
            .unwrap();
        store.attach_to_scene(first).unwrap();
        store.detach_from_scene(first).unwrap();
        assert_eq!(store.node_for_source(&source), Some(first));
        assert!(matches!(
            store.set_source_identity(second, Some(source.clone())),
            Err(SemanticStoreError::DuplicateSourceIdentity(_))
        ));

        store.remove_node(first).unwrap();
        assert_eq!(store.node_for_source(&source), None);
        store
            .set_source_identity(second, Some(source.clone()))
            .unwrap();
        assert_eq!(store.node_for_source(&source), Some(second));
    }

    #[test]
    fn flat_scene_adapter_preserves_legacy_lookup_and_root_order() {
        // Compatibility-only regression owned for deletion by #959/A4.
        let mut scene = SceneDefinition::new();
        let first = scene.add(GeometryRef::circle(1.0));
        let second = scene.add(GeometryRef::rectangle(2.0, 1.0));
        let store = SemanticStore::from_scene_definition(&scene);
        let first_node = store.node_for_object(first).unwrap();
        let second_node = store.node_for_object(second).unwrap();

        assert!(store.node(first_node).unwrap().is_scene_owned());
        assert!(store.node(second_node).unwrap().is_scene_owned());
        assert_eq!(
            store.scene_roots().collect::<Vec<_>>(),
            vec![first_node, second_node]
        );
    }
}
