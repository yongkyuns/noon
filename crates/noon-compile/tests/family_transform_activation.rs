use std::cell::Cell;

use noon_compile::{
    lower_prepared_semantic_animation_schedule, prepare_family_transform_activations,
    EffectiveAnimationProperties, SemanticExecutionIndex,
};
use noon_core::{
    AnimationOptions, ObjectId, RateFunction, SemanticMutationTransaction, SemanticObjectState,
    SemanticStore, StoredGeometry, Style, Transform2D, Vec2,
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

fn effective(object: ObjectId) -> EffectiveAnimationProperties {
    EffectiveAnimationProperties {
        fill_paint: None,
        effects: None,
        world_transform: None,
        camera_profile: None,
        z_index: 0.0,
        transform: Transform2D {
            translation: Vec2::new(object.get() as f32, -(object.get() as f32)),
            ..Transform2D::IDENTITY
        },
        style: Style::default(),
        appearance: 1.0,
        reveal: 1.0,
    }
}

#[test]
fn expansion_reuses_one_effective_capture_for_repeated_source_occurrence() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 1.0);
    let s1 = object(&mut store, 0.8);
    let t0 = object(&mut store, 0.4);
    let t1 = object(&mut store, 0.5);
    let t2 = object(&mut store, 0.6);
    let source = family(&mut store, &[s0, s1]);
    let target = family(&mut store, &[t0, t1, t2]);
    store.attach_to_scene(source).unwrap();
    let execution = index(&store);

    let mut transaction = SemanticMutationTransaction::new();
    let animation = transaction.create_family_transform_animation(
        source,
        target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    let schedule = lower_prepared_semantic_animation_schedule(
        &prepared,
        &execution,
        animation,
        3.0,
        AnimationOptions::new(),
    )
    .unwrap();

    let samples = Cell::new(0_u32);
    let activation =
        prepare_family_transform_activations(&prepared, &execution, &schedule, |object| {
            samples.set(samples.get() + 1);
            Some(effective(object))
        })
        .unwrap();

    assert_eq!(activation.len(), 3);
    assert_eq!(
        samples.get(),
        2,
        "each distinct source row is captured once"
    );
    let occurrences = activation.occurrences();
    assert_eq!(occurrences[0].source, s0);
    assert_eq!(occurrences[0].target_state, t0.into());
    assert!(!occurrences[0].source_padding);
    assert_eq!(occurrences[1].source, s0);
    assert_eq!(occurrences[1].target_state, t1.into());
    assert!(occurrences[1].source_padding);
    assert_eq!(occurrences[2].source, s1);
    assert_eq!(occurrences[2].target_state, t2.into());
    assert!(!occurrences[2].source_padding);
    assert_eq!(
        occurrences[0].source_execution_object_id,
        occurrences[1].source_execution_object_id
    );
    assert_eq!(
        occurrences[0].effective_source,
        occurrences[1].effective_source
    );
    assert_eq!(occurrences[0].timing.start_time, 3.0);
    assert_eq!(occurrences[0].timing.duration, 2.0);
}

#[test]
fn contraction_marks_repeated_target_without_inventing_source_execution_identity() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 1.0);
    let s1 = object(&mut store, 0.9);
    let s2 = object(&mut store, 0.8);
    let t0 = object(&mut store, 0.4);
    let t1 = object(&mut store, 0.5);
    let source = family(&mut store, &[s0, s1, s2]);
    let target = family(&mut store, &[t0, t1]);
    store.attach_to_scene(source).unwrap();
    let execution = index(&store);

    let mut transaction = SemanticMutationTransaction::new();
    let animation = transaction.create_family_transform_animation(
        source,
        target,
        AnimationOptions::new().rate_func(RateFunction::Linear),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    let schedule = lower_prepared_semantic_animation_schedule(
        &prepared,
        &execution,
        animation,
        0.0,
        AnimationOptions::new(),
    )
    .unwrap();
    let activation =
        prepare_family_transform_activations(&prepared, &execution, &schedule, |object| {
            Some(effective(object))
        })
        .unwrap();

    let occurrences = activation.occurrences();
    assert_eq!(occurrences.len(), 3);
    assert!(!occurrences[0].target_padding);
    assert!(occurrences[1].target_padding);
    assert!(!occurrences[2].target_padding);
    assert!(occurrences
        .iter()
        .all(|occurrence| !occurrence.source_padding));
    assert_ne!(
        occurrences[0].source_execution_object_id,
        occurrences[1].source_execution_object_id
    );
    assert_ne!(
        occurrences[1].source_execution_object_id,
        occurrences[2].source_execution_object_id
    );
}

#[test]
fn local_family_endpoints_stay_pending_and_reject_topology_changes() {
    use noon_core::SemanticNodeCreation;
    for profile in ["flat", "unequal", "nested"] {
        let mut store = SemanticStore::new();
        let leaves = [object(&mut store, 1.0), object(&mut store, 1.0)];
        let source = family(&mut store, &leaves);
        store.attach_to_scene(source).unwrap();
        let execution = index(&store);
        let count = store.len();
        let revision = store.scene_revision();
        let mut transaction = SemanticMutationTransaction::new();
        let target = transaction.create_node(SemanticNodeCreation::family());
        let mut targets = Vec::new();
        for id in leaves
            .into_iter()
            .take(if profile == "unequal" { 1 } else { 2 })
        {
            let leaf = transaction.create_node(SemanticNodeCreation::object(
                store.semantic_object_state_checked(id).unwrap().clone(),
            ));
            targets.push(leaf);
            if profile == "nested" {
                let nested = transaction.create_node(SemanticNodeCreation::family());
                transaction.add_member(nested, leaf);
                transaction.add_member(target, nested);
            } else {
                transaction.add_member(target, leaf);
            }
        }
        let animation =
            transaction.create_family_transform_animation(source, target, AnimationOptions::new());
        let prepared = transaction.prepare(&mut store).unwrap();
        let schedule = lower_prepared_semantic_animation_schedule(
            &prepared,
            &execution,
            animation,
            0.0,
            AnimationOptions::new(),
        )
        .unwrap();
        let activation =
            prepare_family_transform_activations(&prepared, &execution, &schedule, |object| {
                Some(effective(object))
            });
        if profile == "flat" {
            let activation = activation.unwrap();
            for (occurrence, target) in activation.occurrences().iter().zip(targets) {
                assert_eq!(occurrence.target_state, target.into());
                assert!(!occurrence.source_padding && !occurrence.target_padding);
            }
            noon_compile::lower_prepared_family_transform_channels(&prepared, &activation).unwrap();
        } else {
            assert!(activation.is_err(), "{profile} local topology was accepted");
        }
        drop(prepared);
        assert_eq!(store.len(), count);
        assert_eq!(store.scene_revision(), revision);
    }
}
