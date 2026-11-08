use super::*;
use crate::{AnimationCompositionRequest, ManimRotationPivot, RateFunction, Scene};
use noon_core::{SemanticMutationTransaction, RIGHT, UP};

fn linear() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}

fn finish(live: &mut LiveSession<'_>, segment: ExecutionSegment) {
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
}

#[test]
fn owned_builders_are_inert_and_retained_edits_capture_at_play_admission() {
    let mut scene = Scene::new();
    let square = scene.square(2.0).unwrap();
    scene.add(&square).unwrap();
    let before = square.state().unwrap();
    let before_revision = scene.revision();
    let before_nodes = scene.integration_store().borrow().len();
    let retained = square.animate().shift(RIGHT).set_fill(Color::BLUE);
    let retained_clone = retained.clone();
    assert_eq!(square.state().unwrap(), before);
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(scene.integration_store().borrow().len(), before_nodes);

    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let first = activate(
        &mut live,
        square.animate().shift(RIGHT * 2.0).into(),
        linear(),
    )
    .unwrap();
    finish(&mut live, first);

    let second = activate(&mut live, retained.into(), linear()).unwrap();
    live.advance_segment_to(second, second.start_time() + 0.5)
        .unwrap();
    assert_eq!(
        live.effective(&square).unwrap().transform.translation,
        RIGHT * 2.5
    );
    finish(&mut live, second);
    assert_eq!(
        live.effective(&square).unwrap().transform.translation,
        RIGHT * 3.0
    );
    assert_eq!(
        live.effective(&square).unwrap().style.fill,
        Some(Color::BLUE)
    );

    // Cloning a request preserves the recipe, not an earlier captured target.
    let third = activate(&mut live, retained_clone.into(), linear()).unwrap();
    finish(&mut live, third);
    assert_eq!(
        live.effective(&square).unwrap().transform.translation,
        RIGHT * 4.0
    );
}

#[test]
fn heterogeneous_tuple_and_array_use_canonical_admission_and_fade_completion() {
    let mut scene = Scene::new();
    let square = scene.square(1.0).unwrap();
    let circle = scene.circle(0.5).unwrap();
    let square_id = square.node_id();
    let circle_id = circle.node_id();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let entering = activate(
        &mut live,
        (
            Create::new(&square),
            circle.animate().shift(RIGHT).set_fill(Color::BLUE),
        )
            .into_animation_request(),
        linear(),
    )
    .unwrap();
    assert!(live.contains(&square).unwrap());
    assert!(live.contains(&circle).unwrap());
    finish(&mut live, entering);
    assert_eq!(square.node_id(), square_id);
    assert_eq!(circle.node_id(), circle_id);
    assert_eq!(
        live.effective(&circle).unwrap().transform.translation,
        RIGHT
    );

    let leaving = activate(
        &mut live,
        parallel([
            AnimationRequest::from(FadeOut::new(&square)),
            circle.animate().scale(2.0).shift(UP).into(),
        ]),
        linear(),
    )
    .unwrap();
    live.advance_segment_to(leaving, leaving.start_time() + 0.5)
        .unwrap();
    assert!(live.contains(&square).unwrap());
    assert!((live.effective(&square).unwrap().appearance - 0.5).abs() < 1e-6);
    finish(&mut live, leaving);
    assert!(!live.contains(&square).unwrap());
    assert!(live.contains(&circle).unwrap());
    let final_circle = live.effective(&circle).unwrap();
    assert_eq!(final_circle.transform.scale, Vec2::new(2.0, 2.0));
    assert_eq!(final_circle.transform.translation, RIGHT + UP);
}

