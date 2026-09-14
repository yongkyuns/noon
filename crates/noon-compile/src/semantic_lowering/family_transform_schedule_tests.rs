use noon_core::{
    AnimationOptions, RateFunction, SemanticObjectState, SemanticStore, StoredGeometry,
    TrackTiming,
};

use super::{lower_semantic_animation_schedule, SemanticExecutionIndex};

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
