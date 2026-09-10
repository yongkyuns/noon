//! Lower authored family ordering into derived execution painter-order patches.

use std::{
    cell::OnceCell,
    cmp::Ordering,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use noon_core::{
    PreparedSemanticMutationTransaction, SemanticMutation, SemanticNodeId, SemanticNodeKind,
    SemanticStore, SemanticTransactionNodeRef, SemanticTransactionReadError,
};

use super::{
    semantic_execution_object_id, SemanticLoweringError, SemanticPublicationLoweringError,
};
use crate::ExecutionPatch;

type Node = SemanticTransactionNodeRef;

#[derive(Clone, Copy)]
struct Links {
    previous: Option<Node>,
    next: Option<Node>,
}

// Only transaction-touched links are retained. Original order comparison uses
// the store's rank metadata; no whole-family snapshot or prefix walk is needed.
struct FamilyOrder<'a> {
    base: Option<&'a noon_core::SemanticNode>,
    links: HashMap<Node, Option<Links>>,
    moved: HashSet<Node>,
    positions: OnceCell<HashMap<Node, (Option<Node>, usize)>>,
    head: Option<Node>,
    tail: Option<Node>,
}

impl<'a> FamilyOrder<'a> {
    fn new(base: Option<&'a noon_core::SemanticNode>) -> Self {
        Self {
            base,
            links: HashMap::new(),
            moved: HashSet::new(),
            positions: OnceCell::new(),
            head: base.and_then(|node| node.first_member()).map(Into::into),
            tail: base.and_then(|node| node.last_member()).map(Into::into),
        }
    }

    fn links(&self, member: Node) -> Option<Links> {
        self.links.get(&member).copied().unwrap_or_else(|| {
            let member = member.existing()?;
            let base = self.base?;
            base.contains_member(member).then(|| Links {
                previous: base.previous_member(member).map(Into::into),
                next: base.next_member(member).map(Into::into),
            })
        })
    }

    fn next(&self, member: Node) -> Option<Node> {
        self.links(member).and_then(|link| link.next)
    }

    fn set_next(&mut self, member: Option<Node>, next: Option<Node>) {
        if let Some(member) = member {
            let mut link = self
                .links(member)
                .expect("validated predecessor is present");
            link.next = next;
            self.links.insert(member, Some(link));
        } else {
            self.head = next;
        }
    }

    fn set_previous(&mut self, member: Option<Node>, previous: Option<Node>) {
        if let Some(member) = member {
            let mut link = self.links(member).expect("validated successor is present");
            link.previous = previous;
            self.links.insert(member, Some(link));
        } else {
            self.tail = previous;
        }
    }

    fn remove(&mut self, member: Node) {
        if let Some(link) = self.links(member) {
            self.positions.take();
            self.set_next(link.previous, link.next);
            self.set_previous(link.next, link.previous);
            self.links.insert(member, None);
            self.moved.insert(member);
        }
    }

    fn insert_before(&mut self, member: Node, before: Option<Node>) {
        if before == Some(member) || self.links(member).is_some_and(|link| link.next == before) {
            return;
        }
        self.remove(member);
        self.positions.take();
        let previous = before.map_or(self.tail, |anchor| {
            self.links(anchor)
                .expect("validated anchor is present")
                .previous
        });
        self.set_next(previous, Some(member));
        self.set_previous(before, Some(member));
        self.links.insert(
            member,
            Some(Links {
                previous,
                next: before,
            }),
        );
        self.moved.insert(member);
    }

    fn stable_anchor(&self, member: Node) -> (Option<Node>, usize) {
        if !self.moved.contains(&member) {
            return (Some(member), 0);
        }
        let positions = self.positions.get_or_init(|| {
            let mut result = HashMap::new();
            for &member in &self.moved {
                if self.links(member).is_none() || result.contains_key(&member) {
                    continue;
                }
                let mut path = Vec::new();
                let mut current = Some(member);
                while let Some(node) =
                    current.filter(|node| self.moved.contains(node) && !result.contains_key(node))
                {
                    path.push(node);
                    current = self.next(node);
                }
                let (anchor, mut distance) = current
                    .and_then(|node| result.get(&node).copied())
                    .unwrap_or((current, 0));
                for node in path.into_iter().rev() {
                    distance += 1;
                    result.insert(node, (anchor, distance));
                }
            }
            result
        });
        positions[&member]
    }

