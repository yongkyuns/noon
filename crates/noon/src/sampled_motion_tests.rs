use std::sync::Arc;

use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, RateFunction, SampledMotionData,
    Scene, SemanticAnimationCompositionKind as Kind, SemanticVec3, TransformToRequest,
};

fn motion(phases: &[f64], centers: &[(f64, f64)], angles: &[f64]) -> Arc<SampledMotionData> {
    Arc::new(
        SampledMotionData::new(
            phases.to_vec(),
            centers
                .iter()
                .map(|&(x, y)| SemanticVec3::new(x, y, 0.0))
                .collect(),
            angles.to_vec(),
        )
        .unwrap(),
    )
}

fn linear(duration: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Linear)
}

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 8e-6, "{actual} != {expected}");
}

#[test]
fn sampled_motion_captures_live_pose_and_moves_an_off_origin_rigid_anchor() {
    let mut scene = Scene::new();
    let target = scene.line((1.0, -0.5), (3.0, 1.5)).unwrap();
    scene.add(&target).unwrap();
    let data = motion(
        &[0.37, 1.0],
        &[(4.0, -1.0), (-2.0, 3.0)],
        &[-1.48 * std::f64::consts::PI, -4.0 * std::f64::consts::PI],
    );
    let declaration = scene
        .declare_sampled_motion(&target, data, linear(1.7))
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    // Edits after declaration must determine the implicit phase-zero pose.
    live.set_scale(&target, 1.7, 0.8).unwrap();
    live.set_rotation(&target, -0.45).unwrap();
    live.shift(&target, -1.0, 2.0).unwrap();
    let source = live.effective(&target).unwrap();
    let center = live.effective_layout(&target).unwrap().center;
    let endpoints = live.effective_line_endpoints(&target).unwrap();
    let segment = live.play_animation(&declaration).unwrap();
    assert_eq!(live.effective(&target).unwrap().transform, source.transform);
    let authored = live.authored(&target).unwrap().transform;
    for phase in [0.0, 0.185, 0.37, 0.685, 1.0] {
        live.advance_segment_to(segment, segment.start_time() + phase * 1.7)
            .unwrap();
        let (from, to, fraction) = if phase <= 0.37 {
            (center, (4.0, -1.0), phase / 0.37)
        } else {
            ((4.0, -1.0), (-2.0, 3.0), (phase - 0.37) / 0.63)
        };
        let expected_center = (
            from.0 + (to.0 - from.0) * fraction,
            from.1 + (to.1 - from.1) * fraction,
        );
        let angle = -4.0 * std::f64::consts::PI * phase;
        let (sin, cos) = angle.sin_cos();
        let actual = live.effective_line_endpoints(&target).unwrap();
        for (original, actual) in [(endpoints.start, actual.start), (endpoints.end, actual.end)] {
            let x = original.0 - center.0;
            let y = original.1 - center.1;
            near(actual.0, expected_center.0 + cos * x - sin * y);
            near(actual.1, expected_center.1 + sin * x + cos * y);
        }
        let effective = live.effective(&target).unwrap();
        near(
            f64::from(effective.transform.rotation),
            f64::from(source.transform.rotation) + angle,
        );
        assert_eq!(effective.transform.scale, source.transform.scale);
        assert_eq!(live.authored(&target).unwrap().transform, authored);
    }
    let final_pose = live.effective(&target).unwrap().transform;
    live.complete_segment(segment).unwrap();
    let persisted = live.authored(&target).unwrap().transform;
    near(persisted.translation.x, f64::from(final_pose.translation.x));
    near(persisted.translation.y, f64::from(final_pose.translation.y));
    near(
        persisted.planar_rotation().unwrap(),
        f64::from(final_pose.rotation),
    );
    let wait = live.wait_segment(0.23).unwrap();
    live.advance_segment_to(wait, wait.end_time()).unwrap();
    assert_eq!(live.effective(&target).unwrap().transform, final_pose);
}

