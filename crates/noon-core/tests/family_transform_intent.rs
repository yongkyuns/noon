use noon_core::{
    AnimationOptions, SemanticAnimationIntent, SemanticMutationImpact, SemanticMutationTransaction,
    SemanticObjectState, SemanticStore, StoredGeometry,
};

fn object(store: &mut SemanticStore) -> noon_core::SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}

fn family(
    store: &mut SemanticStore,
    members: &[noon_core::SemanticNodeId],
) -> noon_core::SemanticNodeId {
    let family = store.insert_family();
    for &member in members {
        store.add_member(family, member).unwrap();
    }
    family
}

#[test]
fn family_transform_is_one_authored_intent_without_padding_or_topology_change() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store);
    let s1 = object(&mut store);
    let t0 = object(&mut store);
    let t1 = object(&mut store);
    let t2 = object(&mut store);
    let source = family(&mut store, &[s0, s1]);
    let target = family(&mut store, &[t0, t1, t2]);
    let source_before = store
        .semantic_family_members_checked(source)
        .unwrap()
        .to_vec();
    let target_before = store
        .semantic_family_members_checked(target)
        .unwrap()
        .to_vec();
    let before_len = store.len();

    let animation = store
        .insert_semantic_family_transform_animation(
            source,
            target,
            AnimationOptions::new().run_time(2.0),
        )
        .unwrap();

    assert_eq!(store.len(), before_len + 1);
    let state = store.semantic_animation_state(animation).unwrap();
    assert_eq!(state.intent().target(), Some(source));
    assert_eq!(state.intent().target_state(), Some(target));
    assert_eq!(state.intent().family_transform(), Some((source, target)));
    assert!(matches!(
        state.intent(),
        SemanticAnimationIntent::FamilyTransformTo {
            source: actual_source,
            target_state: actual_target,
        } if *actual_source == source && *actual_target == target
    ));
    assert_eq!(
        store.semantic_family_members_checked(source).unwrap(),
        source_before
    );
    assert_eq!(
        store.semantic_family_members_checked(target).unwrap(),
        target_before
    );
}

#[test]
fn family_transform_transaction_adds_only_the_animation_node() {
    let mut store = SemanticStore::new();
    let s = object(&mut store);
    let t0 = object(&mut store);
    let t1 = object(&mut store);
    let source = family(&mut store, &[s]);
    let target = family(&mut store, &[t0, t1]);
    let before_len = store.len();
    let source_before = store
        .semantic_family_members_checked(source)
        .unwrap()
        .to_vec();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.create_family_transform_animation(
        source,
        target,
        AnimationOptions::new().run_time(1.5),
    );
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(store.len(), before_len + 1);
    assert!(matches!(
        result.impacts(),
        [SemanticMutationImpact::AnimationAdded { .. }]
    ));
    assert_eq!(
        store.semantic_family_members_checked(source).unwrap(),
        source_before
    );
}

#[test]
fn family_transform_rejects_object_endpoint_atomically() {
    let mut store = SemanticStore::new();
    let source_object = object(&mut store);
    let target_leaf = object(&mut store);
    let target = family(&mut store, &[target_leaf]);
    let revision = store.scene_revision();
    let len = store.len();

    assert!(store
        .insert_semantic_family_transform_animation(source_object, target, AnimationOptions::new(),)
        .is_err());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.len(), len);

    let mut transaction = SemanticMutationTransaction::new();
    transaction.create_family_transform_animation(source_object, target, AnimationOptions::new());
    assert!(transaction.apply(&mut store).is_err());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.len(), len);
}