    fn compare(&self, left: Node, right: Node) -> Ordering {
        let (left_anchor, left_distance) = self.stable_anchor(left);
        let (right_anchor, right_distance) = self.stable_anchor(right);
        match (left_anchor, right_anchor) {
            (left, right) if left == right => right_distance.cmp(&left_distance),
            (None, _) => Ordering::Greater,
            (_, None) => Ordering::Less,
            (Some(left), Some(right)) => self
                .base
                .unwrap()
                .compare_members(left.existing().unwrap(), right.existing().unwrap())
                .expect("unchanged anchors belong to the published family"),
        }
    }
}

fn node_for_root_order(
    store: &SemanticStore,
    node: SemanticNodeId,
) -> Result<&noon_core::SemanticNode, SemanticPublicationLoweringError> {
    #[cfg(test)]
    tests::NODES_VISITED.with(|count| count.set(count.get() + 1));
    store.node(node).ok_or_else(|| {
        SemanticPublicationLoweringError::from(SemanticLoweringError::Store(
            noon_core::SemanticStoreError::UnknownNode(node),
        ))
    })
}

// A borrowed prepared-transaction projection, not an independent semantic graph.
// It holds only changed family links and reverse-edge overrides. Unchanged values
// and relationships remain borrowed from the exclusively held SemanticStore.
struct OrderView<'a, 'store> {
    prepared: &'a PreparedSemanticMutationTransaction<'store>,
    families: HashMap<Node, FamilyOrder<'a>>,
    parents: HashMap<Node, HashMap<Node, bool>>,
}

impl<'a, 'store> OrderView<'a, 'store> {
    fn new(prepared: &'a PreparedSemanticMutationTransaction<'store>) -> Self {
        Self {
            prepared,
            families: HashMap::new(),
            parents: HashMap::new(),
        }
    }

