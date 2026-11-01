use std::sync::Arc;

use noon_core::ObjectContentRef;

use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, Color, ExecutionSegment,
    ExecutionSession, GeometryRef, Mobject, RateFunction, RevealPhaseMap, SampledMotionData, Scene,
    SemanticAnimationCompositionKind as Kind, SemanticPaint, SemanticRotation3D, SemanticStyle,
    SemanticVec3, TransformToRequest, Vec2, VectorPath,
};

const KEYS: [f64; 4] = [0.0, 0.17, 0.61, 1.0];
const POINTS: [(f64, f64); 4] = [(-2.0, 0.0), (-1.0, 1.0), (2.0, -1.0), (3.0, 0.5)];

fn path() -> VectorPath {
    let mut path = VectorPath::new().move_to(Vec2::new(-2.0, 0.0));
    for &(x, y) in &POINTS[1..] {
        path = path.line_to(Vec2::new(x as f32, y as f32));
    }
    path
}

fn path_object(scene: &mut Scene, path: VectorPath) -> Mobject {
    scene
        .path(
            path,
            SemanticStyle {
                fill: None,
                stroke: Some(SemanticPaint::Solid(Color::BLUE)),
                stroke_width: 0.04,
                ..SemanticStyle::default()
            },
        )
        .unwrap()
}

fn map() -> Arc<RevealPhaseMap> {
    Arc::new(RevealPhaseMap::new(KEYS.to_vec()).unwrap())
}

fn linear(duration: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Linear)
}

fn index(session: &ExecutionSession, target: &Mobject) -> usize {
    let id = session.execution_object_id(target.node_id()).unwrap();
    session
        .frame()
        .objects
        .iter()
        .position(|row| row.id == id)
        .unwrap()
}

fn geometry(session: &ExecutionSession, index: usize) -> Arc<VectorPath> {
    let Some(GeometryRef::VectorPath(path)) = session.frame().render_geometry(index) else {
        panic!("phase reveal must keep its retained vector path");
    };
    path.clone()
}

fn expected(phase: f64) -> (f64, Vec2) {
    let upper = KEYS
        .partition_point(|key| *key <= phase)
        .clamp(1, KEYS.len() - 1);
    let lower = upper - 1;
    let fraction = (phase - KEYS[lower]) / (KEYS[upper] - KEYS[lower]);
    (
        (lower as f64 + fraction) / (KEYS.len() - 1) as f64,
        Vec2::new(
            (POINTS[lower].0 + (POINTS[upper].0 - POINTS[lower].0) * fraction) as f32,
            (POINTS[lower].1 + (POINTS[upper].1 - POINTS[lower].1) * fraction) as f32,
        ),
    )
}

fn assert_phase(session: &ExecutionSession, index: usize, phase: f64) {
    let (reveal, endpoint) = expected(phase);
    let actual = session.frame().reveal(index);
    assert!(
        (f64::from(actual) - reveal).abs() <= 2e-7,
        "{actual} != {reveal}"
    );
    if phase > 0.0 {
        let partial = noon_geometry::authored_partial_path(&geometry(session, index), 0.0, actual);
        let (start, end) = noon_geometry::drawable_endpoints(&partial).unwrap();
        assert_eq!(start, Vec2::new(-2.0, 0.0));
        assert!((end - endpoint).length() <= 2e-5, "{end:?} != {endpoint:?}");
    }
}

fn finish(scene: &mut Scene, session: &mut ExecutionSession, segment: ExecutionSegment) {
    let mut live = scene.live(session);
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
}

fn reject_atomically(
    scene: &mut Scene,
    session: &mut ExecutionSession,
    request: &Request<'_>,
    context: &str,
) -> String {
    session.take_frame_changes();
    let publication = session.publication_context();
    let frame = session.frame().clone();
    let nodes = scene.integration_store().borrow().len();
    let result = scene
        .live(session)
        .declare_and_activate_composition(request, AnimationOptions::new());
    let error = match result {
        Ok(segment) => panic!("{context}: unexpectedly admitted {segment:?}"),
        Err(error) => error,
    };
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame(), &frame);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert!(session.take_frame_changes().is_empty());
    error.to_string()
}

