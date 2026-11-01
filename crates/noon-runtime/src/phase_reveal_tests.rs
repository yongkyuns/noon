use super::*;
use noon_compile::CompiledObject;
use noon_core::{
    CompositionTimeMap, CompositionTimeMapStep, RateFunction, RevealPhaseMap, TrackId, TrackTiming,
};

fn phase_map() -> Arc<RevealPhaseMap> {
    Arc::new(RevealPhaseMap::new(vec![0.0, 0.13, 0.5, 0.5 + 2.0_f64.powi(-40), 0.73, 1.0]).unwrap())
}

fn fixture(
    map: &Arc<RevealPhaseMap>,
    timing: TrackTiming,
    time_map: CompositionTimeMap,
    static_objects: usize,
) -> (SceneInstance, Arc<VectorPath>) {
    let mut path = VectorPath::new().move_to(Vec2::new(-2.0, 1.0));
    for index in 0..map.curve_count() {
        path = path.line_to(Vec2::new(index as f32, (index % 2) as f32));
    }
    let path = Arc::new(path);
    let transform = Transform2D {
        translation: Vec2::new(2.0, -1.0),
        rotation: 0.37,
        scale: Vec2::new(2.0, 0.5),
    };
    let mut objects = vec![CompiledObject::new(
        ObjectId::new(1),
        GeometryRef::VectorPath(path.clone()),
        transform,
        Style::default(),
    )];
    objects.extend((0..static_objects).map(|index| {
        CompiledObject::new(
            ObjectId::new(index as u64 + 2),
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        )
    }));
    let definition = TrackDefinition {
        id: TrackId::new(1),
        object: ObjectId::new(1),
        property: Property::Reveal,
        values: TrackValues::PhaseReveal {
            phase_map: map.clone(),
        },
        timing,
        time_map,
    };
    let compiled = CompiledScene::compile_objects(objects, &[definition]).unwrap();
    (SceneInstance::new(compiled), path)
}

fn assert_retained_path(
    runtime: &SceneInstance,
    path: &Arc<VectorPath>,
    map: &Arc<RevealPhaseMap>,
) {
    let Some(GeometryRef::VectorPath(actual)) = runtime.frame().render_geometry(0) else {
        panic!("phase reveal must keep the original retained path")
    };
    assert!(Arc::ptr_eq(actual, path));
    assert!(Arc::ptr_eq(
        runtime
            .compiled
            .tracks_iter()
            .next()
            .unwrap()
            .values
            .reveal_phase_map()
            .unwrap(),
        map,
    ));
}

#[test]
fn phase_reveal_preserves_f64_knots_across_forward_random_reverse_and_repeated_seek() {
    let map = phase_map();
    let (mut forward, path) = fixture(
        &map,
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
        0,
    );
    let mut seek = forward.clone();
    let original_transform = forward.frame().objects[0].transform;
    let tiny_middle = 0.5 + 2.0_f64.powi(-41);
    let times = [
        0.0,
        0.07,
        0.13,
        0.5,
        tiny_middle,
        0.5 + 2.0_f64.powi(-40),
        0.73,
        0.91,
        1.0,
    ];
    for time in times {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(forward.frame().reveal(0), seek.frame().reveal(0));
        assert_eq!(forward.frame().reveal(0), map.sample(time) as f32);
        assert_eq!(forward.frame().objects[0].transform, original_transform);
        assert_retained_path(&forward, &path, &map);
        assert_retained_path(&seek, &path, &map);
    }
    // These distinct f64 phases all round to one f32 clock sample. Their reveal
    // values must still land on the separate retained-curve positions.
    for (time, expected) in [
        (0.5, 0.4),
        (tiny_middle, 0.5),
        (0.5 + 2.0_f64.powi(-40), 0.6),
    ] {
        seek.seek(time).unwrap();
        assert_eq!(seek.frame().reveal(0), expected);
    }
    for time in [
        0.91,
        0.13,
        tiny_middle,
        0.0,
        1.0,
        0.73,
        tiny_middle,
        tiny_middle,
    ] {
        seek.seek(time).unwrap();
        assert_eq!(seek.frame().reveal(0), map.sample(time) as f32);
        assert_eq!(seek.frame().objects[0].transform, original_transform);
        assert_retained_path(&seek, &path, &map);
    }
}

