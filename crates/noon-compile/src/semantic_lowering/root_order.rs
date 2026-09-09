//! Lower authored family ordering into derived execution painter-order patches.

use noon_core::{
    PreparedSemanticMutationTransaction, SemanticMutation, SemanticNodeId, SemanticNodeKind,
    SemanticStore, SemanticTransactionNodeRef,
};

use super::{
    semantic_execution_object_id, SemanticLoweringError, SemanticPublicationLoweringError,
};
use crate::ExecutionPatch;

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
        let node_state = store.node(node).ok_or_else(|| {
            SemanticPublicationLoweringError::from(SemanticLoweringError::Store(
                noon_core::SemanticStoreError::UnknownNode(node),
            ))
        })?;
        match node_state.kind() {
            SemanticNodeKind::AuthoringObject => output.push(node),
            SemanticNodeKind::Family => {
                for member in node_state.members() {
                    leaves(store, member, output)?;
                }
            }
            SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => {}
        }
        Ok(())
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
                let mut anchor =
                    if let Some(before) = before.and_then(SemanticTransactionNodeRef::existing) {
                        let mut anchor_leaves = Vec::new();
                        leaves(prepared.store(), before, &mut anchor_leaves)?;
                        anchor_leaves
                            .first()
                            .copied()
                            .map(semantic_execution_object_id)
                    } else {
                        None
                    };
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
}
