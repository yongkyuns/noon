use std::collections::HashMap;

use noon_compile::{
    lower_prepared_family_transform_channels, lower_prepared_semantic_animation_schedule,
    prepare_family_transform_activations, EffectiveAnimationProperties,
    PreparedFamilyTransformChannelError, SemanticExecutionIndex,
};
use noon_core::{
    AnimationOptions, ObjectId, Property, RateFunction, SemanticMutationTransaction,
    SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry, Style, TrackValues,
    Transform2D, Vec2,
};

fn object(store: &mut SemanticStore, x: f64) -> noon_core::SemanticNodeId {
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    state.transform.translation = SemanticVec3::new(x, 0.0, 0.0);
    store.insert_semantic_object(state)
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

fn effective(x: f32) -> EffectiveAnimationProperties {
    EffectiveAnimationProperties {
        z_index: 0.0,
        transform: Transform2D {
            translation: Vec2::new(x, 0.0),
            ..Transform2D::IDENTITY
        },
        style: Style::default(),
        appearance: 1.0,
        reveal: 1.0,
    }
}

fn prepared_family_transform<'a>(
    store: &'a mut SemanticStore,
    index: &SemanticExecutionIndex,
    source: noon_core::SemanticNodeId,
    target: noon_core::SemanticNodeId,
) -> (
    noon_core::PreparedSemanticMutationTransaction<'a>,
    noon_compile::PreparedSemanticAnimationScheduleProjection,
) {
    let mut transaction = SemanticMutationTransaction::new();
    let animation = transaction.create_family_transform_animation(
        source,
        target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    );
    let prepared = transaction.prepare(store).unwrap();
    let schedule = lower_prepared_semantic_animation_schedule(
        &prepared,
        index,
        animation,
        1.0,
        AnimationOptions::new(),
    )
    .unwrap();
    (prepared, schedule)
}

#[test]
fn expansion_uses_stable_tracks_for_real_sources_and_identity_free_copy_tracks() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 0.0);
    let s1 = object(&mut store, 2.0);
    let t0 = object(&mut store, 1.0);
    let t1 = object(&mut store, 3.0);
    let t2 = object(&mut store, 5.0);
    let source = family(&mut store, &[s0, s1]);
    let target = family(&mut store, &[t0, t1, t2]);
    store.attach_to_scene(source).unwrap();
    let index = index(&store);
    let s0_id = index.execution_object_id(s0).unwrap();
    let s1_id = index.execution_object_id(s1).unwrap();
    let (prepared, schedule) = prepared_family_transform(&mut store, &index, source, target);

    let mut samples = HashMap::<ObjectId, usize>::new();
    let activation = prepare_family_transform_activations(&prepared, &index, &schedule, |id| {
        *samples.entry(id).or_default() += 1;
        Some(if id == s0_id {
            effective(0.0)
        } else if id == s1_id {
            effective(2.0)
        } else {
            panic!("unexpected source execution object")
        })
    })
    .unwrap();
    let projection = lower_prepared_family_transform_channels(&prepared, &activation).unwrap();

    assert_eq!(samples.get(&s0_id), Some(&1));
    assert_eq!(samples.get(&s1_id), Some(&1));
    assert_eq!(projection.derived_occurrences().len(), 1);
    let copy = &projection.derived_occurrences()[0];
    assert_eq!(copy.occurrence_index, 1);
    assert_eq!(copy.anchor_execution_object_id, s0_id);
    assert_eq!(copy.source, s0);
    assert_eq!(copy.target_state, t1);
    assert_eq!(copy.effective_source, effective(0.0));
    assert!(copy.tracks.iter().any(|track| {
        track.property == Property::Position
            && matches!(
                &track.values,
                TrackValues::Vec2 { from, to }
                    if *from == Vec2::new(0.0, 0.0) && *to == Vec2::new(3.0, 0.0)
            )
    }));
    assert!(copy.tracks.iter().any(|track| {
        track.property == Property::Appearance
            && matches!(
                &track.values,
                TrackValues::Scalar { from, to } if *from == 0.0 && *to == 1.0
            )
    }));

    let stable_sources = projection
        .stable_tracks()
        .iter()
        .filter(|track| track.property == Property::Position)
        .map(|track| track.execution_object_id)
        .collect::<Vec<_>>();
    assert_eq!(stable_sources, vec![s0_id, s1_id]);
    assert!(projection
        .stable_tracks()
        .iter()
        .all(|track| track.property != Property::Appearance));
}

#[test]
fn equal_family_uses_only_existing_stable_track_vocabulary() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 0.0);
    let s1 = object(&mut store, 2.0);
    let t0 = object(&mut store, 1.0);
    let t1 = object(&mut store, 4.0);
    let source = family(&mut store, &[s0, s1]);
    let target = family(&mut store, &[t0, t1]);
    store.attach_to_scene(source).unwrap();
    let index = index(&store);
    let s0_id = index.execution_object_id(s0).unwrap();
    let s1_id = index.execution_object_id(s1).unwrap();
    let (prepared, schedule) = prepared_family_transform(&mut store, &index, source, target);

    let activation = prepare_family_transform_activations(&prepared, &index, &schedule, |id| {
        Some(if id == s0_id {
            effective(0.0)
        } else {
            assert_eq!(id, s1_id);
            effective(2.0)
        })
    })
    .unwrap();
    let projection = lower_prepared_family_transform_channels(&prepared, &activation).unwrap();

    assert!(projection.derived_occurrences().is_empty());
    assert_eq!(
        projection
            .stable_tracks()
            .iter()
            .filter(|track| track.property == Property::Position)
            .map(|track| track.execution_object_id)
            .collect::<Vec<_>>(),
        vec![s0_id, s1_id]
    );
}

#[test]
fn contraction_fails_at_explicit_effective_hold_boundary() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 0.0);
    let s1 = object(&mut store, 2.0);
    let s2 = object(&mut store, 4.0);
    let t0 = object(&mut store, 1.0);
    let t1 = object(&mut store, 5.0);
    let source = family(&mut store, &[s0, s1, s2]);
    let target = family(&mut store, &[t0, t1]);
    store.attach_to_scene(source).unwrap();
    let index = index(&store);
    let ids = [s0, s1, s2].map(|node| index.execution_object_id(node).unwrap());
    let (prepared, schedule) = prepared_family_transform(&mut store, &index, source, target);

    let activation = prepare_family_transform_activations(&prepared, &index, &schedule, |id| {
        let x = if id == ids[0] {
            0.0
        } else if id == ids[1] {
            2.0
        } else {
            assert_eq!(id, ids[2]);
            4.0
        };
        Some(effective(x))
    })
    .unwrap();

    assert!(matches!(
        lower_prepared_family_transform_channels(&prepared, &activation),
        Err(PreparedFamilyTransformChannelError::TargetPaddingRequiresEffectiveHold {
            source: padded_source,
            target_state: padded_target,
            occurrence_index: 1,
            ..
        }) if padded_source == s1 && padded_target == t0
    ));
}