#[test]
fn phase_reveal_introduces_a_detached_path_and_keeps_its_resource_after_completion_and_seek() {
    let mut scene = Scene::new();
    let target = path_object(&mut scene, path());
    let authored = target.state().unwrap();
    let mut session = scene.execution_session().unwrap();
    assert!(session.frame().objects.is_empty());
    session.begin_replay_retention(Default::default()).unwrap();
    let hold = scene.live(&mut session).wait_segment(0.11).unwrap();
    finish(&mut scene, &mut session, hold);
    let before = session.publication_context();
    assert!(!scene.live(&mut session).contains(&target).unwrap());
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(
            &Request::PhaseReveal {
                target: &target,
                phase_map: map(),
                options: linear(0.7),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    assert!(scene.live(&mut session).contains(&target).unwrap());
    assert_eq!(session.frame().objects.len(), 1);
    assert_eq!(
        session.publication_context().scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    let row = index(&session, &target);
    let retained = geometry(&session, row);
    assert_phase(&session, row, 0.0);
    for phase in [0.17, 0.39, 0.61, 0.83, 1.0] {
        scene
            .live(&mut session)
            .advance_segment_to(segment, segment.start_time() + phase * 0.7)
            .unwrap();
        assert_phase(&session, row, phase);
        assert!(Arc::ptr_eq(&retained, &geometry(&session, row)));
    }
    finish(&mut scene, &mut session, segment);
    assert_eq!(session.frame().reveal(row), 1.0);
    assert!(scene.live(&mut session).contains(&target).unwrap());
    assert_eq!(target.state().unwrap(), authored);
    let hold = scene.live(&mut session).wait_segment(0.23).unwrap();
    finish(&mut scene, &mut session, hold);
    session.seal_replay().unwrap();
    for (time, phase) in [
        (segment.start_time() + 0.35, Some(0.5)),
        (hold.end_time(), Some(1.0)),
        (0.0, None),
        (segment.start_time() + 0.119, Some(0.17)),
        (segment.end_time(), Some(1.0)),
        (segment.start_time() + 0.35, Some(0.5)),
    ] {
        session.seek(time).unwrap();
        assert_eq!(session.frame().is_present(row), phase.is_some());
        if let Some(phase) = phase {
            assert_phase(&session, row, phase);
            assert!(Arc::ptr_eq(&retained, &geometry(&session, row)));
        }
    }
}

#[test]
fn phase_reveal_reverses_and_removes_an_existing_member_without_losing_retained_history() {
    let mut scene = Scene::new();
    let target = path_object(&mut scene, path());
    scene.add(&target).unwrap();
    let identity = target.node_id();
    let authored = target.state().unwrap();
    let mut session = scene.execution_session().unwrap();
    session.begin_replay_retention(Default::default()).unwrap();
    let row = index(&session, &target);
    let retained = geometry(&session, row);
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(
            &Request::PhaseReveal {
                target: &target,
                phase_map: map(),
                options: linear(0.7).reverse_rate_function(true).remover(true),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    assert_phase(&session, row, 1.0);
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.35)
        .unwrap();
    assert_phase(&session, row, 0.5);
    scene
        .live(&mut session)
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    assert_eq!(session.frame().reveal(row), 0.0);
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert!(!scene.live(&mut session).contains(&target).unwrap());
    assert!(!session.frame().is_present(row));
    assert_eq!(target.node_id(), identity);
    assert_eq!(target.state().unwrap(), authored);
    let hold = scene.live(&mut session).wait_segment(0.23).unwrap();
    finish(&mut scene, &mut session, hold);
    session.seal_replay().unwrap();
    for (time, phase) in [
        (0.35, Some(0.5)),
        (hold.end_time(), None),
        (0.0, Some(1.0)),
        (0.7, None),
        (0.35, Some(0.5)),
    ] {
        session.seek(time).unwrap();
        assert_eq!(session.frame().is_present(row), phase.is_some());
        if let Some(phase) = phase {
            assert_phase(&session, row, phase);
            assert!(Arc::ptr_eq(&retained, &geometry(&session, row)));
        }
    }
}

#[test]
fn phase_reveal_asymmetric_reversal_is_applied_before_mapping_and_matches_a_sampled_pen() {
    let mut scene = Scene::new();
    let trace = path_object(&mut scene, path());
    let mut pen = scene.circle(0.05).unwrap();
    pen.set_translation(POINTS[0].0, POINTS[0].1).unwrap();
    scene.add(&trace).unwrap();
    scene.add(&pen).unwrap();
    let mut session = scene.execution_session().unwrap();
    let options = AnimationOptions::new()
        .run_time(1.3)
        .rate_func(RateFunction::EaseInQuad)
        .reverse_rate_function(true);
    let request = Request::Composition {
        kind: Kind::Parallel,
        children: vec![
            Request::PhaseReveal {
                target: &trace,
                phase_map: map(),
                options,
            },
            Request::SampledMotion {
                target: &pen,
                motion: Arc::new(
                    SampledMotionData::new(
                        KEYS[1..].to_vec(),
                        POINTS[1..]
                            .iter()
                            .map(|&(x, y)| SemanticVec3::new(x, y, 0.0))
                            .collect(),
                        vec![0.0; 3],
                    )
                    .unwrap(),
                ),
                options,
            },
        ],
        options: AnimationOptions::new(),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    let trace_row = index(&session, &trace);
    let pen_row = index(&session, &pen);
    for elapsed in [0.0_f64, 0.11, 0.25, 0.61, 0.87, 1.0] {
        scene
            .live(&mut session)
            .advance_segment_to(segment, elapsed * 1.3)
            .unwrap();
        let phase = (1.0 - elapsed).powi(2);
        assert_phase(&session, trace_row, phase);
        assert!(
            (session.frame().objects[pen_row].transform.translation - expected(phase).1).length()
                <= 2e-5
        );
    }
    finish(&mut scene, &mut session, segment);
    assert_eq!(session.frame().reveal(trace_row), 0.0);
    assert_eq!(
        session.frame().objects[pen_row].transform.translation,
        Vec2::new(-2.0, 0.0)
    );
}

#[test]
fn phase_reveal_returning_rate_keeps_the_empty_terminal_prefix_through_the_following_wait() {
    let mut scene = Scene::new();
    let target = path_object(&mut scene, path());
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(
            &Request::PhaseReveal {
                target: &target,
                phase_map: map(),
                options: AnimationOptions::new()
                    .run_time(1.3)
                    .rate_func(RateFunction::ThereAndBack),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let row = index(&session, &target);
    let retained = geometry(&session, row);
    assert_phase(&session, row, 0.0);
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.65)
        .unwrap();
    assert_phase(&session, row, 1.0);
    finish(&mut scene, &mut session, segment);
    assert!(scene.live(&mut session).contains(&target).unwrap());
    assert_eq!(session.frame().reveal(row), 0.0);
    let hold = scene.live(&mut session).wait_segment(0.23).unwrap();
    finish(&mut scene, &mut session, hold);
    assert_eq!(session.frame().reveal(row), 0.0);
    assert!(Arc::ptr_eq(&retained, &geometry(&session, row)));
}

#[test]
fn phase_reveal_nested_non_dyadic_sequences_honor_play_duration_and_outer_rate() {
    for rate in [RateFunction::Linear, RateFunction::Smoothstep] {
        let mut scene = Scene::new();
        let first = path_object(&mut scene, path());
        let second = path_object(&mut scene, path());
        scene.add(&first).unwrap();
        scene.add(&second).unwrap();
        let mut session = scene.execution_session().unwrap();
        let request = Request::Composition {
            kind: Kind::Sequence,
            children: vec![
                Request::Composition {
                    kind: Kind::Sequence,
                    children: vec![
                        Request::PhaseReveal {
                            target: &first,
                            phase_map: map(),
                            options: linear(0.11),
                        },
                        Request::PhaseReveal {
                            target: &second,
                            phase_map: map(),
                            options: linear(0.23),
                        },
                    ],
                    options: AnimationOptions::new().rate_func(RateFunction::Linear),
                },
                Request::Wait { duration: 0.17 },
            ],
            options: AnimationOptions::new().run_time(0.93).rate_func(rate),
        };
        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(&request, AnimationOptions::new().run_time(1.31))
            .unwrap();
        assert_eq!(segment.start_time(), 0.0);
        assert_eq!(segment.end_time(), 1.31);
        let rows = [index(&session, &first), index(&session, &second)];
        let inverse = |phase: f64| {
            if rate == RateFunction::Linear {
                phase
            } else {
                0.5 - ((1.0 - 2.0 * phase).asin() / 3.0).sin()
            }
        };
        let samples = [
            0.0,
            inverse(0.11 / 0.51),
            0.29,
            inverse(0.34 / 0.51),
            0.81,
            1.0,
        ];
        let assert_at = |session: &ExecutionSession, elapsed: f64| {
            let eased = if rate == RateFunction::Linear {
                elapsed
            } else {
                elapsed * elapsed * (3.0 - 2.0 * elapsed)
            };
            let time = eased * 0.51;
            assert_phase(session, rows[0], (time / 0.11).clamp(0.0, 1.0));
            assert_phase(session, rows[1], ((time - 0.11) / 0.23).clamp(0.0, 1.0));
        };
        let mut forward_samples = samples;
        forward_samples.sort_by(f64::total_cmp);
        for elapsed in forward_samples {
            session.advance_to(elapsed * 1.31).unwrap();
            assert_at(&session, elapsed);
        }
        for elapsed in samples.into_iter().rev() {
            session.seek(elapsed * 1.31).unwrap();
            assert_at(&session, elapsed);
        }
        session.seek(segment.end_time()).unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(session.frame().reveal(rows[0]), 1.0);
        assert_eq!(session.frame().reveal(rows[1]), 1.0);
    }
}

#[test]
fn phase_reveal_malformed_target_or_curve_count_rejects_an_entire_mixed_candidate() {
    for mode in ["curve-count", "analytic", "multi-contour", "spatial"] {
        for invalid_first in [false, true] {
            let mut scene = Scene::new();
            let valid = path_object(&mut scene, path());
            let invalid = match mode {
                "analytic" => scene.circle(0.5).unwrap(),
                "multi-contour" => path_object(
                    &mut scene,
                    VectorPath::new()
                        .move_to(Vec2::ZERO)
                        .line_to(Vec2::new(1.0, 0.0))
                        .move_to(Vec2::new(2.0, 0.0))
                        .line_to(Vec2::new(3.0, 0.0)),
                ),
                "spatial" => {
                    let mut object = path_object(&mut scene, path());
                    // A depth-only shift deliberately preserves planar orientation.
                    // Tilt the retained path out of plane to exercise this profile gate.
                    let mut world = object.world_transform().unwrap();
                    world.rotation =
                        SemanticRotation3D::from_axis_angle(SemanticVec3::new(1.0, 0.0, 0.0), 0.4)
                            .unwrap();
                    object.set_world_transform(world).unwrap();
                    assert!(matches!(
                        object.state().unwrap().transform.orientation,
                        noon_core::SemanticOrientation::Spatial(_)
                    ));
                    assert!(object
                        .state()
                        .unwrap()
                        .transform
                        .planar_rotation()
                        .is_none());
                    object
                }
                _ => path_object(&mut scene, path()),
            };
            let invalid_map = if mode == "spatial" {
                map()
            } else {
                Arc::new(RevealPhaseMap::new(vec![0.0, 0.4, 1.0]).unwrap())
            };
            let mut children = vec![
                Request::PhaseReveal {
                    target: &valid,
                    phase_map: map(),
                    options: linear(0.7),
                },
                Request::PhaseReveal {
                    target: &invalid,
                    phase_map: invalid_map,
                    options: linear(0.7),
                },
            ];
            if invalid_first {
                children.reverse();
            }
            let request = Request::Composition {
                kind: Kind::Parallel,
                children,
                options: AnimationOptions::new(),
            };
            let mut session = scene.execution_session().unwrap();
            reject_atomically(
                &mut scene,
                &mut session,
                &request,
                &format!("{mode}, invalid_first={invalid_first}"),
            );
            assert!(!scene.live(&mut session).contains(&valid).unwrap());
            assert!(!scene.live(&mut session).contains(&invalid).unwrap());
            assert!(session.frame().objects.is_empty());
        }
    }
}

#[test]
fn phase_reveal_rejects_duplicate_reveal_and_content_changing_compositions_atomically() {
    for (kind, content_change) in [
        (Kind::Parallel, false),
        (Kind::Parallel, true),
        (Kind::Sequence, true),
    ] {
        let mut scene = Scene::new();
        let target = path_object(&mut scene, path());
        scene.add(&target).unwrap();
        let mut destination = target.target_editor().unwrap();
        destination
            .set_points_as_corners(&[
                Vec2::new(-2.0, 0.0),
                Vec2::new(0.0, 2.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(3.0, 1.0),
            ])
            .unwrap();
        let second = if content_change {
            Request::TransformTo(TransformToRequest::point_correspondence(
                &target,
                &destination,
                linear(0.7),
            ))
        } else {
            Request::PhaseReveal {
                target: &target,
                phase_map: map(),
                options: linear(0.7),
            }
        };
        let request = Request::Composition {
            kind,
            children: vec![
                Request::PhaseReveal {
                    target: &target,
                    phase_map: map(),
                    options: linear(0.7),
                },
                second,
            ],
            options: AnimationOptions::new(),
        };
        let mut session = scene.execution_session().unwrap();
        let before = target.state().unwrap();
        reject_atomically(
            &mut scene,
            &mut session,
            &request,
            &format!("kind={kind:?}, content_change={content_change}"),
        );
        assert_eq!(target.state().unwrap(), before);
        assert!(scene.live(&mut session).contains(&target).unwrap());
    }
}

#[test]
fn phase_reveal_rejects_an_active_effective_content_lease_without_stealing_it() {
    let mut scene = Scene::new();
    let target = path_object(&mut scene, path());
    scene.add(&target).unwrap();
    let mut session = scene.execution_session().unwrap();
    let authored = target.state().unwrap();
    let replacement = session
        .prepare_effective_content_replacement(
            target.node_id(),
            ObjectContentRef::Geometry(GeometryRef::path(path())),
            None,
            None,
        )
        .unwrap();
    let lease = session
        .commit_effective_content_replacement(replacement)
        .unwrap();
    let request = Request::PhaseReveal {
        target: &target,
        phase_map: map(),
        options: linear(0.7),
    };
    let error = reject_atomically(
        &mut scene,
        &mut session,
        &request,
        "active effective-content lease",
    );
    assert!(error.contains("effective content"), "{error}");
    assert_eq!(
        session.effective_content_lease(target.node_id()),
        Some(lease)
    );
    assert_eq!(target.state().unwrap(), authored);
    session.release_effective_content(lease).unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    finish(&mut scene, &mut session, segment);
    assert_eq!(session.frame().reveal(index(&session, &target)), 1.0);
}

#[test]
fn phase_reveal_existing_declarations_require_a_direct_member_and_cannot_remove_it() {
    for (present, remove) in [(true, false), (false, false), (true, true)] {
        let mut scene = Scene::new();
        let target = path_object(&mut scene, path());
        if present {
            scene.add(&target).unwrap();
        }
        let declaration = scene
            .declare_phase_reveal(&target, map(), linear(0.7).remover(remove))
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let frame = session.frame().clone();
        let nodes = scene.integration_store().borrow().len();
        let result = scene.live(&mut session).play_animation(&declaration);
        if present && !remove {
            let segment = result.unwrap();
            let row = index(&session, &target);
            scene
                .live(&mut session)
                .advance_segment_to(segment, 0.35)
                .unwrap();
            assert_phase(&session, row, 0.5);
            finish(&mut scene, &mut session, segment);
            assert_eq!(session.frame().reveal(row), 1.0);
            assert!(scene.live(&mut session).contains(&target).unwrap());
        } else {
            assert!(result.is_err());
            assert_eq!(session.publication_context(), before);
            assert_eq!(session.frame(), &frame);
            assert_eq!(scene.integration_store().borrow().len(), nodes);
        }
    }
}
