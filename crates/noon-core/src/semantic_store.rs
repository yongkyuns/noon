use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// Opaque in-process provenance for one semantic store. This is neither a node
/// identity nor a publication revision, and is never an interchange value.
#[derive(Clone, Debug, Default)]
pub struct SemanticStoreIdentity(std::sync::Arc<()>);

impl PartialEq for SemanticStoreIdentity {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for SemanticStoreIdentity {}

impl std::hash::Hash for SemanticStoreIdentity {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&std::sync::Arc::as_ptr(&self.0), state);
    }
}

use crate::{ObjectDefinition, ObjectId, SceneDefinition};
use crate::{
    SemanticAnimationState, SemanticObjectState, SemanticSignalState, SemanticUpdaterRegistration,
};

mod semantic_references;
mod semantic_text_resources;
use semantic_references::SemanticIncomingReference;
pub(crate) use semantic_references::{SemanticRemoveNodeEffect, SemanticRemoveNodeOutcome};

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

/// Ordered family membership with local lookup/add/remove/reorder.
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

    /// Move an existing member immediately before `before`, or to the tail when
    /// `before` is `None`. All link lookup and rewiring is O(1).
    fn move_before(&mut self, member: SemanticNodeId, before: Option<SemanticNodeId>) -> bool {
        let link = *self
            .links
            .get(&member)
            .expect("family reorder member must have a membership link");
        if before == Some(member)
            || before.is_some_and(|before| link.next == Some(before))
            || (before.is_none() && self.tail == Some(member))
        {
            return false;
        }

        // Detach the member without changing membership identity.
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

        let (previous, next) = if let Some(before_id) = before {
            let before_link = *self
                .links
                .get(&before_id)
                .expect("family reorder anchor must have a membership link");
            (before_link.previous, Some(before_id))
        } else {
            (self.tail, None)
        };

        if let Some(previous_id) = previous {
            self.links
                .get_mut(&previous_id)
                .expect("family reorder previous link must exist")
                .next = Some(member);
        } else {
            self.head = Some(member);
        }
        if let Some(next_id) = next {
            self.links
                .get_mut(&next_id)
                .expect("family reorder next link must exist")
                .previous = Some(member);
        } else {
            self.tail = Some(member);
        }
        *self
            .links
            .get_mut(&member)
            .expect("family reorder member link must remain present") =
            FamilyMemberLink { previous, next };
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
    /// Authored native-reactive signal using the same scene-global generational
    /// identity allocator as objects and families.
    Signal(SemanticSignalState),
    /// Authored animation declaration using the same scene-global generational
    /// identity allocator as every other semantic entity.
    Animation(SemanticAnimationState),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    id: SemanticNodeId,
    kind: SemanticNodeKind,
    /// Authoritative authored object payload for target semantic objects.
    ///
    /// `None` is valid for families, signals, animations, legacy compatibility
    /// objects, and the temporary state-less frontend identity seam only.
    object_state: Option<SemanticObjectState>,
    source_identity: Option<SourceIdentity>,
    scene_membership: SemanticSceneMembership,
    /// Families containing this node. Multiple parents are intentional and
    /// preserve Manim-style aliasing/reference semantics. This list contains only
    /// direct relationships, so scans are proportional to this node's own degree.
    parents: Vec<SemanticNodeId>,
    /// Ordered direct family membership with O(1) identity removal and append.
    members: OrderedFamilyMembers,
    /// Ordered authored host-updater registrations for this semantic node.
    ///
    /// `HostCallbackId` identifies host-owned callable code; the registration itself
    /// belongs to the Semantic Scene and therefore follows this node across
    /// detach/re-attach. Lowering decides how these declarations become runtime
    /// callback slots.
    host_updaters: Vec<SemanticUpdaterRegistration>,
    /// Signals explicitly authored in this family-root scope.
    ///
    /// This is separate from painter membership: scoped signals participate in
    /// reactive lowering without becoming renderable family children. Scope has
    /// no painter ordering, so one ordered identity set provides deterministic
    /// traversal and local membership insertion/removal without a mirror index.
    scoped_signals: BTreeSet<SemanticNodeId>,
}

impl SemanticNode {
    pub const fn id(&self) -> SemanticNodeId {
        self.id
    }

    pub fn kind(&self) -> &SemanticNodeKind {
        &self.kind
    }

    pub(crate) fn semantic_signal_state_mut(&mut self) -> Option<&mut SemanticSignalState> {
        match &mut self.kind {
            SemanticNodeKind::Signal(state) => Some(state),
            _ => None,
        }
    }

