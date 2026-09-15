use std::collections::HashSet;

use noon_core::{
    SemanticMutationTransaction, SemanticNodeId, SemanticObjectState, SemanticStore, StoredGeometry,
};

use super::{ExecutionSession, SemanticPublicationPurpose};

fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
}

fn family(store: &mut SemanticStore, members: &[SemanticNodeId]) -> SemanticNodeId {
    let family = store.insert_family();
    for &member in members {
        store.add_member(family, member).unwrap();
    }
    family
}

#[test]
fn segment_completion_root_reorder_uses_exact_membership_for_replacement() {
    let mut store = SemanticStore::new();
    let before = object(&mut store, 1.0);
    let source_leaf = object(&mut store, 2.0);
    let after = object(&mut store, 3.0);
    let target_leaf = object(&mut store, 4.0);
    let source_family = family(&mut store, &[source_leaf]);
    let target_family = family(&mut store, &[target_leaf]);
    let root = family(&mut store, &[before, source_family, after]);

    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    let before_execution = session.execution_index.execution_object_id(before).unwrap();
    let source_execution = session
        .execution_index
        .execution_object_id(source_leaf)
        .unwrap();
    let after_execution = session.execution_index.execution_object_id(after).unwrap();
    assert!(session
        .execution_index
        .execution_object_id(target_leaf)
        .is_none());

    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_member(root, source_family);
    transaction.add_member(root, target_family);
    transaction.reorder_member(root, target_family, Some(after));
    let prepared = transaction.prepare(&mut store).unwrap();

    session
        .apply_prepared_scalar_timeline_transaction_with_execution(
            prepared,
            Vec::new(),
            None,
            SemanticPublicationPurpose::SegmentCompletion,
            HashSet::new(),
        )
        .unwrap();

    assert_eq!(
        store.semantic_family_members_checked(root).unwrap(),
        &[before, target_family, after]
    );
    assert_eq!(
        session.execution_index.execution_object_id(source_leaf),
        Some(source_execution),
        "detachment keeps the compatibility key cached"
    );
    assert!(
        session
            .runtime
            .frame_index_for_object(source_execution)
            .is_none(),
        "exact membership must retire the old live source row"
    );
    let target_execution = session
        .execution_index
        .execution_object_id(target_leaf)
        .expect("exact entry must install the detached target identity");
    assert!(session
        .runtime
        .frame_index_for_object(target_execution)
        .is_some());

    let painter_ids = session
        .painter_order()
        .iter()
        .map(|&index| session.frame().objects[index as usize].id)
        .collect::<Vec<_>>();
    assert_eq!(
        painter_ids,
        vec![before_execution, target_execution, after_execution]
    );

    let stats = session.last_structural_publication_stats();
    assert_eq!(stats.preparation.possible_exits, 1);
    assert_eq!(stats.preparation.possible_entries, 1);
    assert_eq!(stats.exited_objects, 1);
    assert_eq!(stats.entered_objects, 1);
}
