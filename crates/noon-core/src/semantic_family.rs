use std::collections::HashSet;

use crate::{SemanticNodeId, SemanticNodeKind, SemanticStore, SemanticStoreError};

impl SemanticStore {
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
                SemanticNodeKind::Signal(_) => {}
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