#[test]
fn sampled_motion_composes_adjacent_non_dyadic_intervals_with_outer_scaling_and_rates() {
    for rate in [RateFunction::Linear, RateFunction::Smoothstep] {
        let mut scene = Scene::new();
        let mut target = scene.square(1.0).unwrap();
        target.set_rotation(0.3).unwrap();
        scene.add(&target).unwrap();
        let mut destination = target.target_editor().unwrap();
        destination.set_translation(3.0, 2.0).unwrap();
        destination.set_rotation(0.7).unwrap();
        let request = Request::Composition {
            kind: Kind::Sequence,
            children: vec![
                Request::Composition {
                    kind: Kind::Sequence,
                    children: vec![
                        Request::SampledMotion {
                            target: &target,
                            motion: motion(&[1.0], &[(1.0, 0.0)], &[2.0 * std::f64::consts::PI]),
                            options: linear(0.11),
                        },
                        Request::SampledMotion {
                            target: &target,
                            motion: motion(&[1.0], &[(1.0, 2.0)], &[-4.0 * std::f64::consts::PI]),
                            options: linear(0.23),
                        },
                    ],
                    options: AnimationOptions::new().rate_func(RateFunction::Linear),
                },
                Request::TransformTo(TransformToRequest::new(&target, &destination, linear(0.17))),
            ],
            options: AnimationOptions::new().run_time(0.93).rate_func(rate),
        };
        let mut session = scene.execution_session().unwrap();
        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();
        near(segment.end_time(), 0.93);
        let sample = |u: f64| {
            let alpha = if rate == RateFunction::Linear {
                u
            } else {
                u * u * (3.0 - 2.0 * u)
            };
            let authored_time = alpha * 0.51;
            let (x, y, angle) = if authored_time <= 0.11 {
                let p = authored_time / 0.11;
                (p, 0.0, 0.3 + 2.0 * std::f64::consts::PI * p)
            } else if authored_time <= 0.34 {
                let p = (authored_time - 0.11) / 0.23;
                (
                    1.0,
                    2.0 * p,
                    0.3 + 2.0 * std::f64::consts::PI - 4.0 * std::f64::consts::PI * p,
                )
            } else {
                let p = (authored_time - 0.34) / 0.17;
                let start_angle = 0.3 - 2.0 * std::f64::consts::PI;
                (1.0 + 2.0 * p, 2.0, start_angle + (0.7 - start_angle) * p)
            };
            (x, y, angle)
        };
        for u in [0.0, 0.11 / 0.51, 0.29, 0.34 / 0.51, 0.81, 1.0] {
            session.advance_to(u * 0.93).unwrap();
            let pose = scene
                .live(&mut session)
                .effective(&target)
                .unwrap()
                .transform;
            let (x, y, angle) = sample(u);
            near(f64::from(pose.translation.x), x);
            near(f64::from(pose.translation.y), y);
            near(f64::from(pose.rotation), angle);
        }
        for u in [0.81, 0.11 / 0.51, 0.0, 0.34 / 0.51, 0.29] {
            session.seek(u * 0.93).unwrap();
            let pose = scene
                .live(&mut session)
                .effective(&target)
                .unwrap()
                .transform;
            let (x, y, angle) = sample(u);
            near(f64::from(pose.translation.x), x);
            near(f64::from(pose.translation.y), y);
            near(f64::from(pose.rotation), angle);
        }
    }
}

#[test]
fn sampled_motion_conflicts_reject_the_complete_composition_without_publication() {
    let mut scene = Scene::new();
    let target = scene.square(1.0).unwrap();
    scene.add(&target).unwrap();
    let mut session = scene.execution_session().unwrap();
    let motion = motion(&[1.0], &[(1.0, 2.0)], &[4.0]);
    let before = session.publication_context();
    let nodes = scene.integration_store().borrow().len();
    let request = Request::Composition {
        kind: Kind::Parallel,
        children: (0..2)
            .map(|_| Request::SampledMotion {
                target: &target,
                motion: Arc::clone(&motion),
                options: linear(1.0),
            })
            .collect(),
        options: AnimationOptions::new(),
    };
    let error = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap_err();
    assert!(
        error.to_string().to_ascii_lowercase().contains("driver"),
        "{error}"
    );
    assert_eq!(session.publication_context(), before);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(
        session.frame().objects[0].transform,
        crate::Transform2D::IDENTITY
    );
}

#[test]
fn sampled_motion_many_keys_add_no_target_objects_and_only_dirty_the_driven_row() {
    let mut scene = Scene::new();
    let target = scene.square(0.5).unwrap();
    scene.add(&target).unwrap();
    for index in 0..128 {
        let mut still = scene.circle(0.1).unwrap();
        still.set_translation(index as f64, -3.0).unwrap();
        scene.add(&still).unwrap();
    }
    let phases = (1..=4096)
        .map(|index| f64::from(index) / 4096.0)
        .collect::<Vec<_>>();
    let data = Arc::new(
        SampledMotionData::new(
            phases.clone(),
            phases
                .iter()
                .map(|&p| SemanticVec3::new(p * 2.0, p, 0.0))
                .collect(),
            phases
                .iter()
                .map(|&p| p * -8.0 * std::f64::consts::PI)
                .collect(),
        )
        .unwrap(),
    );
    let mut session = scene.execution_session().unwrap();
    let nodes = scene.integration_store().borrow().len();
    let request = Request::SampledMotion {
        target: &target,
        motion: data,
        options: linear(2.0),
    };
    scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    assert!(scene.integration_store().borrow().len() <= nodes + 2);
    assert_eq!(session.frame().objects.len(), 129);
    let patch = session.last_patch_stats();
    assert_eq!(patch.object_slots_appended, 0);
    assert_eq!(patch.full_group_rebuilds, 0);
    assert_eq!(patch.full_seeks, 0);
    let revision = session.publication_context().scene_revision();
    session.take_frame_changes();
    session.advance_to(0.7).unwrap();
    assert_eq!(session.take_frame_changes().object_indices(), &[0]);
    assert_eq!(session.publication_context().scene_revision(), revision);
    session.advance_to(0.7).unwrap();
    assert!(session.take_frame_changes().object_indices().is_empty());
}