    fn family_mut(
        &mut self,
        family: Node,
    ) -> Result<&mut FamilyOrder<'a>, SemanticPublicationLoweringError> {
        if !self.families.contains_key(&family) {
            let base = family
                .existing()
                .map(|id| node_for_root_order(self.prepared.store(), id))
                .transpose()?;
            self.families.insert(family, FamilyOrder::new(base));
        }
        Ok(self.families.get_mut(&family).unwrap())
    }

    fn parents(&self, member: Node) -> Result<Vec<Node>, SemanticPublicationLoweringError> {
        let overrides = self.parents.get(&member);
        let mut parents = match member.existing() {
            Some(id) => node_for_root_order(self.prepared.store(), id)?
                .parents()
                .iter()
                .copied()
                .map(Into::into)
                .filter(|parent| {
                    overrides
                        .and_then(|items| items.get(parent))
                        .copied()
                        .unwrap_or(true)
                })
                .collect::<HashSet<_>>(),
            None => HashSet::new(),
        };
        if let Some(overrides) = overrides {
            for (&parent, &present) in overrides {
                if present {
                    parents.insert(parent);
                }
            }
        }
        parents.retain(|parent| !self.prepared.node_is_removed(*parent));
        Ok(parents.into_iter().collect())
    }

    fn first(&self, family: Node) -> Result<Option<Node>, SemanticPublicationLoweringError> {
        if let Some(order) = self.families.get(&family) {
            return Ok(order.head);
        }
        match family.existing() {
            Some(id) => Ok(node_for_root_order(self.prepared.store(), id)?
                .first_member()
                .map(Into::into)),
            None => Ok(None),
        }
    }

    fn next(
        &self,
        family: Node,
        member: Node,
    ) -> Result<Option<Node>, SemanticPublicationLoweringError> {
        if let Some(order) = self.families.get(&family) {
            return Ok(order.next(member));
        }
        Ok(
            node_for_root_order(self.prepared.store(), family.existing().unwrap())?
                .next_member(member.existing().unwrap())
                .map(Into::into),
        )
    }

    fn compare(&self, family: Node, left: Node, right: Node) -> Ordering {
        if let Some(order) = self.families.get(&family) {
            return order.compare(left, right);
        }
        node_for_root_order(self.prepared.store(), family.existing().unwrap())
            .expect("validated path parent exists")
            .compare_members(left.existing().unwrap(), right.existing().unwrap())
            .expect("canonical paths contain live family edges")
    }

    fn is_object(&self, node: Node) -> Result<bool, SemanticPublicationLoweringError> {
        match node.existing() {
            Some(id) => Ok(matches!(
                node_for_root_order(self.prepared.store(), id)?.kind(),
                SemanticNodeKind::AuthoringObject
            )),
            None => match self.prepared.object_state(node) {
                Ok(_) => Ok(true),
                Err(SemanticTransactionReadError::NotObject(_)) => Ok(false),
                Err(error) => Err(error.into()),
            },
        }
    }

    fn collect_leaves(
        &self,
        node: Node,
        seen: &mut HashSet<Node>,
        output: &mut HashSet<Node>,
    ) -> Result<(), SemanticPublicationLoweringError> {
        if !seen.insert(node) {
            return Ok(());
        }
        if self.prepared.node_is_removed(node) {
            // Removing a family can transfer first occurrence of surviving shared
            // leaves. Its old descendant closure is affected even though the family
            // itself no longer has a final staged view.
            if let Some(id) = node.existing() {
                for child in node_for_root_order(self.prepared.store(), id)?.members_iter() {
                    self.collect_leaves(child.into(), seen, output)?;
                }
            }
        } else if self.is_object(node)? {
            output.insert(node);
        } else {
            let mut child = self.first(node)?;
            while let Some(member) = child {
                self.collect_leaves(member, seen, output)?;
                child = self.next(node, member)?;
            }
        }
        Ok(())
    }
}

// First-occurrence paths are shared prefix chains, not copied path vectors. The
// reverse-parent dependency closure is memoized once for this preparation.
struct Occurrence {
    node: Node,
    parent: Option<Rc<Occurrence>>,
    depth: usize,
}

fn compare_paths(view: &OrderView<'_, '_>, left: &Occurrence, right: &Occurrence) -> Ordering {
    let (mut a, mut b) = (left, right);
    while a.depth > b.depth {
        a = a.parent.as_deref().unwrap();
    }
    while b.depth > a.depth {
        b = b.parent.as_deref().unwrap();
    }
    let same = |a: &Occurrence, b: &Occurrence| {
        a.node == b.node
            && match (&a.parent, &b.parent) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            }
    };
    if same(a, b) {
        return left.depth.cmp(&right.depth);
    }
    while !Rc::ptr_eq(a.parent.as_ref().unwrap(), b.parent.as_ref().unwrap()) {
        a = a.parent.as_deref().unwrap();
        b = b.parent.as_deref().unwrap();
    }
    view.compare(a.parent.as_ref().unwrap().node, a.node, b.node)
}

struct FirstOccurrences<'a, 'store> {
    view: OrderView<'a, 'store>,
    paths: HashMap<Node, Option<Rc<Occurrence>>>,
}

impl<'a, 'store> FirstOccurrences<'a, 'store> {
    fn path(
        &mut self,
        node: Node,
    ) -> Result<Option<Rc<Occurrence>>, SemanticPublicationLoweringError> {
        if self.view.prepared.node_is_removed(node) {
            return Ok(None);
        }
        if let Some(path) = self.paths.get(&node) {
            return Ok(path.clone());
        }
        let mut first: Option<Rc<Occurrence>> = None;
        for parent in self.view.parents(node)? {
            if let Some(parent) = self.path(parent)? {
                let candidate = Rc::new(Occurrence {
                    node,
                    depth: parent.depth + 1,
                    parent: Some(parent),
                });
                if first
                    .as_ref()
                    .is_none_or(|first| compare_paths(&self.view, &candidate, first).is_lt())
                {
                    first = Some(candidate);
                }
            }
        }
        self.paths.insert(node, first.clone());
        Ok(first)
    }