#[test]
fn foreign_or_invalid_second_child_publishes_no_targets_or_runtime_changes() {
    let mut scene = Scene::new();
    let left = scene.square(1.0).unwrap();
    let right = scene.circle(0.5).unwrap();
    scene.add(&left).unwrap();
    scene.add(&right).unwrap();
    let mut foreign_scene = Scene::new();
    let foreign = foreign_scene.square(1.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let before_publication = session.publication_context();
    let before_frame = session.frame().clone();
    let before_nodes = scene.integration_store().borrow().len();
    let before_left = left.state().unwrap();
    let before_right = right.state().unwrap();

    for invalid in [
        foreign.animate().shift(UP),
        right.animate().shift(Vec2::new(f32::NAN, 0.0)),
        right.animate().scale(f64::INFINITY),
        right.animate().rotate(f64::NAN),
        right
            .animate()
            .set_fill(Color::rgba(0.2, 0.3, 0.4, f32::NAN)),
        right
            .animate()
            .shift(UP)
            .with_options(AnimationOptions::new().run_time(-1.0)),
    ] {
        let result = activate(
            &mut scene.live(&mut session),
            (left.animate().shift(RIGHT), invalid).into_animation_request(),
            AnimationOptions::new(),
        );
        assert!(result.is_err(), "invalid child was accepted");
        assert_eq!(scene.integration_store().borrow().len(), before_nodes);
        assert_eq!(session.publication_context(), before_publication);
        assert_eq!(session.frame(), &before_frame);
        assert!(session.take_frame_changes().is_empty());
        assert_eq!(left.state().unwrap(), before_left);
        assert_eq!(right.state().unwrap(), before_right);
    }
}

#[test]
fn stale_second_child_is_rejected_without_admitting_the_first() {
    let mut scene = Scene::new();
    let valid = scene.square(1.0).unwrap();
    let stale = scene.circle(0.5).unwrap();
    let request = (valid.animate().shift(RIGHT), stale.animate().shift(UP));
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(stale.node_id());
    remove
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let before = session.publication_context();
    let nodes = scene.integration_store().borrow().len();
    assert!(activate(
        &mut scene.live(&mut session),
        request.into_animation_request(),
        linear(),
    )
    .is_err());
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(session.publication_context(), before);
    assert!(session.frame().objects.is_empty());
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn canonical_driver_conflict_rolls_back_candidate_target_allocations() {
    let mut scene = Scene::new();
    let square = scene.square(1.0).unwrap();
    scene.add(&square).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let before = session.publication_context();
    let before_frame = session.frame().clone();
    let nodes = scene.integration_store().borrow().len();
    let result = activate(
        &mut scene.live(&mut session),
        (square.animate().shift(RIGHT), square.animate().shift(UP)).into_animation_request(),
        linear(),
    );
    assert!(
        matches!(
            result,
            Err(LiveSessionError::Activation(
                crate::ExecutionSessionAnimationError::PreparedAnimation(
                    noon_compile::PreparedSemanticAnimationLoweringError::MultipleDrivers { .. }
                )
            ))
        ),
        "unexpected conflict result: {result:?}"
    );
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.frame(), &before_frame);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn animate_rotation_interpolates_points_while_explicit_rotate_follows_an_arc() {
    for procedural in [false, true] {
        let mut scene = Scene::new();
        let square = scene.square(2.0).unwrap();
        scene.add(&square).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = if procedural {
            live.declare_and_activate_composition(
                &AnimationCompositionRequest::ManimRotate {
                    target: &square,
                    angle: std::f64::consts::PI,
                    pivot: ManimRotationPivot::Center,
                    options: linear(),
                },
                AnimationOptions::new(),
            )
            .unwrap()
        } else {
            activate(
                &mut live,
                square.animate().rotate(std::f64::consts::PI).into(),
                linear(),
            )
            .unwrap()
        };
        live.advance_segment_to(segment, segment.start_time() + 0.5)
            .unwrap();
        let points = live.point_array((&square).into()).unwrap();
        assert!(!points.points().is_empty());
        let radius = points
            .points()
            .iter()
            .map(|point| point.x.hypot(point.y))
            .fold(0.0_f64, f64::max);
        if procedural {
            assert!((radius - 2.0_f64.sqrt()).abs() < 1e-5);
        } else {
            assert!(radius < 1e-5, "method target midpoint radius was {radius}");
        }
        finish(&mut live, segment);
        assert!(
            (square.state().unwrap().transform.planar_rotation().unwrap() - std::f64::consts::PI)
                .abs()
                < 1e-6
        );
    }
}
