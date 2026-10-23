//! Typed handle for authoritative semantic family membership.

mod path_admission;
pub(crate) use path_admission::publish_path_family_with;

use crate::AuthoringError;
use noon_core::{
    Bounds2D64, SemanticMutationTransaction, SemanticNodeId, SemanticNodeKind,
    SemanticObjectProperty, SemanticSheenDirection, SemanticStore, SemanticVec3,
};
use std::{
    cell::RefCell,
    collections::{btree_map::Entry, BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};

/// Ordered family translation over authoritative shared semantic leaf identity.
///
/// Frontends may retain wrapper trees for language-level identity, but the shared
/// semantic traversal decides which unique leaves are mutated and in what order. The delta is
/// validated once in Rust; all affected leaves are then committed atomically.
#[derive(Clone, Debug)]
#[doc(hidden)]
pub(crate) struct FamilyTranslation {
    source_members: Vec<SemanticNodeId>,
    delta: SemanticVec3,
}

impl FamilyTranslation {
    pub fn begin_3d(
        store: &SemanticStore,
        source: SemanticNodeId,
        delta: SemanticVec3,
    ) -> Result<Self, AuthoringError> {
        let source_members = store
            .ordered_leaf_nodes(source)
            .map_err(AuthoringError::from)?;
        Self::from_members_3d(source_members, delta)
    }

    pub fn from_members(
        source_members: Vec<SemanticNodeId>,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<Self, AuthoringError> {
        Self::from_members_3d(source_members, SemanticVec3::new(delta_x, delta_y, 0.0))
    }

    pub fn from_members_3d(
        source_members: Vec<SemanticNodeId>,
        delta: SemanticVec3,
    ) -> Result<Self, AuthoringError> {
        delta.lower_xy_f32().map_err(AuthoringError::from)?;
        Ok(Self {
            source_members,
            delta,
        })
    }

    /// Apply each selected semantic leaf in one transaction.
    pub fn apply(self, store: &mut SemanticStore) -> Result<(), AuthoringError> {
        self.transaction(store)?
            .apply(store)
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn transaction(
        self,
        store: &SemanticStore,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        translation_transaction(self.into_shifts(), |leaf| {
            store
                .semantic_object_state_checked(leaf)
                .map(|state| state.transform.translation)
                .map_err(AuthoringError::from)
        })
    }

    /// Stage this translation into an aggregate semantic transaction.
    ///
    /// Composite authoring uses this instead of publishing an intermediate
    /// placement transaction. Repeated leaves are accumulated before one
    /// property write per semantic identity.
    pub(crate) fn stage(
        self,
        transaction: &mut SemanticMutationTransaction,
        store: &SemanticStore,
    ) -> Result<(), AuthoringError> {
        let mut translations = BTreeMap::new();
        for (leaf, delta) in self.into_shifts() {
            if let std::collections::btree_map::Entry::Vacant(entry) = translations.entry(leaf) {
                let translation = store
                    .semantic_object_state_checked(leaf)
                    .map_err(AuthoringError::from)?
                    .transform
                    .translation;
                entry.insert(translation);
            }
            let translation = translations.get_mut(&leaf).expect("inserted above");
            translation.x += delta.x;
            translation.y += delta.y;
            translation.z += delta.z;
        }
        for (leaf, translation) in translations {
            translation.lower_xy_f32().map_err(AuthoringError::from)?;
            transaction.set_property(leaf, SemanticObjectProperty::Translation, translation);
        }
        Ok(())
    }

    pub fn into_shifts(self) -> Vec<(SemanticNodeId, SemanticVec3)> {
        self.source_members
            .into_iter()
            .map(|member| (member, self.delta))
            .collect()
    }
}

pub(crate) fn translation_transaction<F, E>(
    shifts: impl IntoIterator<Item = (SemanticNodeId, SemanticVec3)>,
    mut authored_translation: F,
) -> Result<SemanticMutationTransaction, E>
where
    F: FnMut(SemanticNodeId) -> Result<SemanticVec3, E>,
    E: From<AuthoringError>,
{
    // Accumulate staged operations before publishing one final property per
    // identity. A single family translation supplies each semantic leaf once.
    let mut translations = BTreeMap::new();
    for (leaf, delta) in shifts {
        let translation = match translations.entry(leaf) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(authored_translation(leaf)?),
        };
        translation.x += delta.x;
        translation.y += delta.y;
        translation.z += delta.z;
    }
    let mut transaction = SemanticMutationTransaction::new();
    for (leaf, translation) in translations {
        // Validate final accumulated positions before any property is published,
        // including aliases whose individual deltas would each fit in f32.
        translation.lower_xy_f32().map_err(AuthoringError::from)?;
        transaction.set_property(leaf, SemanticObjectProperty::Translation, translation);
    }
    Ok(transaction)
}

/// A semantic family identity in one shared scene store.
///
/// The handle retains no copied membership, schedule, or runtime state. Ordered
/// traversal always reads the authoritative semantic family at use time.
#[derive(Clone, Debug)]
pub struct MobjectFamily {
    store: Rc<RefCell<SemanticStore>>,
    node: SemanticNodeId,
}

/// One borrowed object-or-family target over canonical semantic identity.
#[derive(Clone, Copy)]
pub enum MobjectTarget<'a> {
    Object(&'a crate::Mobject),
    Family(&'a MobjectFamily),
}

impl<'a> From<&'a crate::Mobject> for MobjectTarget<'a> {
    fn from(value: &'a crate::Mobject) -> Self {
        Self::Object(value)
    }
}

impl<'a> From<&'a MobjectFamily> for MobjectTarget<'a> {
    fn from(value: &'a MobjectFamily) -> Self {
        Self::Family(value)
    }
}

impl MobjectTarget<'_> {
    /// Observe this root and its descendants in Manim's last-occurrence order.
    /// Each identity appears once; this query neither publishes nor changes paint.
    pub fn family_nodes(&self) -> Result<Vec<SemanticNodeId>, AuthoringError> {
        self.validate()?;
        self.integration_store()
            .borrow()
            .ordered_last_occurrence_authoring_nodes(self.node_id())
            .map_err(AuthoringError::from)
    }

    /// Read Manim's per-node `updating_suspended` state from the shared scene.
    pub fn updating_suspended(&self) -> Result<bool, AuthoringError> {
        self.validate()?;
        self.integration_store()
            .borrow()
            .node(self.node_id())
            .map(noon_core::SemanticNode::updating_suspended)
            .ok_or_else(|| {
                AuthoringError::from(noon_core::SemanticSceneOperationError::UnknownNode(
                    self.node_id(),
                ))
            })
    }

    /// Atomically set this node's updater suspension flag and, by default, the
    /// unique shared-family descendants. Live callback scheduling remains a
    /// separate ordered phase, so scene-owned subtrees reject before mutation.
    pub fn set_updating_suspended(
        &self,
        suspended: bool,
        recursive: bool,
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let store = self.integration_store();
        let mut store = store.borrow_mut();
        let nodes = if recursive {
            store
                .ordered_last_occurrence_authoring_nodes(self.node_id())
                .map_err(AuthoringError::from)?
        } else {
            vec![self.node_id()]
        };
        if nodes
            .iter()
            .copied()
            .any(|node| node_or_ancestor_is_scene_owned(&store, node))
        {
            return Err(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::UpdaterSuspensionLive,
            ));
        }
        let mut transaction = SemanticMutationTransaction::new();
        for node in nodes {
            transaction.set_updating_suspended(node, suspended);
        }
        transaction
            .apply(&mut store)
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub fn sheen_direction(&self) -> Result<SemanticSheenDirection, AuthoringError> {
        self.validate()?;
        self.integration_store()
            .borrow()
            .node(self.node_id())
            .and_then(|node| node.sheen_direction())
            .ok_or(AuthoringError::NonFiniteDirection)
    }

    pub fn set_sheen_direction(
        &self,
        direction: SemanticSheenDirection,
        family: bool,
    ) -> Result<(), AuthoringError> {
        self.sheen_direction_transaction(direction, family)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn sheen_direction_transaction(
        &self,
        direction: SemanticSheenDirection,
        family: bool,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        if !direction.is_valid() {
            return Err(AuthoringError::NonFiniteDirection);
        }
        let store = self.integration_store().borrow();
        let nodes = if family {
            store
                .ordered_last_occurrence_authoring_nodes(self.node_id())
                .map_err(AuthoringError::from)?
        } else {
            vec![self.node_id()]
        };
        let mut transaction = SemanticMutationTransaction::new();
        for node in nodes {
            transaction.set_sheen_direction(node, direction);
        }
        Ok(transaction)
    }

    pub fn rotate_sheen_direction(
        &self,
        angle: f64,
        axis: SemanticVec3,
        family: bool,
    ) -> Result<(), AuthoringError> {
        self.rotate_sheen_direction_transaction(angle, axis, family)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn rotate_sheen_direction_transaction(
        &self,
        angle: f64,
        axis: SemanticVec3,
        family: bool,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        if !angle.is_finite() {
            return Err(AuthoringError::NonFiniteDirection);
        }
        let rotation = noon_core::SemanticRotation3D::from_axis_angle(axis, angle)
            .ok_or(AuthoringError::NonFiniteDirection)?;
        let store = self.integration_store().borrow();
        let nodes = if family {
            store
                .ordered_last_occurrence_authoring_nodes(self.node_id())
                .map_err(AuthoringError::from)?
        } else {
            vec![self.node_id()]
        };
        let mut transaction = SemanticMutationTransaction::new();
        for node in nodes {
            let direction = store
                .node(node)
                .and_then(|node| node.sheen_direction())
                .ok_or(AuthoringError::NonFiniteDirection)?;
            if !direction.is_valid() {
                return Err(AuthoringError::NonFiniteDirection);
            }
            let rotated = rotation
                .rotate_vector(direction.value)
                .ok_or(AuthoringError::NonFiniteDirection)?;
            transaction.set_sheen_direction(node, SemanticSheenDirection::new(rotated, 3));
        }
        Ok(transaction)
    }

    /// Validate this target and return its root identity in the expected store.
    pub fn require_store(
        &self,
        store: &Rc<RefCell<SemanticStore>>,
    ) -> Result<SemanticNodeId, AuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), store) {
            return Err(AuthoringError::ForeignStore);
        }
        self.validate()?;
        Ok(self.node_id())
    }

    pub(crate) fn integration_store(&self) -> &Rc<RefCell<SemanticStore>> {
        match self {
            Self::Object(member) => member.integration_store(),
            Self::Family(member) => member.integration_store(),
        }
    }

    pub(crate) fn node_id(&self) -> SemanticNodeId {
        match self {
            Self::Object(member) => member.node_id(),
            Self::Family(member) => member.node_id(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), AuthoringError> {
        match self {
            Self::Object(member) => member.validate(),
            Self::Family(member) => member.validate(),
        }
    }
}

fn node_or_ancestor_is_scene_owned(store: &SemanticStore, node: SemanticNodeId) -> bool {
    let mut pending = vec![node];
    let mut seen = BTreeSet::new();
    while let Some(node) = pending.pop() {
        if !seen.insert(node) {
            continue;
        }
        let Some(value) = store.node(node) else {
            return true;
        };
        if value.is_scene_owned() {
            return true;
        }
        pending.extend(value.parents().iter().copied());
    }
    false
}

impl crate::Mobject {
    pub fn updating_suspended(&self) -> Result<bool, AuthoringError> {
        MobjectTarget::from(self).updating_suspended()
    }

    pub fn set_updating_suspended(
        &self,
        suspended: bool,
        recursive: bool,
    ) -> Result<(), AuthoringError> {
        MobjectTarget::from(self).set_updating_suspended(suspended, recursive)
    }

    pub fn sheen_direction(&self) -> Result<SemanticSheenDirection, AuthoringError> {
        MobjectTarget::from(self).sheen_direction()
    }

    pub fn set_sheen_direction(
        &self,
        direction: SemanticSheenDirection,
        family: bool,
    ) -> Result<(), AuthoringError> {
        MobjectTarget::from(self).set_sheen_direction(direction, family)
    }

    pub fn rotate_sheen_direction(
        &self,
        angle: f64,
        axis: SemanticVec3,
        family: bool,
    ) -> Result<(), AuthoringError> {
        MobjectTarget::from(self).rotate_sheen_direction(angle, axis, family)
    }
}

impl MobjectFamily {
    pub fn updating_suspended(&self) -> Result<bool, AuthoringError> {
        MobjectTarget::from(self).updating_suspended()
    }

    pub fn set_updating_suspended(
        &self,
        suspended: bool,
        recursive: bool,
    ) -> Result<(), AuthoringError> {
        MobjectTarget::from(self).set_updating_suspended(suspended, recursive)
    }

    pub fn own_sheen_direction(&self) -> Result<SemanticSheenDirection, AuthoringError> {
        MobjectTarget::from(self).sheen_direction()
    }

    pub fn set_sheen_direction(
        &self,
        direction: SemanticSheenDirection,
        family: bool,
    ) -> Result<(), AuthoringError> {
        MobjectTarget::from(self).set_sheen_direction(direction, family)
    }

    pub fn rotate_sheen_direction(
        &self,
        angle: f64,
        axis: SemanticVec3,
        family: bool,
    ) -> Result<(), AuthoringError> {
        MobjectTarget::from(self).rotate_sheen_direction(angle, axis, family)
    }
}

/// Select the last occurrence of each incoming identity, preserving batch order.
/// This only visits the input batch, never the existing family's siblings.
fn last_occurrences(members: &[MobjectTarget<'_>]) -> Vec<usize> {
    let mut seen = BTreeSet::new();
    let mut indices: Vec<_> = members
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(index, member)| seen.insert(member.node_id()).then_some(index))
        .collect();
    indices.reverse();
    indices
}

/// Construct the same pending semantic family for authored and live publication.
pub(crate) fn family_creation_transaction(
    store: &Rc<RefCell<SemanticStore>>,
    members: &[MobjectTarget<'_>],
    z_index: f64,
) -> Result<
    (
        SemanticMutationTransaction,
        noon_core::SemanticLocalNodeToken,
    ),
    AuthoringError,
> {
    family_creation_presentation_transaction(store, members, z_index, crate::Color::WHITE, true)
}

pub(crate) fn family_creation_presentation_transaction(
    store: &Rc<RefCell<SemanticStore>>,
    members: &[MobjectTarget<'_>],
    z_index: f64,
    color: crate::Color,
    draw_enabled: bool,
) -> Result<
    (
        SemanticMutationTransaction,
        noon_core::SemanticLocalNodeToken,
    ),
    AuthoringError,
> {
    for member in members {
        member.require_store(store)?;
    }
    let mut transaction = SemanticMutationTransaction::new();
    let family = transaction.create_node(noon_core::SemanticNodeCreation::family());
    if !draw_enabled {
        transaction.set_draw_enabled(family, false);
    }
    if color != crate::Color::WHITE {
        transaction.set_family_color(family, color);
    }
    if z_index != 0.0 {
        transaction.set_z_index(family, z_index);
    }
    for index in last_occurrences(members) {
        transaction.add_member(family, members[index].node_id());
    }
    Ok((transaction, family))
}

/// Prepare a local direct-member batch, returning one decision per input target.
pub(crate) fn family_membership_transaction(
    family: MobjectTarget<'_>,
    members: &[MobjectTarget<'_>],
    adding: bool,
) -> Result<(SemanticMutationTransaction, Vec<bool>), AuthoringError> {
    family.validate()?;
    for member in members {
        member.require_store(family.integration_store())?;
    }
    let store = family.integration_store().borrow();
    let node = store
        .semantic_authoring_node_checked(family.node_id())
        .map_err(AuthoringError::from)?;
    let mut transaction = SemanticMutationTransaction::new();
    if adding {
        transaction.replace_member_container(family.node_id());
    }
    let mut changed = vec![false; members.len()];
    for index in last_occurrences(members) {
        let id = members[index].node_id();
        let present = node.contains_member(id);
        changed[index] = present != adding;
        if adding {
            if present {
                transaction.reorder_member(family.node_id(), id, None);
            } else {
                transaction.add_member(family.node_id(), id);
            }
        } else if present {
            transaction.remove_member(family.node_id(), id);
        }
    }
    Ok((transaction, changed))
}

/// Prepare Manim `Mobject.add_to_back` ordering for one family root.
///
/// Incoming identities keep their first-occurrence order, matching
/// `dict.fromkeys(mobjects)`, and are moved ahead of the existing direct members
/// in one semantic transaction. Existing child identities and parent edges are
/// retained.
pub(crate) fn family_prepend_transaction(
    family: MobjectTarget<'_>,
    members: &[MobjectTarget<'_>],
) -> Result<SemanticMutationTransaction, AuthoringError> {
    family.validate()?;
    for member in members {
        member.require_store(family.integration_store())?;
    }

    let (current, incoming, observed) = {
        let store = family.integration_store().borrow();
        let node = store
            .semantic_authoring_node_checked(family.node_id())
            .map_err(AuthoringError::from)?;
        let current = node.members().to_vec();
        let mut seen = BTreeSet::new();
        let incoming = members
            .iter()
            .map(MobjectTarget::node_id)
            .filter(|member| seen.insert(*member))
            .collect::<Vec<_>>();
        (
            current,
            incoming,
            store.member_container_is_observed(family.node_id()),
        )
    };

    let incoming_set = incoming.iter().copied().collect::<BTreeSet<_>>();
    let mut desired = incoming.clone();
    desired.extend(
        current
            .iter()
            .copied()
            .filter(|member| !incoming_set.contains(member)),
    );

    let current_set = current.iter().copied().collect::<BTreeSet<_>>();
    let mut transaction = SemanticMutationTransaction::ordered_membership();
    if !observed && desired == current {
        return Ok(transaction);
    }
    if observed {
        // CE removes incoming children from the original list before replacing it.
        // Retained iterators observe this even when the final order is unchanged.
        for member in incoming
            .iter()
            .copied()
            .filter(|member| current_set.contains(member))
        {
            transaction.remove_member(family.node_id(), member);
        }
        transaction.replace_member_container(family.node_id());
    }
    for member in incoming.iter().copied() {
        if observed || !current_set.contains(&member) {
            transaction.add_member(family.node_id(), member);
        }
    }
    let mut before = None;
    for member in desired.iter().rev().copied() {
        transaction.reorder_member(family.node_id(), member, before);
        before = Some(member);
    }
    Ok(transaction)
}

/// Prepare Manim `Mobject.insert` ordering for one new direct family member.
///
/// The Python boundary resolves list-style negative and oversized indices to a
/// non-negative position. Semantic families store each direct identity once;
/// inserting an identity already present therefore moves that identity instead
/// of creating a repeated direct edge.
pub(crate) fn family_insert_transaction(
    family: MobjectTarget<'_>,
    index: usize,
    member: MobjectTarget<'_>,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    family.validate()?;
    member.require_store(family.integration_store())?;

    let (current, present) = {
        let store = family.integration_store().borrow();
        let node = store
            .semantic_authoring_node_checked(family.node_id())
            .map_err(AuthoringError::from)?;
        (
            node.members().to_vec(),
            node.contains_member(member.node_id()),
        )
    };

    let position = index.min(current.len());
    let mut transaction = SemanticMutationTransaction::new();
    if present {
        let mut remaining = current;
        remaining.retain(|current| *current != member.node_id());
        let before = remaining.get(position.min(remaining.len())).copied();
        transaction.reorder_member(family.node_id(), member.node_id(), before);
    } else {
        let before = current.get(position).copied();
        transaction
            .add_member(family.node_id(), member.node_id())
            .reorder_member(family.node_id(), member.node_id(), before);
    }
    Ok(transaction)
}

/// Prepare Manim `Mobject.invert` member ordering for a family DAG.
///
/// Recursive Manim inversion invokes a shared subfamily once for each path that
/// reaches it. Reversal is an involution, so only nodes with an odd number of
/// root-to-node paths need one order reversal. The path parity is propagated in
/// topological order to avoid expanding shared family paths exponentially.
pub(crate) fn family_invert_transaction(
    family: &MobjectFamily,
    recursive: bool,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    family.validate()?;
    let root = family.node_id();
    let families_to_invert = {
        let store = family.integration_store().borrow();
        let root_members = store
            .semantic_family_checked(root)
            .map_err(AuthoringError::from)?
            .members()
            .to_vec();
        if !recursive {
            BTreeMap::from([(root, root_members)])
        } else {
            let mut graph =
                BTreeMap::<SemanticNodeId, (Vec<SemanticNodeId>, Vec<SemanticNodeId>)>::new();
            let mut pending = VecDeque::from([root]);
            while let Some(current) = pending.pop_front() {
                if graph.contains_key(&current) {
                    continue;
                }
                let members = store
                    .semantic_family_checked(current)
                    .map_err(AuthoringError::from)?
                    .members()
                    .to_vec();
                let children = members
                    .iter()
                    .copied()
                    .filter(|member| {
                        store
                            .node(*member)
                            .is_some_and(|node| matches!(node.kind(), SemanticNodeKind::Family(_)))
                    })
                    .collect::<Vec<_>>();
                pending.extend(children.iter().copied());
                graph.insert(current, (members, children));
            }

            let mut indegree = graph
                .keys()
                .map(|node| (*node, 0_usize))
                .collect::<BTreeMap<_, _>>();
            for (_, children) in graph.values() {
                for child in children {
                    *indegree
                        .get_mut(child)
                        .expect("discovered family child has a graph entry") += 1;
                }
            }
            let mut ready = indegree
                .iter()
                .filter_map(|(node, degree)| (*degree == 0).then_some(*node))
                .collect::<VecDeque<_>>();
            let mut path_parity = BTreeMap::from([(root, true)]);
            let mut visited = 0;
            while let Some(current) = ready.pop_front() {
                visited += 1;
                let current_is_odd = path_parity.get(&current).copied().unwrap_or(false);
                for child in &graph[&current].1 {
                    if current_is_odd {
                        let parity = path_parity.entry(*child).or_default();
                        *parity = !*parity;
                    }
                    let degree = indegree
                        .get_mut(child)
                        .expect("discovered family child has an indegree");
                    *degree -= 1;
                    if *degree == 0 {
                        ready.push_back(*child);
                    }
                }
            }
            debug_assert_eq!(visited, graph.len(), "semantic family graph is acyclic");
            graph
                .into_iter()
                .filter_map(|(node, (members, _))| {
                    path_parity
                        .get(&node)
                        .copied()
                        .unwrap_or(false)
                        .then_some((node, members))
                })
                .collect()
        }
    };

    let mut transaction = SemanticMutationTransaction::new();
    for (family, members) in families_to_invert {
        for member in members
            .iter()
            .take(members.len().saturating_sub(1))
            .rev()
            .copied()
        {
            transaction.reorder_member(family, member, None);
        }
    }
    Ok(transaction)
}

impl MobjectFamily {
    /// Create a detached family, including empty and nested families, atomically.
    pub fn create(
        store: Rc<RefCell<SemanticStore>>,
        members: &[MobjectTarget<'_>],
    ) -> Result<Self, AuthoringError> {
        Self::create_with_z_index(store, members, 0.0)
    }

    /// Construct a family with its own priority; member priorities are preserved.
    pub fn create_with_z_index(
        store: Rc<RefCell<SemanticStore>>,
        members: &[MobjectTarget<'_>],
        z_index: f64,
    ) -> Result<Self, AuthoringError> {
        Self::create_with_own_color(store, members, z_index, crate::Color::WHITE)
    }

    /// Construct root metadata in the membership transaction without recoloring children.
    pub fn create_with_own_color(
        store: Rc<RefCell<SemanticStore>>,
        members: &[MobjectTarget<'_>],
        z_index: f64,
        color: crate::Color,
    ) -> Result<Self, AuthoringError> {
        Self::create_with_presentation(store, members, z_index, color, true)
    }

    /// Construct root paint, priority and drawing eligibility with membership atomically.
    pub fn create_with_presentation(
        store: Rc<RefCell<SemanticStore>>,
        members: &[MobjectTarget<'_>],
        z_index: f64,
        color: crate::Color,
        draw_enabled: bool,
    ) -> Result<Self, AuthoringError> {
        let (transaction, family) = family_creation_presentation_transaction(
            &store,
            members,
            z_index,
            color,
            draw_enabled,
        )?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        let node = result
            .resolve(family)
            .expect("committed family token resolves");
        Self::from_node(store, node)
    }

    /// Reverse this family's direct member order, optionally through nested
    /// families while preserving Manim's repeated-alias call parity.
    pub fn invert(&self, recursive: bool) -> Result<(), AuthoringError> {
        let transaction = family_invert_transaction(self, recursive)?;
        if !transaction.mutations().is_empty() {
            transaction
                .apply(&mut self.store.borrow_mut())
                .map_err(AuthoringError::from)?;
        }
        Ok(())
    }

    pub fn from_node(
        store: Rc<RefCell<SemanticStore>>,
        node: SemanticNodeId,
    ) -> Result<Self, AuthoringError> {
        store
            .borrow()
            .semantic_family_checked(node)
            .map_err(AuthoringError::from)?;
        Ok(Self { store, node })
    }

    /// Raw shared arena access for explicit integration, not live mutation.
    ///
    /// External edits can invalidate generational handles and leave an existing
    /// execution session on a stale scene revision. Use `Scene::live` and its
    /// coherent publication operations for edits after lowering. No revision
    /// validation is bypassed by this accessor; see [`crate::integration`].
    pub fn integration_store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
    }

    pub const fn node_id(&self) -> SemanticNodeId {
        self.node
    }

    /// Validate this handle without mutation, preserving typed identity/resource errors.
    pub fn validate(&self) -> Result<(), AuthoringError> {
        self.store
            .borrow()
            .semantic_family_checked(self.node)
            .map(|_| ())
            .map_err(Into::into)
    }

    /// The non-rendered root's authored color, independent of member paint.
    pub fn own_color(&self) -> Result<crate::Color, AuthoringError> {
        self.validate()?;
        Ok(self
            .store
            .borrow()
            .node(self.node)
            .expect("validated family")
            .family_color()
            .expect("family state"))
    }

    /// Authored aggregate world-space bounds center for this family's leaves.
    pub fn world_center(&self) -> Result<noon_core::SemanticVec3, AuthoringError> {
        crate::world_affine::target_world_center(&self.store, MobjectTarget::Family(self))
    }

    /// Aggregate the current layout bounds of this family's authoritative leaves.
    pub fn layout_bounds(&self) -> Result<Option<Bounds2D64>, AuthoringError> {
        Ok(self.layout()?.bounds())
    }

    /// Atomically hide every direct object member before a shared subset display.
    pub fn prepare_subset_display(&self) -> Result<(), String> {
        let transaction = {
            let store = self.store.borrow();
            prepare_subset_display_transaction(&store, self.node)?
        };
        transaction
            .apply(&mut self.store.borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn prepare_subset_display_transaction(
    store: &SemanticStore,
    family: SemanticNodeId,
) -> Result<SemanticMutationTransaction, String> {
    let members = store
        .semantic_family_members_checked(family)
        .map_err(|error| error.to_string())?;
    if members.is_empty() {
        return Err("subset display requires at least one direct family member".into());
    }
    let mut transaction = SemanticMutationTransaction::new();
    for member in members {
        let mut style = store
            .semantic_object_state_checked(member)
            .map_err(|_| "subset display supports direct object members, not nested families")?
            .style
            .clone();
        crate::semantic_mobject::edit_manim_opacity(&mut style, 0.0)
            .map_err(|error| error.to_string())?;
        transaction.replace_style(member, style);
    }
    Ok(transaction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;

    #[test]
    fn updater_suspension_is_shared_recursive_atomic_and_detached_only() {
        let mut scene = Scene::new();
        let first = scene.circle(0.2).unwrap();
        let second = scene.square(0.4).unwrap();
        let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let outer = scene.family(&[(&first).into(), (&nested).into()]).unwrap();

        let before = scene.integration_store().borrow().scene_revision();
        MobjectTarget::from(&outer)
            .set_updating_suspended(true, true)
            .unwrap();
        assert!(MobjectTarget::from(&outer).updating_suspended().unwrap());
        assert!(first.updating_suspended().unwrap());
        assert!(second.updating_suspended().unwrap());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );

        let after_suspend = scene.integration_store().borrow().scene_revision();
        MobjectTarget::from(&outer)
            .set_updating_suspended(true, true)
            .unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            after_suspend
        );

        MobjectTarget::from(&outer)
            .set_updating_suspended(false, false)
            .unwrap();
        assert!(!MobjectTarget::from(&outer).updating_suspended().unwrap());
        assert!(first.updating_suspended().unwrap());
        assert!(second.updating_suspended().unwrap());

        scene
            .integration_store()
            .borrow_mut()
            .attach_to_scene(outer.node_id())
            .unwrap();
        let before_rejected_live_write = scene.integration_store().borrow().scene_revision();
        assert!(MobjectTarget::from(&outer)
            .set_updating_suspended(false, true)
            .is_err());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_rejected_live_write
        );
        assert!(first.updating_suspended().unwrap());
        assert!(second.updating_suspended().unwrap());
    }

    #[test]
    fn updater_suspension_survives_object_and_family_copies() {
        let mut scene = Scene::new();
        let first = scene.circle(0.2).unwrap();
        let second = scene.square(0.4).unwrap();
        MobjectTarget::from(&first)
            .set_updating_suspended(true, false)
            .unwrap();
        MobjectTarget::from(&second)
            .set_updating_suspended(true, false)
            .unwrap();
        let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        MobjectTarget::from(&nested)
            .set_updating_suspended(true, false)
            .unwrap();

        let first_copy = first.copy_handle().unwrap();
        assert!(MobjectTarget::from(&first_copy)
            .updating_suspended()
            .unwrap());
        assert!(MobjectTarget::from(&first.target_editor().unwrap())
            .updating_suspended()
            .unwrap());

        let copied = nested.copy_family().unwrap();
        assert!(MobjectTarget::from(copied.root())
            .updating_suspended()
            .unwrap());
        assert!(MobjectTarget::from(&copied.mobject(&first).unwrap())
            .updating_suspended()
            .unwrap());
        assert!(MobjectTarget::from(&copied.mobject(&second).unwrap())
            .updating_suspended()
            .unwrap());
    }

    #[test]
    fn family_prepend_matches_add_to_back_order_atomically() {
        let mut scene = Scene::new();
        let first = scene.circle(0.2).unwrap();
        let second = scene.square(0.4).unwrap();
        let third = scene.circle(0.3).unwrap();
        let added = scene.rectangle(0.3, 0.2).unwrap();
        let family = scene
            .family(&[(&first).into(), (&second).into(), (&third).into()])
            .unwrap();

        let before = scene.integration_store().borrow().scene_revision();
        family
            .prepend_many(&[
                (&third).into(),
                (&first).into(),
                (&added).into(),
                (&third).into(),
            ])
            .unwrap();
        let members = scene
            .integration_store()
            .borrow()
            .semantic_family_checked(family.node_id())
            .unwrap()
            .members()
            .to_vec();
        assert_eq!(
            members,
            vec![
                third.node_id(),
                first.node_id(),
                added.node_id(),
                second.node_id()
            ]
        );
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );

        let unchanged = scene.integration_store().borrow().scene_revision();
        family
            .prepend_many(&[
                (&third).into(),
                (&first).into(),
                (&added).into(),
                (&third).into(),
            ])
            .unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            unchanged
        );
    }

    #[test]
    fn family_insert_matches_indexed_direct_member_order_atomically() {
        let mut scene = Scene::new();
        let first = scene.circle(0.2).unwrap();
        let second = scene.square(0.4).unwrap();
        let third = scene.circle(0.3).unwrap();
        let added = scene.rectangle(0.3, 0.2).unwrap();
        let family = scene
            .family(&[(&first).into(), (&second).into(), (&third).into()])
            .unwrap();

        let before = scene.integration_store().borrow().scene_revision();
        family.insert_member(1, (&added).into()).unwrap();
        let members = scene
            .integration_store()
            .borrow()
            .semantic_family_checked(family.node_id())
            .unwrap()
            .members()
            .to_vec();
        assert_eq!(
            members,
            vec![
                first.node_id(),
                added.node_id(),
                second.node_id(),
                third.node_id()
            ]
        );
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );

        // An existing semantic identity is moved to the resolved position,
        // while repeated direct edges remain outside the family's data model.
        family.insert_member(0, (&third).into()).unwrap();
        let members = scene
            .integration_store()
            .borrow()
            .semantic_family_checked(family.node_id())
            .unwrap()
            .members()
            .to_vec();
        assert_eq!(
            members,
            vec![
                third.node_id(),
                first.node_id(),
                added.node_id(),
                second.node_id()
            ]
        );

        let unchanged = scene.integration_store().borrow().scene_revision();
        family.insert_member(0, (&third).into()).unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            unchanged
        );
        family.insert_member(usize::MAX, (&added).into()).unwrap();
        let members = scene
            .integration_store()
            .borrow()
            .semantic_family_checked(family.node_id())
            .unwrap()
            .members()
            .to_vec();
        assert_eq!(
            members,
            vec![
                third.node_id(),
                first.node_id(),
                second.node_id(),
                added.node_id()
            ]
        );
    }

    #[test]
    fn family_invert_matches_recursive_alias_path_parity_atomically() {
        let mut scene = Scene::new();
        let first = scene.circle(0.2).unwrap();
        let second = scene.square(0.4).unwrap();
        let third = scene.rectangle(0.3, 0.2).unwrap();
        let direct = scene
            .family(&[(&first).into(), (&second).into(), (&third).into()])
            .unwrap();

        let before = scene.integration_store().borrow().scene_revision();
        direct.invert(false).unwrap();
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_family_checked(direct.node_id())
                .unwrap()
                .members(),
            vec![third.node_id(), second.node_id(), first.node_id()]
        );
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );

        let singleton = scene.family(&[(&first).into()]).unwrap();
        let unchanged = scene.integration_store().borrow().scene_revision();
        singleton.invert(false).unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            unchanged
        );

        let shared = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let left_only = scene.line((0.0, 0.0), (1.0, 0.0)).unwrap();
        let right_only = scene.line((0.0, 0.0), (0.0, 1.0)).unwrap();
        let left = scene
            .family(&[(&shared).into(), (&left_only).into()])
            .unwrap();
        let right = scene
            .family(&[(&shared).into(), (&right_only).into()])
            .unwrap();
        let root = scene.family(&[(&left).into(), (&right).into()]).unwrap();

        let before_recursive = scene.integration_store().borrow().scene_revision();
        root.invert(true).unwrap();
        let store = scene.integration_store();
        let store = store.borrow();
        assert_eq!(
            store
                .semantic_family_checked(root.node_id())
                .unwrap()
                .members(),
            vec![right.node_id(), left.node_id()]
        );
        assert_eq!(
            store
                .semantic_family_checked(left.node_id())
                .unwrap()
                .members(),
            vec![left_only.node_id(), shared.node_id()]
        );
        assert_eq!(
            store
                .semantic_family_checked(right.node_id())
                .unwrap()
                .members(),
            vec![right_only.node_id(), shared.node_id()]
        );
        assert_eq!(
            store
                .semantic_family_checked(shared.node_id())
                .unwrap()
                .members(),
            vec![first.node_id(), second.node_id()]
        );
        assert_eq!(
            store.scene_revision(),
            before_recursive.checked_next().unwrap()
        );
        drop(store);

        let odd_root = scene
            .family(&[(&left).into(), (&right).into(), (&shared).into()])
            .unwrap();
        let before_odd_recursive = scene.integration_store().borrow().scene_revision();
        odd_root.invert(true).unwrap();
        let store = scene.integration_store();
        let store = store.borrow();
        assert_eq!(
            store
                .semantic_family_checked(odd_root.node_id())
                .unwrap()
                .members(),
            vec![shared.node_id(), right.node_id(), left.node_id()]
        );
        assert_eq!(
            store
                .semantic_family_checked(left.node_id())
                .unwrap()
                .members(),
            vec![shared.node_id(), left_only.node_id()]
        );
        assert_eq!(
            store
                .semantic_family_checked(right.node_id())
                .unwrap()
                .members(),
            vec![shared.node_id(), right_only.node_id()]
        );
        assert_eq!(
            store
                .semantic_family_checked(shared.node_id())
                .unwrap()
                .members(),
            vec![second.node_id(), first.node_id()]
        );
        assert_eq!(
            store.scene_revision(),
            before_odd_recursive.checked_next().unwrap()
        );
    }

    #[test]
    fn sheen_direction_preserves_dimensions_family_scope_and_rotation_atomically() {
        let mut scene = Scene::new();
        let first = scene.circle(0.2).unwrap();
        let second = scene.square(0.4).unwrap();
        let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let outer = scene.family(&[(&first).into(), (&nested).into()]).unwrap();

        let up = SemanticSheenDirection::new(SemanticVec3::new(0.0, 1.0, 0.0), 3);
        outer.set_sheen_direction(up, true).unwrap();
        assert_eq!(outer.own_sheen_direction().unwrap(), up);
        assert_eq!(nested.own_sheen_direction().unwrap(), up);
        assert_eq!(first.sheen_direction().unwrap(), up);
        assert_eq!(second.sheen_direction().unwrap(), up);

        let root_only = SemanticSheenDirection::new(SemanticVec3::new(1.0, 0.0, 0.0), 2);
        outer.set_sheen_direction(root_only, false).unwrap();
        assert_eq!(outer.own_sheen_direction().unwrap(), root_only);
        assert_eq!(
            outer.own_sheen_direction().unwrap().components(),
            vec![1.0, 0.0]
        );
        assert_eq!(first.sheen_direction().unwrap(), up);

        let before_invalid = scene.integration_store().borrow().scene_revision();
        let invalid = SemanticSheenDirection::new(SemanticVec3::new(f64::NAN, 0.0, 0.0), 3);
        assert!(outer.set_sheen_direction(invalid, true).is_err());
        let invalid_2d = SemanticSheenDirection::new(SemanticVec3::new(1.0, 0.0, 1.0), 2);
        assert!(outer.set_sheen_direction(invalid_2d, true).is_err());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_invalid
        );
        assert_eq!(outer.own_sheen_direction().unwrap(), root_only);
        assert_eq!(first.sheen_direction().unwrap(), up);

        outer
            .rotate_sheen_direction(
                std::f64::consts::FRAC_PI_2,
                SemanticVec3::new(0.0, 0.0, 1.0),
                true,
            )
            .unwrap();
        let rotated_root = outer.own_sheen_direction().unwrap();
        assert_eq!(rotated_root.dimensions, 3);
        assert!(rotated_root.value.x.abs() < 1e-12);
        assert!((rotated_root.value.y - 1.0).abs() < 1e-12);
        let rotated_leaf = first.sheen_direction().unwrap();
        assert_eq!(rotated_leaf.dimensions, 3);
        assert!((rotated_leaf.value.x + 1.0).abs() < 1e-12);
        assert!(rotated_leaf.value.y.abs() < 1e-12);
    }

    #[test]
    fn copy_and_become_preserve_object_and_family_sheen_directions() {
        let mut scene = Scene::new();
        let source_a = scene.circle(0.2).unwrap();
        let source_b = scene.square(0.4).unwrap();
        let source_nested = scene
            .family(&[(&source_a).into(), (&source_b).into()])
            .unwrap();
        let source = scene
            .family(&[(&source_a).into(), (&source_nested).into()])
            .unwrap();
        let source_direction = SemanticSheenDirection::new(SemanticVec3::new(0.0, 1.0, 0.0), 3);
        source.set_sheen_direction(source_direction, true).unwrap();

        let copied = source.copy_family().unwrap();
        assert_eq!(
            copied.root().own_sheen_direction().unwrap(),
            source_direction
        );
        assert_eq!(
            copied
                .mobject(&source_a)
                .unwrap()
                .sheen_direction()
                .unwrap(),
            source_direction
        );

        let target_a = scene.circle(0.3).unwrap();
        let target_b = scene.square(0.5).unwrap();
        let target_nested = scene
            .family(&[(&target_a).into(), (&target_b).into()])
            .unwrap();
        let target = scene
            .family(&[(&target_a).into(), (&target_nested).into()])
            .unwrap();
        let root_direction = SemanticSheenDirection::new(SemanticVec3::new(1.0, 0.0, 0.0), 3);
        let a_direction = SemanticSheenDirection::new(SemanticVec3::new(0.0, -1.0, 0.0), 3);
        let b_direction = SemanticSheenDirection::new(SemanticVec3::new(1.0, 1.0, 0.0), 3);
        target.set_sheen_direction(root_direction, false).unwrap();
        target_a.set_sheen_direction(a_direction, false).unwrap();
        target_b.set_sheen_direction(b_direction, false).unwrap();

        source
            .become_family(&target, crate::ManimBecomeOptions::default())
            .unwrap();
        assert_eq!(source.own_sheen_direction().unwrap(), root_direction);
        assert_eq!(source_a.sheen_direction().unwrap(), a_direction);
        assert_eq!(source_b.sheen_direction().unwrap(), b_direction);
    }

    #[test]
    fn ordinary_family_arrange_uses_shared_bounds_and_one_transaction() {
        let mut scene = Scene::new();
        let first = scene.square(0.4).unwrap();
        let second = scene.circle(0.2).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let before = scene.integration_store().borrow().scene_revision();

        family.arrange(1.0, 0.0, 0.2, true).unwrap();

        let first_center = first.center().unwrap();
        let second_center = second.center().unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );
        assert!((second_center.0 - first_center.0 - 0.6).abs() < 1e-6);
        assert!((first_center.0 + second_center.0).abs() < 1e-6);
    }

    #[test]
    fn family_arrange_moves_shared_leaves_once_and_rejects_invalid_input() {
        let mut scene = Scene::new();
        let first = scene.circle(0.2).unwrap();
        let mut second = scene.circle(0.2).unwrap();
        second.shift(2.0, 0.0).unwrap();
        let unrelated = scene.square(1.0).unwrap();
        let unrelated_before = unrelated.state().unwrap();
        let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let outer = scene.family(&[(&first).into()]).unwrap();
        scene
            .integration_store()
            .borrow_mut()
            .add_member(outer.node_id(), nested.node_id())
            .unwrap();
        let before = scene.integration_store().borrow().scene_revision();

        assert!(outer.arrange(1.0, 0.0, f64::NAN, true).is_err());
        assert_eq!(scene.integration_store().borrow().scene_revision(), before);
        assert_eq!(first.center().unwrap(), (0.0, 0.0));
        assert_eq!(second.center().unwrap(), (2.0, 0.0));

        outer.arrange(1.0, 0.0, 0.2, true).unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );
        assert!((first.center().unwrap().0 + 1.0).abs() < 1e-6);
        assert!((second.center().unwrap().0 - 1.0).abs() < 1e-6);
        assert_eq!(unrelated.state().unwrap(), unrelated_before);
    }

    #[test]
    fn family_layout_bounds_follow_nested_and_aliased_semantic_leaves() {
        let mut scene = Scene::new();
        let mut first = scene.rectangle(2.0, 1.0).unwrap();
        first.shift(-2.0, 0.0).unwrap();
        let mut second = scene.circle(0.5).unwrap();
        second.shift(2.0, 1.0).unwrap();

        let outer = {
            let mut store = scene.integration_store().borrow_mut();
            let nested = store.insert_family();
            store.add_member(nested, first.node_id()).unwrap();
            store.add_member(nested, second.node_id()).unwrap();
            let outer = store.insert_family();
            store.add_member(outer, first.node_id()).unwrap();
            store.add_member(outer, nested).unwrap();
            outer
        };
        let family = MobjectFamily::from_node(Rc::clone(scene.integration_store()), outer).unwrap();

        assert_eq!(
            family.layout_bounds().unwrap(),
            Some(Bounds2D64 {
                min_x: -3.0,
                min_y: -0.5,
                max_x: 2.5,
                max_y: 1.5,
            })
        );
    }
}