    pub fn object(&self) -> Option<&ObjectDefinition> {
        match &self.kind {
            SemanticNodeKind::Object(object) => Some(object),
            SemanticNodeKind::AuthoringObject
            | SemanticNodeKind::Family
            | SemanticNodeKind::Signal(_)
            | SemanticNodeKind::Animation(_) => None,
        }
    }

    pub fn object_mut(&mut self) -> Option<&mut ObjectDefinition> {
        match &mut self.kind {
            SemanticNodeKind::Object(object) => Some(object),
            SemanticNodeKind::AuthoringObject
            | SemanticNodeKind::Family
            | SemanticNodeKind::Signal(_)
            | SemanticNodeKind::Animation(_) => None,
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

    pub fn host_updaters(&self) -> &[SemanticUpdaterRegistration] {
        &self.host_updaters
    }

    pub(crate) fn host_updaters_mut(&mut self) -> &mut Vec<SemanticUpdaterRegistration> {
        &mut self.host_updaters
    }

    pub fn scoped_signals(&self) -> &BTreeSet<SemanticNodeId> {
        &self.scoped_signals
    }

    pub(crate) fn scoped_signals_mut(&mut self) -> &mut BTreeSet<SemanticNodeId> {
        &mut self.scoped_signals
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

#[derive(Debug, Default)]
pub struct SemanticStore {
    identity: SemanticStoreIdentity,
    geometry_resources: crate::GeometryResourceArena,
    text_resources: crate::TextResourceArena,
    font_resources: crate::FontResourceArena,
    slots: Vec<SemanticSlot>,
    free_head: Option<u32>,
    live_nodes: usize,
    scene_head: Option<SemanticNodeId>,
    scene_tail: Option<SemanticNodeId>,
    scene_nodes: usize,
    next_insertion_order: u64,
    object_nodes: HashMap<ObjectId, SemanticNodeId>,
    source_nodes: HashMap<SourceIdentity, SemanticNodeId>,
    incoming_references: HashMap<SemanticNodeId, Vec<SemanticIncomingReference>>,
    last_mutation: SemanticMutationStats,
    scene_revision: crate::SceneRevision,
}

// A cloned scene is an independent authoring store. Re-namespace its resource
// references while sharing immutable payload allocations through the arena's Arc values.
impl Clone for SemanticStore {
    fn clone(&self) -> Self {
        let mut geometry_resources = self.geometry_resources.clone();
        let namespace = geometry_resources.fork_namespace();
        let mut text_resources = self.text_resources.clone();
        let text_namespace = text_resources.fork_namespace();
        text_resources.remap_geometry_handles(|handle| {
            if self.geometry_resources.get(*handle).is_some() {
                handle.arena = namespace;
            }
        });
        let mut font_resources = self.font_resources.clone();
        font_resources.fork_namespace();
        let mut slots = self.slots.clone();
        for slot in &mut slots {
            if let Some(node) = slot.node.as_mut() {
                if let Some(state) = node.object_state.as_mut() {
                    match &mut state.content {
                        crate::SemanticObjectContent::Geometry(
                            crate::StoredGeometry::Resource(handle),
                        ) if self.geometry_resources.get(*handle).is_some() => {
                            handle.arena = namespace;
                        }
                        crate::SemanticObjectContent::Text(handle)
                            if self.text_resources.get(*handle).is_some() =>
                        {
                            handle.arena = text_namespace;
                        }
                        _ => {}
                    }
                }
            }
        }
        Self {
            identity: SemanticStoreIdentity::default(),
            geometry_resources,
            text_resources,
            font_resources,
            slots,
            free_head: self.free_head,
            live_nodes: self.live_nodes,
            scene_head: self.scene_head,
            scene_tail: self.scene_tail,
            scene_nodes: self.scene_nodes,
            next_insertion_order: self.next_insertion_order,
            object_nodes: self.object_nodes.clone(),
            source_nodes: self.source_nodes.clone(),
            incoming_references: self.incoming_references.clone(),
            last_mutation: self.last_mutation,
            scene_revision: self.scene_revision,
        }
    }
}

impl SemanticStore {
    pub(crate) const fn next_insertion_order(&self) -> u64 {
        self.next_insertion_order
    }

    #[cfg(test)]
    pub(crate) fn set_next_insertion_order_for_test(&mut self, next: u64) {
        self.next_insertion_order = next;
    }

    /// A cloneable provenance token; cloning a store itself creates a new owner.
    pub fn identity(&self) -> SemanticStoreIdentity {
        self.identity.clone()
    }

    pub fn geometry_resources(&self) -> &crate::GeometryResourceArena {
        &self.geometry_resources
    }

    /// Intern immutable path content in this store's resource namespace.
    pub fn insert_geometry_path(
        &mut self,
        path: crate::VectorPath,
    ) -> Result<crate::GeometryResourceHandle, String> {
        if !path.is_finite() {
            return Err("geometry path contains non-finite points".into());
        }
        Ok(self.geometry_resources.insert_path(path))
    }

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
        self.register_semantic_references_for_owner(id);
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

    pub(crate) fn insert_semantic_signal_state(
        &mut self,
        state: SemanticSignalState,
    ) -> SemanticNodeId {
        let id = self.insert_kind(SemanticNodeKind::Signal(state));
        self.register_semantic_references_for_owner(id);
        id
    }

    pub(crate) fn insert_semantic_animation_state(
        &mut self,
        state: SemanticAnimationState,
    ) -> SemanticNodeId {
        let id = self.insert_kind(SemanticNodeKind::Animation(state));
        self.register_semantic_references_for_owner(id);
        id
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
            host_updaters: Vec::new(),
            scoped_signals: BTreeSet::new(),
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

    /// Signals explicitly included in one family-root execution scope, ordered
    /// by stable semantic identity rather than painter position.
    pub fn semantic_scoped_signals(
        &self,
        scope: SemanticNodeId,
    ) -> Result<&BTreeSet<SemanticNodeId>, SemanticStoreError> {
        let node = self
            .node(scope)
            .ok_or(SemanticStoreError::UnknownNode(scope))?;
        if !matches!(node.kind(), SemanticNodeKind::Family) {
            return Err(SemanticStoreError::NotFamily(scope));
        }
        Ok(node.scoped_signals())
    }

    pub(crate) fn scope_semantic_signal(
        &mut self,
        scope: SemanticNodeId,
        signal: SemanticNodeId,
    ) -> Result<bool, SemanticStoreError> {
        if !matches!(
            self.node(scope).map(SemanticNode::kind),
            Some(SemanticNodeKind::Family)
        ) {
            return Err(match self.node(scope) {
                None => SemanticStoreError::UnknownNode(scope),
                Some(_) => SemanticStoreError::NotFamily(scope),
            });
        }
        if !matches!(
            self.node(signal).map(SemanticNode::kind),
            Some(SemanticNodeKind::Signal(_))
        ) {
            return Err(SemanticStoreError::UnknownNode(signal));
        }
        if self.is_semantic_signal_scoped(scope, signal) {
            return Ok(false);
        }
        let inserted = self
            .node_mut(scope)
            .expect("validated scope remains live")
            .scoped_signals_mut()
            .insert(signal);
        debug_assert!(inserted);
        self.register_semantic_scoped_signal_reference(scope, signal);
        Ok(true)
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

    /// Replace one attached scene root with detached semantic nodes in place.
    ///
    /// This is a crate-private storage primitive for shared scene operations. It
    /// preserves the old root's exact position and writes only that root, the
    /// replacement nodes, and its immediate outside neighbors. Family links,
    /// source identity, node identity, and authored object state are unchanged.
    pub(crate) fn replace_scene_root_with_detached(
        &mut self,
        root: SemanticNodeId,
        replacements: &[SemanticNodeId],
    ) -> usize {
        let membership = self
            .node(root)
            .expect("scene-root splice requires a live root")
            .scene_membership;
        let SemanticSceneMembership::Attached { previous, next } = membership else {
            panic!("scene-root splice requires an attached root");
        };

        let mut seen = HashSet::with_capacity(replacements.len());
        for replacement in replacements.iter().copied() {
            assert_ne!(
                replacement, root,
                "scene-root splice cannot reinsert its root"
            );
            let node = self
                .node(replacement)
                .expect("scene-root splice replacement must be live");
            assert!(
                matches!(node.scene_membership, SemanticSceneMembership::Detached),
                "scene-root splice replacement must be detached"
            );
            assert!(
                seen.insert(replacement),
                "scene-root splice replacements must be unique"
            );
        }

        let first = replacements.first().copied();
        let last = replacements.last().copied();

        if let Some(previous_id) = previous {
            let previous_node = self
                .node_mut(previous_id)
                .expect("scene previous link must reference a live semantic node");
            match &mut previous_node.scene_membership {
                SemanticSceneMembership::Attached {
                    next: previous_next,
                    ..
                } => {
                    *previous_next = first.or(next);
                }
                SemanticSceneMembership::Detached => {
                    unreachable!("attached root cannot have a detached previous root")
                }
            }
        } else {
            debug_assert_eq!(self.scene_head, Some(root));
            self.scene_head = first.or(next);
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
                    *next_previous = last.or(previous);
                }
                SemanticSceneMembership::Detached => {
                    unreachable!("attached root cannot have a detached next root")
                }
            }
        } else {
            debug_assert_eq!(self.scene_tail, Some(root));
            self.scene_tail = last.or(previous);
        }

        for (index, replacement) in replacements.iter().copied().enumerate() {
            let replacement_previous = if index == 0 {
                previous
            } else {
                Some(replacements[index - 1])
            };
            let replacement_next = if index + 1 == replacements.len() {
                next
            } else {
                Some(replacements[index + 1])
            };
            self.node_mut(replacement)
                .expect("scene-root splice replacement validated above")
                .scene_membership = SemanticSceneMembership::Attached {
                previous: replacement_previous,
                next: replacement_next,
            };
        }

        self.node_mut(root)
            .expect("scene-root splice root validated above")
            .scene_membership = SemanticSceneMembership::Detached;
        self.scene_nodes = self.scene_nodes - 1 + replacements.len();
        if self.scene_nodes == 0 {
            debug_assert!(self.scene_head.is_none());
            debug_assert!(self.scene_tail.is_none());
        }

        let slots_written =
            1 + replacements.len() + (previous.is_some() as usize) + (next.is_some() as usize);
        self.last_mutation = SemanticMutationStats {
            slots_written,
            cycle_nodes_visited: 0,
        };
        slots_written
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

    /// Check one direct family edge without materializing or scanning its ordered members.
    pub fn is_direct_member(
        &self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<bool, SemanticStoreError> {
        let family_id = family;
        let family = self
            .node(family_id)
            .ok_or(SemanticStoreError::UnknownNode(family_id))?;
        if !matches!(family.kind(), SemanticNodeKind::Family) {
            return Err(SemanticStoreError::NotFamily(family_id));
        }
        if self.node(member).is_none() {
            return Err(SemanticStoreError::UnknownNode(member));
        }
        Ok(family.members.contains(member))
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

    /// Reorder one direct family member without changing membership or parent edges.
    ///
    /// `Some(anchor)` moves `member` immediately before the anchor. `None` moves
    /// the member to the tail. Identity validation and link rewiring are O(1) and
    /// only the family's authoritative order is mutated.
    pub fn reorder_member(
        &mut self,
        family: SemanticNodeId,
        member: SemanticNodeId,
        before: Option<SemanticNodeId>,
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
        let members = &self.node(family).expect("family validated above").members;
        if !members.contains(member) {
            return Err(SemanticStoreError::NotFamilyMember { family, member });
        }
        if let Some(anchor) = before {
            if self.node(anchor).is_none() {
                return Err(SemanticStoreError::UnknownNode(anchor));
            }
            if !members.contains(anchor) {
                return Err(SemanticStoreError::NotFamilyMember {
                    family,
                    member: anchor,
                });
            }
        }

        let changed = self
            .node_mut(family)
            .expect("family validated above")
            .members
            .move_before(member, before);
        self.last_mutation = if changed {
            SemanticMutationStats {
                slots_written: 1,
                cycle_nodes_visited: 0,
            }
        } else {
            SemanticMutationStats::default()
        };
        Ok(changed)
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
        self.unregister_semantic_references_for_owner(id);

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
        self.incoming_references.remove(&id);

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

    /// Revision of the last coherently published authoritative semantic transaction.
    ///
    /// Direct storage-building helpers intentionally do not advance this clock;
    /// `SemanticMutationTransaction::apply` is the publication boundary and calls
    /// publishes once after complete preflight/commit. Mutation work counters are
    /// independent of publication and may be updated by nested storage helpers.
    pub const fn scene_revision(&self) -> crate::SceneRevision {
        self.scene_revision
    }

    pub const fn last_mutation_stats(&self) -> SemanticMutationStats {
        self.last_mutation
    }

    pub(crate) fn set_last_mutation_writes(&mut self, slots_written: usize) {
        self.last_mutation = SemanticMutationStats {
            slots_written,
            cycle_nodes_visited: 0,
        };
    }

    pub(crate) fn publish_scene_revision(&mut self, revision: crate::SceneRevision) {
        self.scene_revision = revision;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticStoreError {
    UnknownNode(SemanticNodeId),
    NotFamily(SemanticNodeId),
    NotFamilyMember {
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
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
            Self::NotFamilyMember { family, member } => write!(
                formatter,
                "semantic node {}:{} is not a direct member of family {}:{}",
                member.slot(),
                member.generation(),
                family.slot(),
                family.generation()
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

    #[test]
    fn resources_are_store_scoped_and_clones_share_only_immutable_payloads() {
        use crate::{
            FontFaceIdentity, GeometryResource, Rect, SemanticObjectContent, StoredGeometry,
            TextResource, TextSourceKind, Vec2, VectorPath,
        };
        use std::sync::Arc;
        let mut first = SemanticStore::new();
        let mut second = SemanticStore::new();
        let a = first.insert_geometry_path(VectorPath::new()).unwrap();
        let b = second.insert_geometry_path(VectorPath::new()).unwrap();
        assert_eq!((a.id, a.version), (b.id, b.version));
        assert_ne!(a.arena, b.arena);
        assert!(first.geometry_resources().get(b).is_none());
        assert!(second.geometry_resources().get(a).is_none());
        let node =
            first.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(a)));
        // The low-level store fixture can contain invalid state; cloning must not legitimize it.
        let foreign =
            first.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(b)));
        let text = first
            .text_resources
            .insert(TextResource {
                source: Arc::from(""),
                kind: TextSourceKind::Plain,
                runs: Arc::from([]),
                vector_items: Arc::from([]),
                render_items: Arc::from([]),
                parts: Arc::from([]),
                bounds: Rect::new(Vec2::ZERO, Vec2::ZERO),
                baseline: 0.0,
                layout_artifact: None,
            })
            .unwrap();
        let text_node = first.insert_semantic_object(SemanticObjectState::new(text));
        let face = FontFaceIdentity {
            family: Arc::from("Test Sans"),
            face_key: Arc::from("test-sans"),
            face_index: 0,
            variation_key: Arc::from(""),
        };
        let font = first
            .font_resources
            .intern_face(&face, Arc::<[u8]>::from([1, 2, 3]))
            .unwrap();
        let cloned = first.clone();
        let SemanticObjectContent::Geometry(StoredGeometry::Resource(c)) =
            cloned.semantic_object_state_checked(node).unwrap().content
        else {
            panic!("resource content")
        };
        assert_ne!(a.arena, c.arena);
        assert!(cloned.geometry_resources().get(a).is_none());
        let GeometryResource::VectorPath(original) = first.geometry_resources().get(a).unwrap();
        let GeometryResource::VectorPath(copied) = cloned.geometry_resources().get(c).unwrap();
        assert!(Arc::ptr_eq(original, copied));
        assert_eq!(
            cloned
                .semantic_object_state_checked(foreign)
                .unwrap()
                .content,
            SemanticObjectContent::Geometry(StoredGeometry::Resource(b))
        );
        assert!(cloned.geometry_resources().get(b).is_none());
        let SemanticObjectContent::Text(cloned_text) = cloned
            .semantic_object_state_checked(text_node)
            .unwrap()
            .content
        else {
            panic!("text resource content")
        };
        assert_ne!(text.arena, cloned_text.arena);
        assert!(first.text_resources().get(cloned_text).is_none());
        assert!(cloned.text_resources().get(text).is_none());
        assert!(Arc::ptr_eq(
            &first.text_resources().get_shared(text).unwrap(),
            &cloned.text_resources().get_shared(cloned_text).unwrap(),
        ));
        let cloned_font = cloned.font_resources().handle_for_face(&face).unwrap();
        assert_ne!(font.arena, cloned_font.arena);
        assert!(first.font_resources().get(cloned_font).is_none());
        assert!(cloned.font_resources().get(font).is_none());
        assert!(Arc::ptr_eq(
            &first.font_resources().get_shared(font).unwrap(),
            &cloned.font_resources().get_shared(cloned_font).unwrap(),
        ));
    }

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
    fn family_member_reorder_is_one_slot_local() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let members = (0..10_000)
            .map(|_| store.insert_authoring_object())
            .collect::<Vec<_>>();
        for member in members.iter().copied() {
            store.add_member(family, member).unwrap();
        }
        let target = members[5_000];
        let anchor = members[10];
        let parents_before = store.node(target).unwrap().parents().to_vec();

        assert!(store.reorder_member(family, target, Some(anchor)).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(store.node(target).unwrap().parents(), parents_before);
        let ordered = store.node(family).unwrap().members();
        let anchor_index = ordered.iter().position(|id| *id == anchor).unwrap();
        assert_eq!(ordered[anchor_index - 1], target);

        assert!(!store.reorder_member(family, target, Some(anchor)).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 0);
        assert!(store.reorder_member(family, target, None).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(store.node(family).unwrap().members().last(), Some(&target));
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
