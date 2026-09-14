use noon_core::{
    AnimationOptions, Property, RateFunction, SemanticAnimationCompositionKind,
    SemanticMutationTransaction, SemanticObjectState, SemanticStore, StoredGeometry, Style,
    TrackTiming, TrackValues, Transform2D,
};

use super::{
    lower_prepared_family_transform_channels, lower_prepared_semantic_animation_schedule,
    lower_semantic_animation_schedule, prepare_family_transform_activations,
    EffectiveAnimationProperties, SemanticExecutionIndex,
};

fn object(store: &mut SemanticStore) -> noon_core::SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}

fn family(store: &mut SemanticStore, count: usize) -> noon_core::SemanticNodeId {
    let family = store.insert_family();
    for _ in 0..count {
        let member = object(store);
        store.add_member(family, member).unwrap();
    }
    family
}

#[test]
fn one_child_parallel_family_transform_keeps_full_root_time_map() {
    let mut store = SemanticStore::new();
    let source = family(&mut store, 3);
    let target = family(&mut store, 3);
    store.attach_to_scene(source).unwrap();

    let transform = store
        .insert_semantic_family_transform_animation(
            source,
            target,
            AnimationOptions::new()
                .run_time(1.8)
                .rate_func(RateFunction::Smooth),
        )
        .unwrap();
    let root = store
        .insert_semantic_parallel_animation(
            &[transform],
            AnimationOptions::new().rate_func(RateFunction::Linear),
        )
        .unwrap();

    let mut index = SemanticExecutionIndex::new();
    index.lower_scene(&store).unwrap();
    let schedule = lower_semantic_animation_schedule(
        &store,
        &index,
        root,
        3.75,
        AnimationOptions::new(),
    )
    .unwrap();

    assert_eq!(schedule.start_time(), 3.75);
    assert_eq!(schedule.run_time(), 1.8);
    assert_eq!(schedule.family_transforms().len(), 1);
    let family = &schedule.family_transforms()[0];
    assert_eq!(
        family.timing,
        TrackTiming::new(3.75, 1.8, RateFunction::Smooth)
    );
    assert_eq!(family.time_map.steps.len(), 1);
    let step = family.time_map.steps[0];
    assert_eq!(step.start, 0.0);
    assert_eq!(step.duration, 1.0);
    assert_eq!(step.rate_func, RateFunction::Linear);

    let midpoint = family.time_map.evaluate(0.5);
    assert!(midpoint.begun);
    assert_eq!(midpoint.alpha, 0.5);
    let endpoint = family.time_map.evaluate(1.0);
    assert!(endpoint.begun);
    assert_eq!(endpoint.alpha, 1.0);
}

#[test]
fn hidden_real_source_in_one_child_parallel_emits_restoring_appearance_track() {
    let mut store = SemanticStore::new();
    let source_members = (0..3).map(|_| object(&mut store)).collect::<Vec<_>>();
    let source = store.insert_family();
    for &member in &source_members {
        store.add_member(source, member).unwrap();
    }
    let target = family(&mut store, 3);
    store.attach_to_scene(source).unwrap();

    let mut index = SemanticExecutionIndex::new();
    index.lower_scene(&store).unwrap();
    let hidden = index.execution_object_id(source_members[1]).unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    let transform = transaction.create_family_transform_animation(
        source,
        target,
        AnimationOptions::new()
            .run_time(1.8)
            .rate_func(RateFunction::Smooth),
    );
    let root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [transform],
        AnimationOptions::new().rate_func(RateFunction::Linear),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    let schedule = lower_prepared_semantic_animation_schedule(
        &prepared,
        &index,
        root,
        3.75,
        AnimationOptions::new(),
    )
    .unwrap();
    let activation = prepare_family_transform_activations(
        &prepared,
        &index,
        &schedule,
        |object| {
            Some(EffectiveAnimationProperties {
                z_index: 0.0,
                transform: Transform2D::default(),
                style: Style::default(),
                appearance: if object == hidden { 0.0 } else { 1.0 },
                reveal: 1.0,
            })
        },
    )
    .unwrap();
    let channels = lower_prepared_family_transform_channels(&prepared, &activation).unwrap();
    let appearances = channels
        .stable_tracks()
        .iter()
        .filter(|track| track.track.property == Property::Appearance)
        .collect::<Vec<_>>();

    assert_eq!(appearances.len(), 1);
    let restoration = appearances[0];
    assert!(!restoration.retain_effective);
    assert_eq!(
        restoration.track.timing,
        TrackTiming::new(3.75, 1.8, RateFunction::Smooth)
    );
    assert_eq!(restoration.track.time_map.steps.len(), 1);
    assert_eq!(restoration.track.time_map.steps[0].start, 0.0);
    assert_eq!(restoration.track.time_map.steps[0].duration, 1.0);
    assert_eq!(
        restoration.track.time_map.steps[0].rate_func,
        RateFunction::Linear
    );
    assert_eq!(
        restoration.track.values,
        TrackValues::Scalar { from: 0.0, to: 1.0 }
    );
}