#[test]
fn phase_reveal_uses_nested_f64_timing_and_preserves_reversed_returning_endpoints() {
    let phases = phase_map();
    let time_map = CompositionTimeMap::from_steps(vec![
        CompositionTimeMapStep::new(0.1, 0.7, RateFunction::Smoothstep),
        CompositionTimeMapStep::new(0.2, 0.6, RateFunction::Linear),
    ]);
    for easing in [
        RateFunction::Linear,
        RateFunction::Smoothstep,
        RateFunction::ThereAndBack,
    ] {
        for reverse in [false, true] {
            let mut timing = TrackTiming::new(0.4, 2.3, easing);
            timing.reverse_rate_function = reverse;
            let (mut forward, path) = fixture(&phases, timing, time_map.clone(), 0);
            let mut seek = forward.clone();
            let initial = phases.sample(timing.evaluate_progress_f64(0.0)) as f32;
            assert_eq!(forward.frame().reveal(0), initial);
            for time in [0.0, 0.2, 0.4, 0.63, 1.0, 1.55, 2.1, 2.7, 3.1] {
                forward.advance_to(time).unwrap();
                seek.seek(time).unwrap();
                let expected = noon_core::mapped_continuous_progress_f64(timing, &time_map, time)
                    .map_or(initial, |phase| phases.sample(phase) as f32);
                assert_eq!(
                    forward.frame().reveal(0),
                    expected,
                    "{easing:?}, {reverse}, {time}"
                );
                assert_eq!(seek.frame().reveal(0), expected);
                assert_retained_path(&forward, &path, &phases);
            }
            // RevealLifecycle retains this channel's actual eased endpoint; a
            // reversed or returning reveal is not forced to full visibility.
            let terminal = phases.sample(timing.evaluate_progress_f64(1.0)) as f32;
            assert_eq!(forward.frame().reveal(0), terminal);
            for time in [1.55, 0.0, 2.1, 3.1, 0.2] {
                seek.seek(time).unwrap();
                let expected = noon_core::mapped_continuous_progress_f64(timing, &time_map, time)
                    .map_or(initial, |phase| phases.sample(phase) as f32);
                assert_eq!(seek.frame().reveal(0), expected);
                assert_retained_path(&seek, &path, &phases);
            }
        }
    }
}

#[test]
fn phase_reveal_unchanged_frames_publish_no_changes_and_active_work_stays_local() {
    let phases = phase_map();
    let (mut runtime, path) = fixture(
        &phases,
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
        2048,
    );
    runtime.advance_to(0.25).unwrap();
    runtime.take_frame_changes();
    let context = runtime.publication_context();
    runtime.advance_to(0.25).unwrap();
    // A caller forcing another active evaluation visits the one existing
    // channel, but produces no visual invalidation or publication. A paused
    // transport does not request this evaluation in the first place.
    assert_eq!(
        runtime.last_stats(),
        EvaluationStats {
            groups_evaluated: 1,
            ..EvaluationStats::default()
        }
    );
    assert!(runtime.take_frame_changes().object_indices().is_empty());
    assert_eq!(runtime.publication_context(), context);
    runtime.advance_to(0.3).unwrap();
    assert_eq!(runtime.last_stats().groups_evaluated, 1);
    assert_eq!(runtime.take_frame_changes().object_indices(), &[0]);
    assert_retained_path(&runtime, &path, &phases);
    runtime.advance_to(1.0).unwrap();
    runtime.take_frame_changes();
    runtime.advance_to(2.0).unwrap();
    assert_eq!(runtime.last_stats(), EvaluationStats::default());
    assert!(runtime.take_frame_changes().object_indices().is_empty());
    assert_eq!(runtime.frame().reveal(0), 1.0);
    assert_retained_path(&runtime, &path, &phases);
}

