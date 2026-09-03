use noon_core::{
    GeometryResourceArena, SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry, Vec2,
    VectorPath,
};

#[test]
fn semantic_store_owns_object_state_and_assigns_insertion_order() {
    let mut store = SemanticStore::new();

    let mut first_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    first_state.set_z_index(7);
    let first = store.insert_semantic_object(first_state);

    let mut second_state = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: Vec2::new(2.0, 1.0),
    });
    second_state.set_z_index(-3);
    let second = store.insert_semantic_object(second_state);

    let first_state = store.node(first).unwrap().semantic_object_state().unwrap();
    let second_state = store
        .node(second)
        .unwrap()
        .semantic_object_state()
        .unwrap();

    assert_eq!(first_state.z_index(), 7);
    assert_eq!(second_state.z_index(), -3);
    assert_eq!(first_state.insertion_order(), 0);
    assert_eq!(second_state.insertion_order(), 1);
}

#[test]
fn semantic_state_survives_detach_readd_and_family_aliasing() {
    let mut arena = GeometryResourceArena::new();
    let geometry = arena.insert_path(
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(3.0, 4.0)),
    );

    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(
        geometry,
    )));
    let first_family = store.insert_family();
    let second_family = store.insert_family();
    store.add_member(first_family, object).unwrap();
    store.add_member(second_family, object).unwrap();

    store.attach_to_scene(object).unwrap();
    store.detach_from_scene(object).unwrap();
    store.attach_to_scene(object).unwrap();

    let node = store.node(object).unwrap();
    assert_eq!(node.id(), object);
    assert_eq!(node.parents(), &[first_family, second_family]);
    assert_eq!(
        node.semantic_object_state().unwrap().content.geometry(),
        Some(StoredGeometry::Resource(geometry))
    );
    assert!(arena.get(geometry).is_some());
}

#[test]
fn semantic_state_mutation_is_local_and_preserves_content_identity() {
    let mut arena = GeometryResourceArena::new();
    let geometry = arena.insert_path(
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0)),
    );

    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(
        geometry,
    )));
    let unrelated = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 5.0,
    }));
    let unrelated_before = store
        .node(unrelated)
        .unwrap()
        .semantic_object_state()
        .unwrap()
        .clone();

    let state = store
        .node_mut(object)
        .unwrap()
        .semantic_object_state_mut()
        .unwrap();
    state.transform.translation = SemanticVec3::new(10.25, -2.5, 8.0);
    state.style.object_opacity = 0.25;
    state.set_z_index(11);

    let state = store.node(object).unwrap().semantic_object_state().unwrap();
    assert_eq!(
        state.content.geometry(),
        Some(StoredGeometry::Resource(geometry))
    );
    assert_eq!(state.transform.translation.z, 8.0);
    assert_eq!(state.style.object_opacity, 0.25);
    assert_eq!(state.z_index(), 11);
    assert_eq!(
        store
            .node(unrelated)
            .unwrap()
            .semantic_object_state()
            .unwrap(),
        &unrelated_before
    );
    assert_eq!(arena.len(), 1);
}

#[test]
fn deleted_semantic_state_cannot_be_reached_through_stale_generation() {
    let mut store = SemanticStore::new();
    let first = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.remove_node(first).unwrap();

    let second = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 2.0,
    }));
    assert_eq!(first.slot(), second.slot());
    assert_ne!(first.generation(), second.generation());
    assert!(store.node(first).is_none());
    assert_eq!(
        store
            .node(second)
            .unwrap()
            .semantic_object_state()
            .unwrap()
            .content
            .geometry(),
        Some(StoredGeometry::Circle { radius: 2.0 })
    );
}

#[test]
fn identity_only_frontend_seam_is_not_target_object_state() {
    let mut store = SemanticStore::new();
    let identity_only = store.insert_authoring_object();
    assert!(store
        .node(identity_only)
        .unwrap()
        .semantic_object_state()
        .is_none());
}