#[test]
fn sampled_motion_reverse_and_returning_rates_persist_the_captured_source_endpoint() {
    for (rate, reverse) in [
        (RateFunction::Linear, true),
        (RateFunction::ThereAndBack, false),
    ] {
        let mut scene = Scene::new();
        let mut target = scene.square(0.5).unwrap();
        target.set_translation(-1.0, 0.0).unwrap();
        target.set_rotation(0.2).unwrap();
        scene.add(&target).unwrap();
        let mut session = scene.execution_session().unwrap();
        let source = session.frame().objects[0].transform;
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_sampled_motion(
                &target,
                motion(&[1.0], &[(5.0, 1.0)], &[6.0 * std::f64::consts::PI]),
                AnimationOptions::new()
                    .run_time(1.3)
                    .rate_func(rate)
                    .reverse_rate_function(reverse),
            )
            .unwrap();
        let start = live.effective(&target).unwrap().transform;
        near(
            f64::from(start.translation.x),
            if reverse { 5.0 } else { -1.0 },
        );
        live.advance_segment_to(segment, 0.65).unwrap();
        let middle = live.effective(&target).unwrap().transform;
        near(
            f64::from(middle.translation.x),
            if reverse { 2.0 } else { 5.0 },
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        assert_eq!(live.effective(&target).unwrap().transform, source);
        live.complete_segment(segment).unwrap();
        assert_eq!(live.effective(&target).unwrap().transform, source);
        let authored = live.authored(&target).unwrap().transform;
        near(authored.translation.x, -1.0);
        near(
            authored.planar_rotation().unwrap(),
            f64::from(source.rotation),
        );
    }
}

#[cfg(feature = "prepared-prefix")]
#[test]
fn sampled_motion_prepared_history_admits_whole_tables_and_restores_both_channels() {
    let mut scene = Scene::new();
    let target = scene.square(0.5).unwrap();
    scene.add(&target).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(noon_runtime::ReplayLimits {
            revisions: 128,
            payloads: 8192,
        })
        .unwrap();
    let first = {
        let mut live = scene.live(&mut source);
        let segment = live
            .declare_and_activate_sampled_motion(
                &target,
                motion(
                    &[0.2, 1.0],
                    &[(0.3, 1.0), (2.0, 0.0)],
                    &[-0.6 * std::f64::consts::PI, -3.0 * std::f64::consts::PI],
                ),
                linear(0.7),
            )
            .unwrap();
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        segment
    };
    let (mut publisher, mut reader) = source.create_prepared_prefix(first).unwrap();
    reader.seek(0.35).unwrap();
    let before = reader.frame().objects[0].transform;
    let bytes = reader.retained_resource_bytes();
    assert!(reader.seek(0.85).is_err());
    assert_eq!(reader.frame().time, 0.35);
    assert_eq!(reader.frame().objects[0].transform, before);

    let second = {
        let mut live = scene.live(&mut source);
        let segment = live
            .declare_and_activate_sampled_motion(
                &target,
                motion(&[1.0], &[(-1.0, 1.0)], &[5.0 * std::f64::consts::PI]),
                linear(0.3),
            )
            .unwrap();
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        segment
    };
    source.seal_replay().unwrap();
    let batch = source
        .finish_prepared_prefix(second, &mut publisher)
        .unwrap();
    reader.preflight_append(&batch).unwrap();
    reader.append(batch).unwrap();
    assert_eq!(reader.frame().time, 0.35);
    assert_eq!(reader.frame().objects[0].transform, before);
    assert!(reader.retained_resource_bytes() > bytes);
    for time in [
        0.0, 0.14, 0.35, 0.699999, 0.7, 0.85, 1.0, 0.35, 0.0, 0.85, 0.14, 1.0,
    ] {
        reader.seek(time).unwrap();
        let (x, y, angle) = if time < 0.7 {
            let phase = time / 0.7;
            let (x, y) = if phase <= 0.2 {
                (0.3 * phase / 0.2, phase / 0.2)
            } else {
                let p = (phase - 0.2) / 0.8;
                (0.3 + 1.7 * p, 1.0 - p)
            };
            (x, y, -3.0 * std::f64::consts::PI * phase)
        } else {
            let p = (time - 0.7) / 0.3;
            (
                2.0 - 3.0 * p,
                p,
                -3.0 * std::f64::consts::PI + 5.0 * std::f64::consts::PI * p,
            )
        };
        let pose = reader.frame().objects[0].transform;
        near(f64::from(pose.translation.x), x);
        near(f64::from(pose.translation.y), y);
        near(f64::from(pose.rotation), angle);
    }
    assert_eq!(source.frame().time, second.end_time());
}