#[test]
fn phase_reveal_content_guard_is_target_scoped_and_honors_mapped_lifetimes() {
    let object = ObjectId::new(1);
    let unrelated = ObjectId::new(2);
    let path = VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, 0.0));
    let target = VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(0.0, 1.0));
    let geometry = GeometryRef::path(path.with_morph_target(target));
    for (property, values, geometry) in [
        (
            Property::Morph,
            TrackValues::Scalar { from: 0.0, to: 1.0 },
            geometry,
        ),
        (
            Property::Transform,
            TrackValues::Object {
                from: TransformTrackEndpoint::new(GeometryRef::circle(1.0)),
                to: TransformTrackEndpoint::new(GeometryRef::circle(2.0)),
            },
            GeometryRef::circle(1.0),
        ),
    ] {
        let objects = [object, unrelated]
            .map(|id| {
                CompiledObject::new(
                    id,
                    geometry.clone(),
                    Transform2D::IDENTITY,
                    Style::default(),
                )
            })
            .to_vec();
        for (time_map, last_live) in [
            (CompositionTimeMap::identity(), 6.0),
            (
                CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                    0.25,
                    0.5,
                    RateFunction::Linear,
                )]),
                5.0,
            ),
            (
                CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                    0.25,
                    0.5,
                    RateFunction::ThereAndBack,
                )]),
                6.0,
            ),
        ] {
            let track = TrackDefinition {
                id: TrackId::new(1),
                object,
                property,
                values: values.clone(),
                timing: TrackTiming::new(2.0, 4.0, RateFunction::Linear),
                time_map,
            };
            let mut runtime = SceneInstance::new(
                CompiledScene::compile_objects(objects.clone(), &[track]).unwrap(),
            );
            for time in [0.0, 2.0, 3.5, last_live] {
                assert!(runtime.object_has_current_or_future_content_track(object, time));
                assert!(!runtime.object_has_current_or_future_content_track(unrelated, time));
            }
            assert_eq!(
                runtime.object_has_current_or_future_content_track(object, last_live + 0.125),
                property == Property::Transform,
            );
            assert!(runtime.object_has_current_or_future_content_track(object, f64::NAN));
            let reconciliation = ExecutionPatch::ReconcileTrack {
                track: TrackId::new(1),
                object,
                property,
                end_time: 6.0,
            };
            if property == Property::Morph {
                // A reconciled Morph has ceded ownership to authored content,
                // while its historical track remains retained for seek.
                runtime.apply_execution_patch(&reconciliation).unwrap();
                assert!(!runtime.object_has_current_or_future_content_track(object, 6.0));
                assert!(runtime.object_has_current_or_future_content_track(object, 3.5));
            } else {
                // An opaque Transform directly replaces effective content, so
                // its held endpoint need not appear in any override array.
                runtime.seek(7.0).unwrap();
                assert_eq!(
                    runtime.frame().objects[0].geometry(),
                    Some(&GeometryRef::circle(2.0))
                );
                assert!(runtime.frame().render_geometries[0].is_none());
                assert!(runtime.frame().render_transforms[0].is_none());
                assert_eq!(runtime.frame().morph(0), 0.0);
                assert!(runtime.frame().family_animations[0].is_none());
                assert!(runtime.object_has_current_or_future_content_track(object, 7.0));
                let frame = runtime.frame().clone();
                let publication = runtime.publication_context();
                assert!(matches!(
                    runtime.apply_execution_patch(&reconciliation),
                    Err(CompilePatchError::UnsupportedTrackReconciliation(id)) if id == TrackId::new(1)
                ));
                assert_eq!(runtime.frame(), &frame);
                assert_eq!(runtime.publication_context(), publication);
                runtime
                    .apply_execution_patch(&ExecutionPatch::RemoveTrack(TrackId::new(1)))
                    .unwrap();
                assert!(!runtime.object_has_current_or_future_content_track(object, 7.0));
            }
        }
    }
}
