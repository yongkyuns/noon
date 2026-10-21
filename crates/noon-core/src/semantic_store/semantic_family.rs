use std::collections::{HashMap, HashSet};

use crate::{SemanticNodeId, SemanticNodeKind, SemanticStore, SemanticStoreError};

/// Failure while matching two semantic families for an ordered transform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticFamilyPairingError {
    UnknownNode(SemanticNodeId),
    RootIsNotFamily(SemanticNodeId),
    TopologyMismatch {
        source: SemanticNodeId,
        target: SemanticNodeId,
    },
    UnsupportedLeaf(SemanticNodeId),
    AliasMismatch {
        source: SemanticNodeId,
        target: SemanticNodeId,
    },
    Empty,
}

impl std::fmt::Display for SemanticFamilyPairingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNode(node) => write!(formatter, "unknown semantic node {node:?}"),
            Self::RootIsNotFamily(node) => {
                write!(formatter, "semantic node {node:?} is not a family")
            }
            Self::TopologyMismatch { source, target } => write!(
                formatter,
                "semantic family topology differs at source {source:?} and target {target:?}"
            ),
            Self::UnsupportedLeaf(node) => write!(
                formatter,
                "semantic family leaf {node:?} is not an ordinary semantic object"
            ),
            Self::AliasMismatch { source, target } => write!(
                formatter,
                "semantic family alias structure differs at source {source:?} and target {target:?}"
            ),
            Self::Empty => formatter.write_str("family animation requires at least one leaf"),
        }
    }
}

impl std::error::Error for SemanticFamilyPairingError {}

type FamilyNodePairs = Vec<(SemanticNodeId, SemanticNodeId)>;

impl SemanticStore {
    /// Pair ordinary leaves from two structurally equivalent semantic families.
    ///
    /// Families are traversed together in authoritative member order. Aliases
    /// must occur in the same positions on both sides and produce one pair at
    /// their first occurrence. The complete topology is validated before the
    /// returned pairs can be staged into an animation transaction.
    pub fn ordered_family_leaf_pairs(
        &self,
        source: SemanticNodeId,
        target: SemanticNodeId,
    ) -> Result<Vec<(SemanticNodeId, SemanticNodeId)>, SemanticFamilyPairingError> {
        let (leaves, _) = self.paired_family_nodes(source, target)?;
        if leaves.is_empty() {
            return Err(SemanticFamilyPairingError::Empty);
        }
        Ok(leaves)
    }

    /// Pair family metadata through the same validated topology and alias contract.
    /// Empty roots still have metadata, and every shared family appears only once.
    pub fn ordered_family_root_pairs(
        &self,
        source: SemanticNodeId,
        target: SemanticNodeId,
    ) -> Result<Vec<(SemanticNodeId, SemanticNodeId)>, SemanticFamilyPairingError> {
        self.paired_family_nodes(source, target)
            .map(|(_, families)| families)
    }