    fn first_leaf(
        &mut self,
        node: Node,
        parent: &Rc<Occurrence>,
    ) -> Result<Option<Node>, SemanticPublicationLoweringError> {
        let Some(path) = self.path(node)? else {
            return Ok(None);
        };
        // A later occurrence of an entire family is execution-empty. Skip it
        // without traversing its descendants, including diamond alias DAGs.
        if !path
            .parent
            .as_ref()
            .is_some_and(|owner| Rc::ptr_eq(owner, parent))
        {
            return Ok(None);
        }
        if self.view.is_object(node)? {
            return Ok(Some(node));
        }
        let mut child = self.view.first(node)?;
        while let Some(member) = child {
            if let Some(leaf) = self.first_leaf(member, &path)? {
                return Ok(Some(leaf));
            }
            child = self.view.next(node, member)?;
        }
        Ok(None)
    }

    fn successor(
        &mut self,
        mut path: Rc<Occurrence>,
    ) -> Result<Option<Node>, SemanticPublicationLoweringError> {
        while let Some(parent) = path.parent.clone() {
            let mut next = self.view.next(parent.node, path.node)?;
            while let Some(node) = next {
                if let Some(leaf) = self.first_leaf(node, &parent)? {
                    return Ok(Some(leaf));
                }
                next = self.view.next(parent.node, node)?;
            }
            path = parent;
        }
        Ok(None)
    }

    fn execution_id(
        &self,
        node: Node,
    ) -> Result<noon_core::ObjectId, SemanticPublicationLoweringError> {
        self.view
            .prepared
            .planned_node_id(node)
            .map(semantic_execution_object_id)
            .ok_or_else(|| match node {
                Node::Existing(id) => SemanticTransactionReadError::UnknownExistingNode(id).into(),
                Node::Pending(token) => {
                    SemanticTransactionReadError::UnknownPendingNode(token).into()
                }
            })
    }
}

/// Validate the root identity used by semantic publication without traversing its members.
/// Resource-producing callers use the same admission check before importing resources.
pub fn validate_semantic_publication_root(
    store: &SemanticStore,
    root: SemanticNodeId,
) -> Result<(), SemanticPublicationLoweringError> {
    let root_node = node_for_root_order(store, root)?;
    if !matches!(root_node.kind(), SemanticNodeKind::Family) {
        return Err(
            SemanticLoweringError::Store(noon_core::SemanticStoreError::NotFamily(root)).into(),
        );
    }
    Ok(())
}

/// Prepare root-relative painter effects against final staged membership/order.
///
/// This is fallible compiler work before any publication. Only mutation targets,
/// their descendant/reverse-parent dependencies and successor anchors are visited.
/// Family rank metadata resolves distant aliases without scanning root prefixes.
/// The session consumes existing patches and does not interpret semantic ordering.
pub fn prepare_semantic_root_order(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    root: SemanticNodeId,
) -> Result<Vec<ExecutionPatch>, SemanticPublicationLoweringError> {
    validate_semantic_publication_root(prepared.store(), root)?;
    if prepared.node_is_removed(root) {
        return Err(SemanticTransactionReadError::RemovedExistingNode(root).into());
    }
    let mut view = OrderView::new(prepared);
    let mut targets = Vec::new();
    for mutation in prepared.candidate_mutations() {
        match *mutation {
            SemanticMutation::AddMember { family, member } => {
                view.family_mut(family)?.insert_before(member, None);
                view.parents.entry(member).or_default().insert(family, true);
                targets.push(member);
            }
            SemanticMutation::RemoveMember { family, member } => {
                view.family_mut(family)?.remove(member);
                view.parents
                    .entry(member)
                    .or_default()
                    .insert(family, false);
                targets.push(member);
            }
            SemanticMutation::ReorderMember {
                family,
                member,
                before,
            } => {
                view.family_mut(family)?.insert_before(member, before);
                targets.push(member);
            }
            SemanticMutation::RemoveNode { node } => {
                for parent in view.parents(node)? {
                    view.family_mut(parent)?.remove(node);
                }
                targets.push(node);
            }
            _ => {}
        }
    }
    let mut affected = HashSet::new();
    let mut seen = HashSet::new();
    for target in targets {
        view.collect_leaves(target, &mut seen, &mut affected)?;
    }
    let root_path = Rc::new(Occurrence {
        node: root.into(),
        parent: None,
        depth: 0,
    });
    let mut occurrences = FirstOccurrences {
        view,
        paths: HashMap::from([(root.into(), Some(root_path))]),
    };
    let mut ordered = Vec::new();
    for leaf in affected {
        if let Some(path) = occurrences.path(leaf)? {
            ordered.push(path);
        }
    }
    ordered.sort_by(|left, right| compare_paths(&occurrences.view, left, right));
    let mut patches = Vec::with_capacity(ordered.len());
    for path in ordered.into_iter().rev() {
        let object = occurrences.execution_id(path.node)?;
        let before = occurrences
            .successor(path)?
            .map(|node| occurrences.execution_id(node))
            .transpose()?;
        patches.push(ExecutionPatch::ReorderObject { object, before });
    }
    Ok(patches)
}

