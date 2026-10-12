use noon_compile::{
    lower_prepared_semantic_animation_schedule, lower_semantic_animation_schedule,
    SemanticExecutionIndex,
};
use noon_core::{
    AnimationOptions, RateFunction, SemanticMutationTransaction, SemanticObjectState,
    SemanticStore, StoredGeometry,
};

fn object(store: &mut SemanticStore, radius: f32) -> noon_core::SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
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

fn index(store: &SemanticStore) -> SemanticExecutionIndex {
    let mut index = SemanticExecutionIndex::new();
    index.lower_scene(store).unwrap();
    index
}

#[test]
fn published_family_transform_owns_timing_without_execution_object_identity() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 1.0);
    let s1 = object(&mut store, 0.7);
    let t0 = object(&mut store, 0.4);
    let t1 = object(&mut store, 0.5);
    let t2 = object(&mut store, 0.6);
    let source = family(&mut store, &[s0, s1]);
    let target = family(&mut store, &[t0, t1, t2]);
    store.attach_to_scene(source).unwrap();
    let animation = store
        .insert_semantic_family_transform_animation(
            source,
            target,
            AnimationOptions::new()
                .run_time(2.5)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let source_before = store
        .semantic_family_members_checked(source)
        .unwrap()
        .to_vec();
    let target_before = store
        .semantic_family_members_checked(target)
        .unwrap()
        .to_vec();
    let revision = store.scene_revision();

    let schedule = lower_semantic_animation_schedule(
        &store,
        &index(&store),
        animation,
        3.0,
        AnimationOptions::new(),
    )
    .unwrap();

    assert!(schedule.leaves().is_empty());
    assert!(schedule.scalar_leaves().is_empty());
    assert_eq!(schedule.family_transforms().len(), 1);
    assert_eq!(schedule.len(), 1);
    let family = &schedule.family_transforms()[0];
    assert_eq!(family.animation, animation);
    assert_eq!(family.source, source);
    assert_eq!(family.target_state, target);
    assert_eq!(family.timing.start_time, 3.0);
    assert_eq!(family.timing.duration, 2.5);
    assert_eq!(family.options.rate_func, RateFunction::Linear);
    assert_eq!(store.scene_revision(), revision);
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
fn prepared_family_transform_uses_same_scheduler_without_object_id_lookup() {
    let mut store = SemanticStore::new();
    let source_leaf = object(&mut store, 1.0);
    let target_leaf = object(&mut store, 2.0);
    let source = family(&mut store, &[source_leaf]);
    let target = family(&mut store, &[target_leaf]);
    store.attach_to_scene(source).unwrap();
    let index = index(&store);

    let mut transaction = SemanticMutationTransaction::new();
    let animation = transaction.create_family_transform_animation(
        source,
        target,
        AnimationOptions::new()
            .run_time(1.25)
            .rate_func(RateFunction::Linear),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    let schedule = lower_prepared_semantic_animation_schedule(
        &prepared,
        &index,
        animation,
        4.0,
        AnimationOptions::new(),
    )
    .unwrap();

    assert!(schedule.leaves().is_empty());
    assert!(schedule.scalar_leaves().is_empty());
    assert_eq!(schedule.family_transforms().len(), 1);
    let family = &schedule.family_transforms()[0];
    assert_eq!(family.source.existing(), Some(source));
    assert_eq!(family.target_state.existing(), Some(target));
    assert_eq!(family.timing.start_time, 4.0);
    assert_eq!(family.timing.duration, 1.25);
}

#[test]
fn family_transform_in_sequence_uses_existing_composition_time_map() {
    let mut store = SemanticStore::new();
    let s = object(&mut store, 1.0);
    let t = object(&mut store, 2.0);
    let source = family(&mut store, &[s]);
    let target = family(&mut store, &[t]);
    store.attach_to_scene(source).unwrap();

    let transform = store
        .insert_semantic_family_transform_animation(
            source,
            target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let wait = store.insert_semantic_wait_animation(1.0).unwrap();
    let sequence = store
        .insert_semantic_sequence_animation(
            &[transform, wait],
            AnimationOptions::new().rate_func(RateFunction::Linear),
        )
        .unwrap();

    let schedule = lower_semantic_animation_schedule(
        &store,
        &index(&store),
        sequence,
        5.0,
        AnimationOptions::new(),
    )
    .unwrap();
    assert_eq!(schedule.family_transforms().len(), 1);
    let family = &schedule.family_transforms()[0];
    assert!(!family.time_map.is_identity());
    assert_eq!(family.timing.start_time, 5.0);
    assert_eq!(family.timing.duration, schedule.run_time());
}