    fn paired_family_nodes(
        &self,
        source: SemanticNodeId,
        target: SemanticNodeId,
    ) -> Result<(FamilyNodePairs, FamilyNodePairs), SemanticFamilyPairingError> {
        let require_family = |node_id| {
            let node = self
                .node(node_id)
                .ok_or(SemanticFamilyPairingError::UnknownNode(node_id))?;
            if !matches!(node.kind(), SemanticNodeKind::Family(_)) {
                return Err(SemanticFamilyPairingError::RootIsNotFamily(node_id));
            }
            Ok(())
        };
        require_family(source)?;
        require_family(target)?;

        fn pair(
            store: &SemanticStore,
            source: SemanticNodeId,
            target: SemanticNodeId,
            source_aliases: &mut HashMap<SemanticNodeId, SemanticNodeId>,
            target_aliases: &mut HashMap<SemanticNodeId, SemanticNodeId>,
            leaves: &mut Vec<(SemanticNodeId, SemanticNodeId)>,
            families: &mut Vec<(SemanticNodeId, SemanticNodeId)>,
        ) -> Result<(), SemanticFamilyPairingError> {
            let source_node = store
                .node(source)
                .ok_or(SemanticFamilyPairingError::UnknownNode(source))?;
            let target_node = store
                .node(target)
                .ok_or(SemanticFamilyPairingError::UnknownNode(target))?;
            // Memoize family pairs as well as leaves: a shared family DAG must
            // not expand every alias path, and aliases must agree on both sides.
            match (source_aliases.get(&source), target_aliases.get(&target)) {
                (None, None) => {
                    source_aliases.insert(source, target);
                    target_aliases.insert(target, source);
                }
                (Some(expected_target), Some(expected_source))
                    if *expected_target == target && *expected_source == source =>
                {
                    return Ok(())
                }
                _ => return Err(SemanticFamilyPairingError::AliasMismatch { source, target }),
            }
            match (source_node.kind(), target_node.kind()) {
                (SemanticNodeKind::Family(_), SemanticNodeKind::Family(_)) => {
                    families.push((source, target));
                }
                (SemanticNodeKind::AuthoringObject, SemanticNodeKind::AuthoringObject) => {
                    if source_node.semantic_object_state().is_none() {
                        return Err(SemanticFamilyPairingError::UnsupportedLeaf(source));
                    }
                    if target_node.semantic_object_state().is_none() {
                        return Err(SemanticFamilyPairingError::UnsupportedLeaf(target));
                    }
                    leaves.push((source, target));
                }
                (SemanticNodeKind::Family(_), _)
                | (SemanticNodeKind::AuthoringObject, SemanticNodeKind::Family(_)) => {
                    return Err(SemanticFamilyPairingError::TopologyMismatch { source, target });
                }
                (SemanticNodeKind::Effect(_), _)
                | (_, SemanticNodeKind::Effect(_))
                | (SemanticNodeKind::Signal(_), _)
                | (SemanticNodeKind::Animation(_), _)
                | (_, SemanticNodeKind::Signal(_))
                | (_, SemanticNodeKind::Animation(_)) => {
                    let unsupported = if !matches!(
                        source_node.kind(),
                        SemanticNodeKind::Family(_) | SemanticNodeKind::AuthoringObject
                    ) {
                        source
                    } else {
                        target
                    };
                    return Err(SemanticFamilyPairingError::UnsupportedLeaf(unsupported));
                }
            }
            if source_node.member_count() != target_node.member_count() {
                return Err(SemanticFamilyPairingError::TopologyMismatch { source, target });
            }
            for (source_member, target_member) in
                source_node.members_iter().zip(target_node.members_iter())
            {
                pair(
                    store,
                    source_member,
                    target_member,
                    source_aliases,
                    target_aliases,
                    leaves,
                    families,
                )?;
            }
            Ok(())
        }

        let mut leaves = Vec::new();
        let mut families = Vec::new();
        pair(
            self,
            source,
            target,
            &mut HashMap::new(),
            &mut HashMap::new(),
            &mut leaves,
            &mut families,
        )?;
        Ok((leaves, families))
    }