#[cfg(test)]
mod tests {
    use noon_core::{SemanticMutationTransaction, SemanticObjectState, StoredGeometry};

    use super::*;

    std::thread_local! {
        pub(super) static NODES_VISITED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    #[test]
    fn family_reorder_prepares_leaf_order_without_publishing_semantics() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let family = store.insert_family();
        let nodes = (0..3)
            .map(|_| {
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }))
            })
            .collect::<Vec<_>>();
        store.add_member(family, nodes[1]).unwrap();
        store.add_member(family, nodes[2]).unwrap();
        store.add_member(root, nodes[0]).unwrap();
        store.add_member(root, family).unwrap();
        let revision = store.scene_revision();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.reorder_member(root, family, Some(nodes[0]));
        let prepared = transaction.prepare(&mut store).unwrap();
        let patches = prepare_semantic_root_order(&prepared, root).unwrap();

        assert_eq!(
            patches,
            vec![
                ExecutionPatch::ReorderObject {
                    object: semantic_execution_object_id(nodes[2]),
                    before: Some(semantic_execution_object_id(nodes[0])),
                },
                ExecutionPatch::ReorderObject {
                    object: semantic_execution_object_id(nodes[1]),
                    before: Some(semantic_execution_object_id(nodes[2])),
                },
            ]
        );
        assert_eq!(prepared.store().scene_revision(), revision);
        assert_eq!(
            prepared.store().node(root).unwrap().members(),
            &[nodes[0], family]
        );
    }

    #[test]
    fn empty_anchor_uses_the_next_root_leaf_without_publishing() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let empty = store.insert_family();
        let nodes = (0..3)
            .map(|_| {
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }))
            })
            .collect::<Vec<_>>();
        for member in [nodes[0], empty, nodes[1], nodes[2]] {
            store.add_member(root, member).unwrap();
        }
        let revision = store.scene_revision();
        let mutation_stats = store.last_mutation_stats();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.reorder_member(root, nodes[2], Some(empty));
        let prepared = transaction.prepare(&mut store).unwrap();
        assert_eq!(
            prepare_semantic_root_order(&prepared, root).unwrap(),
            vec![ExecutionPatch::ReorderObject {
                object: semantic_execution_object_id(nodes[2]),
                before: Some(semantic_execution_object_id(nodes[1])),
            }],
        );
        drop(prepared);
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.last_mutation_stats(), mutation_stats);
        assert_eq!(
            store.node(root).unwrap().members(),
            &[nodes[0], empty, nodes[1], nodes[2]]
        );
    }

    #[test]
    fn invalid_roots_are_rejected_even_without_root_mutations() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let unknown = SemanticNodeId::new(u32::MAX, 0);
        let prepared = SemanticMutationTransaction::new()
            .prepare(&mut store)
            .unwrap();
        for (root, expected) in [
            (unknown, noon_core::SemanticStoreError::UnknownNode(unknown)),
            (object, noon_core::SemanticStoreError::NotFamily(object)),
        ] {
            assert!(matches!(prepare_semantic_root_order(&prepared, root),
                Err(SemanticPublicationLoweringError::Value(SemanticLoweringError::Store(error)))
                    if error == expected));
        }
    }

    #[test]
    fn anchor_work_is_independent_of_unrelated_roots_and_anchor_tail() {
        for unrelated in [0, 20_000] {
            let mut store = SemanticStore::new();
            let root = store.insert_family();
            let detached = store.insert_family();
            let anchor_family = store.insert_family();
            let first =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }));
            store.add_member(anchor_family, first).unwrap();
            for family in [root, detached, anchor_family] {
                for _ in 0..unrelated {
                    let node = store.insert_semantic_object(SemanticObjectState::new(
                        StoredGeometry::Circle { radius: 1.0 },
                    ));
                    store.add_member(family, node).unwrap();
                }
            }
            let empty = store.insert_family();
            let moved =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }));
            for member in [empty, anchor_family, moved] {
                store.add_member(root, member).unwrap();
            }
            let mut transaction = SemanticMutationTransaction::new();
            transaction.reorder_member(root, moved, Some(empty));
            let prepared = transaction.prepare(&mut store).unwrap();
            NODES_VISITED.with(|count| count.set(0));
            assert_eq!(
                prepare_semantic_root_order(&prepared, root).unwrap(),
                vec![ExecutionPatch::ReorderObject {
                    object: semantic_execution_object_id(moved),
                    before: Some(semantic_execution_object_id(first)),
                }],
            );
            // Includes reverse-parent paths and successor validation, but still no
            // prefix, unrelated family, or unused anchor-tail traversal.
            assert_eq!(NODES_VISITED.with(|count| count.get()), 12);
        }
    }

    #[test]
    fn provisional_empty_anchor_keeps_the_next_existing_leaf() {
        use noon_core::SemanticNodeCreation;
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let source =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let tail = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 1.0,
        }));
        store.add_member(root, source).unwrap();
        store.add_member(root, tail).unwrap();
        let revision = store.scene_revision();
        let mut tx = SemanticMutationTransaction::new();
        let empty = tx.create_node(SemanticNodeCreation::family());
        tx.add_member(root, empty);
        tx.reorder_member(root, empty, Some(tail));
        tx.reorder_member_ref(root, source, Some(empty.into()));
        let prepared = tx.prepare(&mut store).unwrap();
        assert_eq!(
            prepare_semantic_root_order(&prepared, root).unwrap(),
            vec![ExecutionPatch::ReorderObject {
                object: semantic_execution_object_id(source),
                before: Some(semantic_execution_object_id(tail)),
            }]
        );
        drop(prepared);
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.node(root).unwrap().members(), vec![source, tail]);
    }

    #[test]
    fn alias_dag_work_counts_nodes_not_paths_for_blocks_and_empty_anchors() {
        const DEPTH: usize = 12;
        for empty in [false, true] {
            let mut store = SemanticStore::new();
            let leaf =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }));
            let anchor =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }));
            let mut child = if empty { store.insert_family() } else { leaf };
            for _ in 0..DEPTH {
                let left = store.insert_family();
                let right = store.insert_family();
                let parent = store.insert_family();
                store.add_member(left, child).unwrap();
                store.add_member(right, child).unwrap();
                store.add_member(parent, left).unwrap();
                store.add_member(parent, right).unwrap();
                child = parent;
            }
            let root = store.insert_family();
            let (member, before) = if empty {
                for member in [child, anchor, leaf] {
                    store.add_member(root, member).unwrap();
                }
                (leaf, child)
            } else {
                for member in [anchor, child] {
                    store.add_member(root, member).unwrap();
                }
                (child, anchor)
            };
            let mut tx = SemanticMutationTransaction::new();
            tx.reorder_member(root, member, Some(before));
            let prepared = tx.prepare(&mut store).unwrap();
            NODES_VISITED.with(|count| count.set(0));
            assert_eq!(
                prepare_semantic_root_order(&prepared, root).unwrap(),
                vec![ExecutionPatch::ReorderObject {
                    object: semantic_execution_object_id(leaf),
                    before: Some(semantic_execution_object_id(anchor)),
                }]
            );
            let reads = NODES_VISITED.with(|count| count.get());
            assert!(reads <= 22 * DEPTH + 24);
            println!("alias DAG depth={DEPTH}, empty={empty}: node reads={reads}");
        }
    }

    #[test]
    fn cross_root_alias_lookup_skips_unrelated_prefix_gap_and_owner_tail() {
        for unrelated in [0, 20_000] {
            let mut store = SemanticStore::new();
            let root = store.insert_family();
            let detached = store.insert_family();
            let left = store.insert_family();
            let right = store.insert_family();
            let make = |store: &mut SemanticStore| {
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }))
            };
            let shared = make(&mut store);
            let a = make(&mut store);
            let b = make(&mut store);
            let next = make(&mut store);
            for member in [shared, a] {
                store.add_member(left, member).unwrap();
            }
            for member in [b, shared, next] {
                store.add_member(right, member).unwrap();
            }
            // The detached alias owner is a reverse dependency. Its children
            // other than shared must not be traversed just to prove it detached.
            store.add_member(detached, shared).unwrap();
            for family in [root, detached, right] {
                for _ in 0..unrelated {
                    let node = make(&mut store);
                    store.add_member(family, node).unwrap();
                }
            }
            store.add_member(root, left).unwrap();
            for _ in 0..unrelated {
                let node = make(&mut store);
                store.add_member(root, node).unwrap();
            }
            store.add_member(root, right).unwrap();
            let revision = store.scene_revision();
            let mut tx = SemanticMutationTransaction::new();
            tx.reorder_member(root, left, None);
            let prepared = tx.prepare(&mut store).unwrap();
            NODES_VISITED.with(|count| count.set(0));
            assert_eq!(
                prepare_semantic_root_order(&prepared, root).unwrap(),
                vec![
                    ExecutionPatch::ReorderObject {
                        object: semantic_execution_object_id(a),
                        before: None
                    },
                    ExecutionPatch::ReorderObject {
                        object: semantic_execution_object_id(shared),
                        before: Some(semantic_execution_object_id(next))
                    },
                ]
            );
            let reads = NODES_VISITED.with(|count| count.get());
            println!(
                "cross-root alias extra nodes={}: semantic node reads={reads}",
                4 * unrelated
            );
            assert!(reads <= 40);
            assert_eq!(prepared.store().scene_revision(), revision);
            assert_eq!(
                prepared
                    .store()
                    .node(root)
                    .unwrap()
                    .compare_members(left, right),
                Some(Ordering::Less)
            );
        }
    }

    #[test]
    fn whole_batch_staged_positions_are_cached_not_rewalked_per_comparison() {
        for count in [16, 1024] {
            let mut store = SemanticStore::new();
            let root = store.insert_family();
            let nodes = (0..count)
                .map(|_| {
                    let node = store.insert_semantic_object(SemanticObjectState::new(
                        StoredGeometry::Circle { radius: 1.0 },
                    ));
                    store.add_member(root, node).unwrap();
                    node
                })
                .collect::<Vec<_>>();
            let mut tx = SemanticMutationTransaction::new();
            for &node in &nodes[..count - 1] {
                tx.reorder_member(root, node, None);
            }
            let prepared = tx.prepare(&mut store).unwrap();
            NODES_VISITED.with(|count| count.set(0));
            let patches = prepare_semantic_root_order(&prepared, root).unwrap();
            assert_eq!(patches.len(), count - 1);
            let reads = NODES_VISITED.with(|count| count.get());
            assert!(reads <= 10 * count + 10);
            println!(
                "batch size={count}: node reads={reads}, patches={}",
                patches.len()
            );
        }
    }
}
