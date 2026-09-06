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
        let require_family = |node_id| {
            let node = self
                .node(node_id)
                .ok_or(SemanticFamilyPairingError::UnknownNode(node_id))?;
            if !matches!(node.kind(), SemanticNodeKind::Family) {
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
        ) -> Result<(), SemanticFamilyPairingError> {
            let source_node = store
                .node(source)
                .ok_or(SemanticFamilyPairingError::UnknownNode(source))?;
            let target_node = store
                .node(target)
                .ok_or(SemanticFamilyPairingError::UnknownNode(target))?;
            match (source_node.kind(), target_node.kind()) {
                (SemanticNodeKind::Family, SemanticNodeKind::Family) => {
                    if source_node.members().len() != target_node.members().len() {
                        return Err(SemanticFamilyPairingError::TopologyMismatch {
                            source,
                            target,
                        });
                    }
                    for (&source_member, target_member) in
                        source_node.members().iter().zip(target_node.members())
                    {
                        pair(
                            store,
                            source_member,
                            target_member,
                            source_aliases,
                            target_aliases,
                            leaves,
                        )?;
                    }
                    Ok(())
                }
                (SemanticNodeKind::AuthoringObject, SemanticNodeKind::AuthoringObject) => {
                    if source_node.semantic_object_state().is_none() {
                        return Err(SemanticFamilyPairingError::UnsupportedLeaf(source));
                    }
                    if target_node.semantic_object_state().is_none() {
                        return Err(SemanticFamilyPairingError::UnsupportedLeaf(target));
                    }
                    match (source_aliases.get(&source), target_aliases.get(&target)) {
                        (None, None) => {
                            source_aliases.insert(source, target);
                            target_aliases.insert(target, source);
                            leaves.push((source, target));
                            Ok(())
                        }
                        (Some(expected_target), Some(expected_source))
                            if *expected_target == target && *expected_source == source =>
                        {
                            Ok(())
                        }
                        _ => Err(SemanticFamilyPairingError::AliasMismatch { source, target }),
                    }
                }
                (SemanticNodeKind::Family, _)
                | (SemanticNodeKind::AuthoringObject, SemanticNodeKind::Family) => {
                    Err(SemanticFamilyPairingError::TopologyMismatch { source, target })
                }
                (SemanticNodeKind::Object(_), _)
                | (SemanticNodeKind::Signal(_), _)
                | (SemanticNodeKind::Animation(_), _)
                | (_, SemanticNodeKind::Object(_))
                | (_, SemanticNodeKind::Signal(_))
                | (_, SemanticNodeKind::Animation(_)) => {
                    let unsupported = if !matches!(
                        source_node.kind(),
                        SemanticNodeKind::Family | SemanticNodeKind::AuthoringObject
                    ) {
                        source
                    } else {
                        target
                    };
                    Err(SemanticFamilyPairingError::UnsupportedLeaf(unsupported))
                }
            }
        }

        let mut leaves = Vec::new();
        pair(
            self,
            source,
            target,
            &mut HashMap::new(),
            &mut HashMap::new(),
            &mut leaves,
        )?;
        if leaves.is_empty() {
            return Err(SemanticFamilyPairingError::Empty);
        }
        Ok(leaves)
    }

    /// Return render/animation leaves below `root` in authoritative semantic order.
    ///
    /// A non-family object is its own leaf. Families are flattened depth-first in
    /// member order. Shared descendants are emitted once at their first occurrence,
    /// matching Manim's ordered family de-duplication instead of animating aliases
    /// multiple times.
    pub fn ordered_leaf_nodes(
        &self,
        root: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticStoreError> {
        fn collect(
            store: &SemanticStore,
            node_id: SemanticNodeId,
            seen: &mut HashSet<SemanticNodeId>,
            leaves: &mut Vec<SemanticNodeId>,
        ) -> Result<(), SemanticStoreError> {
            let node = store
                .node(node_id)
                .ok_or(SemanticStoreError::UnknownNode(node_id))?;
            match node.kind() {
                SemanticNodeKind::Object(_) | SemanticNodeKind::AuthoringObject => {
                    if seen.insert(node_id) {
                        leaves.push(node_id);
                    }
                }
                SemanticNodeKind::Family => {
                    for member in node.members() {
                        collect(store, member, seen, leaves)?;
                    }
                }
                SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => {}
            }
            Ok(())
        }

        // Validate the root even when it is an empty family.
        self.node(root)
            .ok_or(SemanticStoreError::UnknownNode(root))?;

        let mut leaves = Vec::new();
        let mut seen = HashSet::new();
        collect(self, root, &mut seen, &mut leaves)?;
        Ok(leaves)
    }
}

#[cfg(test)]
mod tests {
    use crate::{GeometryRef, ObjectDefinition, ObjectId};

    use super::*;

    fn object(id: u64) -> ObjectDefinition {
        ObjectDefinition::new(ObjectId::new(id), GeometryRef::circle(1.0))
    }

    #[test]
    fn leaf_target_is_its_own_ordered_leaf_sequence() {
        let mut store = SemanticStore::new();
        let object = store.insert_object(object(1));
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
    fn family_pairing_preserves_order_and_matching_aliases() {
        let mut store = SemanticStore::new();
        let source_first = store.insert_authoring_object();
        let source_second = store.insert_authoring_object();
        let target_first = store.insert_authoring_object();
        let target_second = store.insert_authoring_object();
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
    fn family_pairing_rejects_structural_or_alias_mismatch() {
        let mut store = SemanticStore::new();
        let source_leaf = store.insert_authoring_object();
        let target_first = store.insert_authoring_object();
        let target_second = store.insert_authoring_object();
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
        let stale = store.insert_authoring_object();
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