    /// Return render/animation leaves below `root` in authoritative semantic order.
    ///
    /// Own content precedes ordered children. A family without owned content
    /// contributes only its descendants. Shared nodes are emitted
    /// once at their first occurrence,
    /// instead of animating aliases multiple times. Manim's public family query
    /// retains the last occurrence; use `ordered_last_occurrence_leaf_nodes` for
    /// that distinct ordering contract.
    pub fn ordered_leaf_nodes(
        &self,
        root: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticStoreError> {
        self.ordered_nodes(root, false, false)
    }

    /// Unique leaves in depth-first order of their last membership occurrence.
    /// This matches Manim's family query ordering without expanding every alias
    /// path: reverse traversal visits each semantic node and its edges once.
    pub fn ordered_last_occurrence_leaf_nodes(
        &self,
        root: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticStoreError> {
        self.ordered_nodes(root, false, true)
    }

    /// Unique leaves in first-occurrence order, with the number of membership
    /// paths to each leaf. Recursive Manim fade visits aliases once per path,
    /// unlike animation/family queries that de-duplicate the same descendants.
    /// Topological accumulation visits each node/edge once instead of expanding
    /// every path through a shared DAG. An unrepresentable count fails explicitly.
    pub fn ordered_leaf_multiplicities(
        &self,
        root: SemanticNodeId,
    ) -> Result<Vec<(SemanticNodeId, u64)>, SemanticStoreError> {
        self.ordered_node_multiplicities(root, false)
    }

    /// Include non-rendered family roots in the same bounded membership-path
    /// count used for leaves. Each node is emitted once at its first occurrence.
    pub fn ordered_authoring_multiplicities(
        &self,
        root: SemanticNodeId,
    ) -> Result<Vec<(SemanticNodeId, u64)>, SemanticStoreError> {
        self.ordered_node_multiplicities(root, true)
    }

    fn ordered_node_multiplicities(
        &self,
        root: SemanticNodeId,
        include_families: bool,
    ) -> Result<Vec<(SemanticNodeId, u64)>, SemanticStoreError> {
        fn postorder(
            store: &SemanticStore,
            node: SemanticNodeId,
            seen: &mut HashSet<SemanticNodeId>,
            order: &mut Vec<SemanticNodeId>,
        ) -> Result<(), SemanticStoreError> {
            let entry = store
                .node(node)
                .ok_or(SemanticStoreError::UnknownNode(node))?;
            if !seen.insert(node) {
                return Ok(());
            }
            if entry.can_own_members() {
                for member in entry.members() {
                    postorder(store, member, seen, order)?;
                }
            }
            order.push(node);
            Ok(())
        }
        let nodes = self.ordered_nodes(root, include_families, false)?;
        let mut order = Vec::new();
        postorder(self, root, &mut HashSet::new(), &mut order)?;
        let mut counts = HashMap::from([(root, 1u64)]);
        for node in order.into_iter().rev() {
            let entry = self
                .node(node)
                .ok_or(SemanticStoreError::UnknownNode(node))?;
            if entry.can_own_members() {
                let count = counts[&node];
                for member in entry.members() {
                    let accumulated = counts.entry(member).or_default();
                    *accumulated = accumulated
                        .checked_add(count)
                        .ok_or(SemanticStoreError::FamilyMultiplicityOverflow(root))?;
                }
            }
        }
        Ok(nodes
            .into_iter()
            .map(|node| (node, counts[&node]))
            .collect())
    }

    /// Return root and descendants once in family order, including family roots.
    pub fn ordered_authoring_nodes(
        &self,
        root: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticStoreError> {
        self.ordered_nodes(root, true, false)
    }

    /// Include family roots in Manim's last-occurrence de-duplication order.
    pub fn ordered_last_occurrence_authoring_nodes(
        &self,
        root: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticStoreError> {
        self.ordered_nodes(root, true, true)
    }

    fn ordered_nodes(
        &self,
        root: SemanticNodeId,
        include_families: bool,
        keep_last: bool,
    ) -> Result<Vec<SemanticNodeId>, SemanticStoreError> {
        fn collect(
            store: &SemanticStore,
            node_id: SemanticNodeId,
            seen: &mut HashSet<SemanticNodeId>,
            leaves: &mut Vec<SemanticNodeId>,
            include_families: bool,
            keep_last: bool,
        ) -> Result<(), SemanticStoreError> {
            let node = store
                .node(node_id)
                .ok_or(SemanticStoreError::UnknownNode(node_id))?;
            if !seen.insert(node_id) {
                return Ok(());
            }
            match node.kind() {
                SemanticNodeKind::AuthoringObject | SemanticNodeKind::Family(_) => {
                    let emit = include_families
                        || matches!(node.kind(), SemanticNodeKind::AuthoringObject)
                        || node.semantic_object_state().is_some();
                    if emit && !keep_last {
                        leaves.push(node_id);
                    }
                    let mut members = node.members();
                    if keep_last {
                        members.reverse();
                    }
                    for member in members {
                        collect(store, member, seen, leaves, include_families, keep_last)?;
                    }
                    if emit && keep_last {
                        leaves.push(node_id);
                    }
                }
                SemanticNodeKind::Signal(_)
                | SemanticNodeKind::Animation(_)
                | SemanticNodeKind::Effect(_) => {}
            }
            Ok(())
        }

        // Validate the root even when it is an empty family.
        self.node(root)
            .ok_or(SemanticStoreError::UnknownNode(root))?;

        let mut leaves = Vec::new();
        let mut seen = HashSet::new();
        collect(
            self,
            root,
            &mut seen,
            &mut leaves,
            include_families,
            keep_last,
        )?;
        if keep_last {
            leaves.reverse();
        }
        Ok(leaves)
    }
}

#[cfg(test)]
mod tests {
    use crate::{SemanticObjectState, StoredGeometry};

    use super::*;

    #[test]
    fn point_owning_roots_keep_their_content_and_ordered_descendants() {
        let mut store = SemanticStore::new();
        let root = store.insert_authoring_object();
        let nested = store.insert_family();
        let a = store.insert_authoring_object();
        let b = store.insert_authoring_object();
        store.add_member(nested, a).unwrap();
        store.add_member(nested, b).unwrap();
        store.add_member(root, nested).unwrap();
        store.add_member(root, a).unwrap();
        assert_eq!(store.ordered_leaf_nodes(root).unwrap(), vec![root, a, b]);
        assert_eq!(
            store.ordered_last_occurrence_leaf_nodes(root).unwrap(),
            vec![root, b, a]
        );
        assert_eq!(
            store.ordered_authoring_nodes(root).unwrap(),
            vec![root, nested, a, b]
        );
        assert_eq!(
            store.ordered_last_occurrence_authoring_nodes(root).unwrap(),
            vec![root, nested, b, a]
        );
        assert_eq!(
            store.ordered_leaf_multiplicities(root).unwrap(),
            vec![(root, 1), (a, 2), (b, 1)]
        );
        assert!(store.add_member(a, root).is_err());
        assert_eq!(store.node(root).unwrap().members(), vec![nested, a]);
        store.reorder_member(root, a, Some(nested)).unwrap();
        assert_eq!(store.node(root).unwrap().members(), vec![a, nested]);
        assert!(store.remove_member(root, a).unwrap());
        assert_eq!(store.ordered_leaf_nodes(root).unwrap(), vec![root, a, b]);
    }

    #[test]
    fn pending_point_owning_roots_publish_members_and_payload_atomically() {
        use crate::{SemanticMutationTransaction, SemanticNodeCreation};
        let mut store = SemanticStore::new();
        let mut transaction = SemanticMutationTransaction::new();
        let state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        let root = transaction.create_node(SemanticNodeCreation::object(state.clone()));
        let child = transaction.create_node(SemanticNodeCreation::object(state.clone()));
        transaction.add_member(root, child);
        let prepared = transaction.prepare(&mut store).unwrap();
        assert_eq!(prepared.family_members(root).unwrap(), vec![child.into()]);
        assert_eq!(prepared.proposed_object_state(root).unwrap(), state);
        let result = prepared.commit();
        let root = result.resolve(root).unwrap();
        let child = result.resolve(child).unwrap();
        assert_eq!(store.ordered_leaf_nodes(root).unwrap(), vec![root, child]);
        assert_eq!(store.node(child).unwrap().parents(), &[root]);

        let before = store.len();
        let mut invalid = SemanticMutationTransaction::new();
        let pending = invalid.create_node(SemanticNodeCreation::object(state));
        invalid.add_member(child, pending).add_member(pending, root);
        assert!(invalid.apply(&mut store).is_err());
        assert_eq!(store.len(), before);
        assert_eq!(store.node(child).unwrap().members(), Vec::new());
        assert_eq!(store.node(root).unwrap().members(), vec![child]);
    }

    #[test]
    fn last_occurrence_order_preserves_roots_and_late_shared_leaf_position() {
        let mut store = SemanticStore::new();
        let a = store.insert_authoring_object();
        let b = store.insert_authoring_object();
        let nested = store.insert_family();
        let root = store.insert_family();
        store.add_member(nested, a).unwrap();
        store.add_member(nested, b).unwrap();
        store.add_member(root, nested).unwrap();
        store.add_member(root, a).unwrap();
        assert_eq!(store.ordered_leaf_nodes(root).unwrap(), vec![a, b]);
        assert_eq!(
            store.ordered_authoring_multiplicities(root).unwrap(),
            vec![(root, 1), (nested, 1), (a, 2), (b, 1)]
        );
        assert_eq!(
            store.ordered_last_occurrence_leaf_nodes(root).unwrap(),
            vec![b, a]
        );
        assert_eq!(
            store.ordered_last_occurrence_authoring_nodes(root).unwrap(),
            vec![root, nested, b, a]
        );
        assert_eq!(
            store.ordered_last_occurrence_leaf_nodes(a).unwrap(),
            vec![a]
        );
        let empty = store.insert_family();
        assert!(store
            .ordered_last_occurrence_leaf_nodes(empty)
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .ordered_last_occurrence_authoring_nodes(empty)
                .unwrap(),
            vec![empty]
        );
    }

    #[test]
    fn last_occurrence_order_agrees_with_expanded_paths_in_small_shared_dags() {
        fn expand(store: &SemanticStore, node: SemanticNodeId, out: &mut Vec<SemanticNodeId>) {
            out.push(node);
            for child in store.node(node).unwrap().members() {
                expand(store, child, out);
            }
        }
        for pattern in 0_u32..64 {
            let mut store = SemanticStore::new();
            let leaves: Vec<_> = (0..3).map(|_| store.insert_authoring_object()).collect();
            let families: Vec<_> = (0..3).map(|_| store.insert_family()).collect();
            for (index, family) in families.iter().enumerate() {
                for (slot, leaf) in leaves.iter().enumerate() {
                    if pattern & (1 << ((index + slot) % 6)) != 0 {
                        store.add_member(*family, *leaf).unwrap();
                    }
                }
                if index > 0 {
                    store.add_member(*family, families[index - 1]).unwrap();
                }
            }
            let root = store.insert_family();
            for family in &families {
                store.add_member(root, *family).unwrap();
            }
            for leaf in &leaves {
                store.add_member(root, *leaf).unwrap();
            }
            let mut expanded = Vec::new();
            expand(&store, root, &mut expanded);
            let expected: Vec<_> = expanded
                .iter()
                .enumerate()
                .filter_map(|(index, node)| {
                    (!expanded[index + 1..].contains(node)).then_some(*node)
                })
                .collect();
            assert_eq!(
                store.ordered_last_occurrence_authoring_nodes(root).unwrap(),
                expected
            );
            let expected_leaves: Vec<_> = expected
                .into_iter()
                .filter(|node| {
                    matches!(
                        store.node(*node).unwrap().kind(),
                        SemanticNodeKind::AuthoringObject
                    )
                })
                .collect();
            assert_eq!(
                store.ordered_last_occurrence_leaf_nodes(root).unwrap(),
                expected_leaves
            );
        }
    }

    #[test]
    fn leaf_target_is_its_own_ordered_leaf_sequence() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let authoring = store.insert_authoring_object();

        assert_eq!(store.ordered_leaf_nodes(object).unwrap(), vec![object]);
        assert_eq!(
            store.ordered_leaf_nodes(authoring).unwrap(),
            vec![authoring]
        );
    }

    #[test]
    fn nested_family_preserves_authoritative_depth_first_member_order() {
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let third = store.insert_authoring_object();
        let nested = store.insert_family();
        let root = store.insert_family();

        store.add_member(nested, second).unwrap();
        store.add_member(nested, third).unwrap();
        store.add_member(root, first).unwrap();
        store.add_member(root, nested).unwrap();

        assert_eq!(
            store.ordered_leaf_nodes(root).unwrap(),
            vec![first, second, third]
        );
    }

    #[test]
    fn aliased_leaf_is_emitted_once_at_first_family_occurrence() {
        let mut store = SemanticStore::new();
        let shared = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let nested = store.insert_family();
        let root = store.insert_family();

        store.add_member(nested, second).unwrap();
        store.add_member(nested, shared).unwrap();
        store.add_member(root, shared).unwrap();
        store.add_member(root, nested).unwrap();

        assert_eq!(
            store.ordered_leaf_nodes(root).unwrap(),
            vec![shared, second]
        );
    }

    #[test]
    fn empty_family_has_no_leaves() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        assert!(store.ordered_leaf_nodes(family).unwrap().is_empty());
    }

    #[test]
    fn recursive_leaf_multiplicities_preserve_order_and_bound_shared_dag_work() {
        let mut store = SemanticStore::new();
        let a = store.insert_authoring_object();
        let b = store.insert_authoring_object();
        let nested = store.insert_family();
        let root = store.insert_family();
        store.add_member(nested, a).unwrap();
        store.add_member(nested, b).unwrap();
        store.add_member(root, nested).unwrap();
        store.add_member(root, a).unwrap();
        assert_eq!(
            store.ordered_leaf_multiplicities(root).unwrap(),
            vec![(a, 2), (b, 1)]
        );
        assert_eq!(store.ordered_leaf_nodes(root).unwrap(), vec![a, b]);

        let mut previous = a;
        for level in 1..=64 {
            let left = store.insert_family();
            let right = store.insert_family();
            let next = store.insert_family();
            store.add_member(left, previous).unwrap();
            store.add_member(right, previous).unwrap();
            store.add_member(next, left).unwrap();
            store.add_member(next, right).unwrap();
            if level < 64 {
                assert_eq!(
                    store.ordered_leaf_multiplicities(next).unwrap(),
                    vec![(a, 1u64 << level)]
                );
            } else {
                assert_eq!(
                    store.ordered_leaf_multiplicities(next),
                    Err(SemanticStoreError::FamilyMultiplicityOverflow(next))
                );
                assert_eq!(
                    store.ordered_authoring_multiplicities(next),
                    Err(SemanticStoreError::FamilyMultiplicityOverflow(next))
                );
            }
            previous = next;
        }
    }

    #[test]
    fn root_multiplicities_count_shared_family_paths_and_keep_empty_roots() {
        let mut store = SemanticStore::new();
        let leaf = store.insert_authoring_object();
        let shared = store.insert_family();
        let branch = store.insert_family();
        let root = store.insert_family();
        let empty = store.insert_family();
        store.add_member(shared, leaf).unwrap();
        store.add_member(branch, shared).unwrap();
        store.add_member(root, branch).unwrap();
        store.add_member(root, shared).unwrap();
        assert_eq!(
            store.ordered_authoring_multiplicities(root).unwrap(),
            vec![(root, 1), (branch, 1), (shared, 2), (leaf, 2)]
        );
        assert_eq!(
            store.ordered_leaf_multiplicities(root).unwrap(),
            vec![(leaf, 2)]
        );
        assert_eq!(
            store.ordered_authoring_multiplicities(empty).unwrap(),
            vec![(empty, 1)]
        );
    }

    #[test]
    fn family_pairing_preserves_order_and_matching_aliases() {
        let mut store = SemanticStore::new();
        let source_first = store.insert_semantic_object(crate::SemanticObjectState::new(
            crate::StoredGeometry::Circle { radius: 1.0 },
        ));
        let source_second = store.insert_semantic_object(crate::SemanticObjectState::new(
            crate::StoredGeometry::Circle { radius: 1.0 },
        ));
        let target_first = store.insert_semantic_object(crate::SemanticObjectState::new(
            crate::StoredGeometry::Circle { radius: 1.0 },
        ));
        let target_second = store.insert_semantic_object(crate::SemanticObjectState::new(
            crate::StoredGeometry::Circle { radius: 1.0 },
        ));
        let source_nested = store.insert_family();
        let target_nested = store.insert_family();
        let source = store.insert_family();
        let target = store.insert_family();
        store.add_member(source_nested, source_second).unwrap();
        store.add_member(source_nested, source_first).unwrap();
        store.add_member(target_nested, target_second).unwrap();
        store.add_member(target_nested, target_first).unwrap();
        store.add_member(source, source_first).unwrap();
        store.add_member(source, source_nested).unwrap();
        store.add_member(target, target_first).unwrap();
        store.add_member(target, target_nested).unwrap();

        assert_eq!(
            store.ordered_family_leaf_pairs(source, target).unwrap(),
            vec![(source_first, target_first), (source_second, target_second)]
        );
    }

    #[test]
    fn family_pairing_includes_point_owning_roots_and_validates_their_children() {
        let mut store = SemanticStore::new();
        let objects = (0..4)
            .map(|_| {
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }))
            })
            .collect::<Vec<_>>();
        let source = store.insert_family();
        let target = store.insert_family();
        store.add_member(source, objects[0]).unwrap();
        store.add_member(target, objects[2]).unwrap();
        store.add_member(objects[0], objects[1]).unwrap();
        store.add_member(objects[2], objects[3]).unwrap();
        assert_eq!(
            store.ordered_family_leaf_pairs(source, target).unwrap(),
            [(objects[0], objects[2]), (objects[1], objects[3])]
        );
        store.remove_member(objects[2], objects[3]).unwrap();
        assert_eq!(
            store.ordered_family_leaf_pairs(source, target),
            Err(SemanticFamilyPairingError::TopologyMismatch {
                source: objects[0],
                target: objects[2],
            })
        );
    }

    #[test]
    fn family_pairing_rejects_structural_or_alias_mismatch() {
        let mut store = SemanticStore::new();
        let source_leaf = store.insert_semantic_object(crate::SemanticObjectState::new(
            crate::StoredGeometry::Circle { radius: 1.0 },
        ));
        let target_first = store.insert_semantic_object(crate::SemanticObjectState::new(
            crate::StoredGeometry::Circle { radius: 1.0 },
        ));
        let target_second = store.insert_semantic_object(crate::SemanticObjectState::new(
            crate::StoredGeometry::Circle { radius: 1.0 },
        ));
        let source_nested = store.insert_family();
        let target_nested = store.insert_family();
        let source = store.insert_family();
        let target = store.insert_family();
        store.add_member(source, source_leaf).unwrap();
        store.add_member(source_nested, source_leaf).unwrap();
        store.add_member(source, source_nested).unwrap();
        store.add_member(target, target_first).unwrap();
        store.add_member(target_nested, target_second).unwrap();
        store.add_member(target, target_nested).unwrap();

        assert!(matches!(
            store.ordered_family_leaf_pairs(source, target),
            Err(SemanticFamilyPairingError::AliasMismatch { .. })
        ));

        let nested = store.insert_family();
        store.add_member(nested, source_leaf).unwrap();
        let source_outer = store.insert_family();
        let target_outer = store.insert_family();
        store.add_member(source_outer, nested).unwrap();
        store.add_member(target_outer, target_first).unwrap();
        assert!(matches!(
            store.ordered_family_leaf_pairs(source_outer, target_outer),
            Err(SemanticFamilyPairingError::TopologyMismatch { .. })
        ));
    }

    #[test]
    fn stale_or_unknown_root_fails_closed() {
        let mut store = SemanticStore::new();
        let stale = store.insert_semantic_object(crate::SemanticObjectState::new(
            crate::StoredGeometry::Circle { radius: 1.0 },
        ));
        store.remove_node(stale).unwrap();

        assert_eq!(
            store.ordered_leaf_nodes(stale),
            Err(SemanticStoreError::UnknownNode(stale))
        );
        assert_eq!(
            store.ordered_leaf_nodes(SemanticNodeId::new(u32::MAX, 0)),
            Err(SemanticStoreError::UnknownNode(SemanticNodeId::new(
                u32::MAX,
                0
            )))
        );
    }
}
