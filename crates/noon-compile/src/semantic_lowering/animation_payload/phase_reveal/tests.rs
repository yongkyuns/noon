use super::*;
use crate::{
    lower_prepared_semantic_animation_composition, lower_semantic_affine_animation_tracks,
    lower_semantic_animation_schedule, PreparedSemanticAnimationLoweringError,
    SemanticExecutionIndex,
};
use noon_core::{
    mapped_continuous_progress_f64, AnimationOptions, RateFunction,
    SemanticAnimationCompositionKind, SemanticMutationTransaction, SemanticNodeId, StoredGeometry,
    TrackId, Vec2,
};

fn path() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, 0.0))
        .line_to(Vec2::new(1.0, 3.0))
}

fn target(store: &mut SemanticStore, path: VectorPath) -> SemanticNodeId {
    let handle = store.insert_geometry_path(path).unwrap();
    let object =
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(handle)));
    store.attach_to_scene(object).unwrap();
    object
}

fn map() -> Arc<RevealPhaseMap> {
    Arc::new(RevealPhaseMap::new(vec![0.0, 0.1, 1.0]).unwrap())
}

#[test]
fn phase_reveal_prepared_and_published_share_data_rate_and_lifecycle() {
    for rate in [
        RateFunction::Linear,
        RateFunction::EaseInQuad,
        RateFunction::ThereAndBack,
    ] {
        for reverse in [false, true] {
            for remove in [false, true] {
                let mut store = SemanticStore::new();
                let object = target(&mut store, path());
                let mut index = SemanticExecutionIndex::new();
                index.lower_scene(&store).unwrap();
                let phase_map = map();
                let mut transaction = SemanticMutationTransaction::new();
                let animation = transaction.create_phase_reveal_animation(
                    object,
                    phase_map.clone(),
                    AnimationOptions::new()
                        .rate_func(rate)
                        .reverse_rate_function(reverse)
                        .remover(remove),
                );
                let prepared = transaction.prepare(&mut store).unwrap();
                let activation = lower_prepared_semantic_animation_composition(
                    &prepared,
                    &index,
                    animation,
                    0.0,
                    AnimationOptions::new().run_time(2.0),
                    |_| panic!("phase reveal requires no effective-pose capture"),
                )
                .unwrap();
                assert_eq!(activation.tracks().len(), 1);
                let track = &activation.tracks()[0];
                assert_eq!(track.property, Property::Reveal);
                assert_eq!(track.timing.easing, rate);
                assert_eq!(track.timing.reverse_rate_function, reverse);
                assert_eq!(
                    track.completion,
                    SemanticAnimationCompletion::RevealLifecycle { remove }
                );
                assert!(Arc::ptr_eq(
                    track.values.reveal_phase_map().unwrap(),
                    &phase_map
                ));
                track.with_track_id(TrackId::new(1)).unwrap();
                let progress =
                    mapped_continuous_progress_f64(track.timing, &track.time_map, 0.5).unwrap();
                let expected =
                    phase_map.sample(rate.evaluate_f64(if reverse { 0.75 } else { 0.25 }));
                assert_eq!(phase_map.sample(progress), expected);
                assert_eq!(
                    phase_map.sample(track.values.terminal_progress_f64(track.timing)),
                    phase_map.sample(rate.evaluate_f64(if reverse { 0.0 } else { 1.0 }))
                );
                let committed = prepared.commit();
                let published_animation = committed.resolve(animation).unwrap();
                let schedule = lower_semantic_animation_schedule(
                    &store,
                    &index,
                    published_animation,
                    0.0,
                    AnimationOptions::new().run_time(2.0),
                )
                .unwrap();
                let published =
                    lower_semantic_affine_animation_tracks(&store, &schedule, |_| None).unwrap();
                let lowered = &published.tracks()[0];
                assert_eq!(track.timing, lowered.timing);
                assert_eq!(track.time_map, lowered.time_map);
                assert_eq!(track.values, lowered.values);
                assert_eq!(track.completion, lowered.completion);
                assert!(Arc::ptr_eq(
                    lowered.values.reveal_phase_map().unwrap(),
                    &phase_map
                ));
            }
        }
    }
}

