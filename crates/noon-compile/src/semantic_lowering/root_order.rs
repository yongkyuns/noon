//! Lower authored family ordering into derived execution painter-order patches.

use noon_core::{
    PreparedSemanticMutationTransaction, SemanticMutation, SemanticNodeId, SemanticNodeKind,
    SemanticStore, SemanticTransactionNodeRef,
};

use super::{
    semantic_execution_object_id, SemanticLoweringError, SemanticPublicationLoweringError,
};
use crate::ExecutionPatch;

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

// An anchor needs only its first visible leaf, not a snapshot of its whole
// descendant family. Empty branches are skipped in authoritative member order.
fn first_leaf(
    store: &SemanticStore,
    node: SemanticNodeId,
) -> Result<Option<SemanticNodeId>, SemanticPublicationLoweringError> {
    let node = node_for_root_order(store, node)?;
    match node.kind() {
        SemanticNodeKind::AuthoringObject => Ok(Some(node.id())),
        SemanticNodeKind::Family => {
            for member in node.members_iter() {
                if let Some(leaf) = first_leaf(store, member)? {
                    return Ok(Some(leaf));
                }
            }
            Ok(None)
        }
        SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => Ok(None),
    }
}

/// Prepare execution order changes for an explicitly rooted authored transaction.
///
/// This is fallible compiler work before semantic or runtime publication. It visits
/// candidate root membership edits and their affected families/anchors, without a
/// whole-scene traversal. The session consumes these derived patches alongside the
/// ordinary publication; it does not interpret semantic family ordering itself.
pub fn prepare_semantic_root_order(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    root: SemanticNodeId,
) -> Result<Vec<ExecutionPatch>, SemanticPublicationLoweringError> {
    fn leaves(
        store: &SemanticStore,
        node: SemanticNodeId,
        output: &mut Vec<SemanticNodeId>,
    ) -> Result<(), SemanticPublicationLoweringError> {
        let node_state = node_for_root_order(store, node)?;
        match node_state.kind() {
            SemanticNodeKind::AuthoringObject => output.push(node),
            SemanticNodeKind::Family => {
                for member in node_state.members_iter() {
                    leaves(store, member, output)?;
                }
            }
            SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => {}
        }
        Ok(())
    }

    let root_node = node_for_root_order(prepared.store(), root)?;
    if !matches!(root_node.kind(), SemanticNodeKind::Family) {
        return Err(SemanticLoweringError::Store(
            noon_core::SemanticStoreError::NotFamily(root),
        )
        .into());
    }

    let mut patches = Vec::new();
    for mutation in prepared.candidate_mutations() {
        match mutation {
            SemanticMutation::AddMember { family, member } if family.existing() == Some(root) => {
                let Some(member) = member.existing() else {
                    continue;
                };
                let mut member_leaves = Vec::new();
                leaves(prepared.store(), member, &mut member_leaves)?;
                for leaf in member_leaves {
                    patches.push(ExecutionPatch::ReorderObject {
                        object: semantic_execution_object_id(leaf),
                        before: None,
                    });
                }
            }
            SemanticMutation::ReorderMember {
                family,
                member,
                before,
            } if family.existing() == Some(root) => {
                let Some(member) = member.existing() else {
                    continue;
                };
                let mut member_leaves = Vec::new();
                leaves(prepared.store(), member, &mut member_leaves)?;
                let mut before = before.and_then(SemanticTransactionNodeRef::existing);
                let mut anchor = None;
                while let Some(candidate) = before {
                    if let Some(leaf) = first_leaf(prepared.store(), candidate)? {
                        anchor = Some(semantic_execution_object_id(leaf));
                        break;
                    }
                    // An empty root member still marks a semantic position. Its
                    // next sibling, not the execution tail, supplies the anchor.
                    before = root_node.next_member(candidate);
                }
                for leaf in member_leaves.into_iter().rev() {
                    let object = semantic_execution_object_id(leaf);
                    patches.push(ExecutionPatch::ReorderObject {
                        object,
                        before: anchor,
                    });
                    anchor = Some(object);
                }
            }
            _ => {}
        }
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
            .map(|_| store.insert_semantic_object(SemanticObjectState::new(
                StoredGeometry::Circle { radius: 1.0 },
            )))
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
        assert_eq!(store.node(root).unwrap().members(), &[nodes[0], empty, nodes[1], nodes[2]]);
    }

    #[test]
    fn invalid_roots_are_rejected_even_without_root_mutations() {
        let mut store = SemanticStore::new();
        let object = store.insert_semantic_object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 1.0 },
        ));
        let unknown = SemanticNodeId::new(u32::MAX, 0);
        let prepared = SemanticMutationTransaction::new().prepare(&mut store).unwrap();
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
            let first = store.insert_semantic_object(SemanticObjectState::new(
                StoredGeometry::Circle { radius: 1.0 },
            ));
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
            let moved = store.insert_semantic_object(SemanticObjectState::new(
                StoredGeometry::Circle { radius: 1.0 },
            ));
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
            // Root validation + moved leaf + empty anchor + anchor family + its
            // first leaf. No prefix, unrelated family, or anchor-tail traversal.
            assert_eq!(NODES_VISITED.with(|count| count.get()), 5);
        }
    }
}
