use noon_core::{
    SemanticObjectState, SemanticStore, SemanticStoreError, SourceIdentity, StoredGeometry,
};

const REUSE_CYCLES: u32 = 1_000;

fn object() -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 })
}

fn source(generation: u32) -> SourceIdentity {
    SourceIdentity::ExplicitKey(format!("semantic-reuse-{generation}"))
}

#[test]
fn semantic_slot_reuse_stays_bounded_and_never_aliases_stale_identity() {
    let mut store = SemanticStore::new();
    let mut current = store.insert_semantic_object(object());
    let mut current_source = source(0);
    store
        .set_source_identity(current, Some(current_source.clone()))
        .expect("initial source identity must install");

    assert_eq!(store.len(), 1);
    assert_eq!(store.slot_capacity(), 1);

    for generation in 1..=REUSE_CYCLES {
        let stale = current;
        let stale_source = current_source;

        store
            .remove_node(stale)
            .expect("live semantic node must remove exactly once");
        assert!(store.node(stale).is_none());
        assert_eq!(store.node_for_source(&stale_source), None);
        assert!(matches!(
            store.remove_node(stale),
            Err(SemanticStoreError::UnknownNode(id)) if id == stale
        ));

        current = store.insert_semantic_object(object());
        current_source = source(generation);
        store
            .set_source_identity(current, Some(current_source.clone()))
            .expect("replacement source identity must install");

        assert_eq!(current.slot(), stale.slot());
        assert_eq!(current.generation(), generation);
        assert_ne!(current, stale);
        assert_eq!(store.len(), 1);
        assert_eq!(store.slot_capacity(), 1);
        assert_eq!(store.node_for_source(&current_source), Some(current));
        assert_eq!(store.node_for_source(&stale_source), None);
    }

    assert_eq!(current.generation(), REUSE_CYCLES);
    assert_eq!(store.len(), 1);
    assert_eq!(store.slot_capacity(), 1);
}