#[test]
fn phase_reveal_nested_sequence_preserves_non_dyadic_timing_and_reversal() {
    let mut store = SemanticStore::new();
    let object = target(&mut store, path());
    let mut index = SemanticExecutionIndex::new();
    index.lower_scene(&store).unwrap();
    let phase_map = map();
    let mut transaction = SemanticMutationTransaction::new();
    let wait = transaction.create_wait_animation(0.2);
    let reveal = transaction.create_phase_reveal_animation(
        object,
        phase_map.clone(),
        AnimationOptions::new()
            .run_time(0.7)
            .rate_func(RateFunction::EaseInQuad)
            .reverse_rate_function(true),
    );
    let sequence = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Sequence,
        [wait, reveal],
        AnimationOptions::new().rate_func(RateFunction::Linear),
    );
    let outer = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [sequence],
        AnimationOptions::new()
            .run_time(0.93)
            .rate_func(RateFunction::Linear),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    let activation = lower_prepared_semantic_animation_composition(
        &prepared,
        &index,
        outer,
        0.0,
        AnimationOptions::new(),
        |_| None,
    )
    .unwrap();
    let track = &activation.tracks()[0];
    assert!(!track.time_map.is_identity());
    let (start, end) =
        noon_core::continuous_time_map_interval(track.timing, &track.time_map).unwrap();
    for local in [0.0, 0.25, 0.5, 1.0] {
        let time = start + (end - start) * local;
        let phase = mapped_continuous_progress_f64(track.timing, &track.time_map, time).unwrap();
        let expected = phase_map.sample(RateFunction::EaseInQuad.evaluate_f64(1.0 - local));
        assert!((phase_map.sample(phase) - expected).abs() < 1e-14);
    }
}

#[test]
fn phase_reveal_rejects_a_candidate_content_morph_atomically() {
    let mut store = SemanticStore::new();
    let object = target(&mut store, path());
    let destination = target(
        &mut store,
        path().transformed(noon_core::Transform2D {
            translation: Vec2::ONE,
            ..noon_core::Transform2D::IDENTITY
        }),
    );
    let mut index = SemanticExecutionIndex::new();
    index.lower_scene(&store).unwrap();
    let before = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    let reveal = transaction.create_phase_reveal_animation(object, map(), AnimationOptions::new());
    let morph =
        transaction.create_transform_animation(object, destination, AnimationOptions::new());
    let root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [reveal, morph],
        AnimationOptions::new(),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    let result = lower_prepared_semantic_animation_composition(
        &prepared,
        &index,
        root,
        0.0,
        AnimationOptions::new(),
        |_| {
            Some(crate::EffectiveAnimationProperties {
                fill_paint: None,
                effects: None,
                z_index: 0.0,
                transform: noon_core::Transform2D::IDENTITY,
                style: noon_core::Style::default(),
                appearance: 1.0,
                reveal: 1.0,
                world_transform: None,
                camera_profile: None,
            })
        },
    );
    assert!(
        matches!(
            result,
            Err(PreparedSemanticAnimationLoweringError::PhaseReveal {
                error: PhaseRevealLoweringError::ContentDriverConflict,
                ..
            }) | Err(PreparedSemanticAnimationLoweringError::UnsupportedContentChange { .. })
        ),
        "{result:?}"
    );
    drop(prepared);
    assert_eq!(store.len(), before);
}

#[test]
fn phase_reveal_rejects_wrong_count_contours_and_morph_without_publication() {
    let invalid = [
        (
            path(),
            Arc::new(RevealPhaseMap::new(vec![0.0, 1.0]).unwrap()),
        ),
        (
            path()
                .move_to(Vec2::new(3.0, 0.0))
                .line_to(Vec2::new(4.0, 0.0)),
            Arc::new(RevealPhaseMap::new(vec![0.0, 0.2, 0.4, 1.0]).unwrap()),
        ),
        (path().with_morph_target(path()), map()),
        (path().close().line_to(Vec2::ONE), map()),
    ];
    for (path, phase_map) in invalid {
        let mut store = SemanticStore::new();
        let object = target(&mut store, path);
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let before = store.len();
        let resource = store.semantic_object_state_checked(object).unwrap().content;
        let mut transaction = SemanticMutationTransaction::new();
        let animation =
            transaction.create_phase_reveal_animation(object, phase_map, AnimationOptions::new());
        let prepared = transaction.prepare(&mut store).unwrap();
        assert!(matches!(
            lower_prepared_semantic_animation_composition(
                &prepared,
                &index,
                animation,
                0.0,
                AnimationOptions::new(),
                |_| None
            ),
            Err(PreparedSemanticAnimationLoweringError::PhaseReveal { .. })
        ));
        drop(prepared);
        assert_eq!(store.len(), before);
        assert_eq!(
            store.semantic_object_state_checked(object).unwrap().content,
            resource
        );
    }
}

#[test]
fn phase_reveal_duplicate_driver_rejection_is_atomic() {
    let mut store = SemanticStore::new();
    let object = target(&mut store, path());
    let mut index = SemanticExecutionIndex::new();
    index.lower_scene(&store).unwrap();
    let before = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    let a = transaction.create_phase_reveal_animation(object, map(), AnimationOptions::new());
    let b = transaction.create_create_animation(object, AnimationOptions::new());
    let root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [a, b],
        AnimationOptions::new(),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    assert!(matches!(
        lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            root,
            0.0,
            AnimationOptions::new(),
            |_| None
        ),
        Err(PreparedSemanticAnimationLoweringError::MultipleDrivers { .. })
    ));
    drop(prepared);
    assert_eq!(store.len(), before);
}

#[test]
fn phase_reveal_rejects_unsupported_target_and_unbounded_rate() {
    let mut store = SemanticStore::new();
    let circle = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(circle).unwrap();
    let before = store.len();
    let mut invalid = SemanticMutationTransaction::new();
    invalid.create_phase_reveal_animation(circle, map(), AnimationOptions::new());
    assert!(invalid.prepare(&mut store).is_err());
    assert_eq!(store.len(), before);

    let object = target(&mut store, path());
    let mut index = SemanticExecutionIndex::new();
    index.lower_scene(&store).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let animation = transaction.create_phase_reveal_animation(
        object,
        map(),
        AnimationOptions::new().rate_func(RateFunction::EaseInBack),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    assert!(matches!(
        lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            animation,
            0.0,
            AnimationOptions::new(),
            |_| None
        ),
        Err(PreparedSemanticAnimationLoweringError::PhaseReveal {
            error: PhaseRevealLoweringError::UnsupportedOptions,
            ..
        })
    ));
}

#[test]
fn phase_reveal_rejects_raw_spatial_points_before_publication() {
    use noon_core::SemanticVec3;
    // Retained commands project these coordinates into XY, but the authored
    // points remain spatial. A planar object role alone must not admit them.
    let spatial = VectorPath::from_manim_points_3d(vec![
        SemanticVec3::new(0.0, 0.0, 1.0),
        SemanticVec3::new(0.3, 0.0, 1.0),
        SemanticVec3::new(0.6, 0.0, 1.0),
        SemanticVec3::new(1.0, 0.0, 1.0),
        SemanticVec3::new(1.0, 0.0, 1.0),
        SemanticVec3::new(1.0, 1.0, 1.0),
        SemanticVec3::new(1.0, 2.0, 1.0),
        SemanticVec3::new(1.0, 3.0, 1.0),
    ]);
    assert_eq!(
        noon_geometry::retained_reveal_curve_count(&spatial).unwrap(),
        2
    );
    let mut store = SemanticStore::new();
    let object = target(&mut store, spatial);
    let mut index = SemanticExecutionIndex::new();
    index.lower_scene(&store).unwrap();
    let before = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    let animation =
        transaction.create_phase_reveal_animation(object, map(), AnimationOptions::new());
    let prepared = transaction.prepare(&mut store).unwrap();
    assert!(matches!(
        lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            animation,
            0.0,
            AnimationOptions::new(),
            |_| None
        ),
        Err(PreparedSemanticAnimationLoweringError::PhaseReveal {
            error: PhaseRevealLoweringError::UnsupportedTarget,
            ..
        })
    ));
    drop(prepared);
    assert_eq!(store.len(), before);
}
