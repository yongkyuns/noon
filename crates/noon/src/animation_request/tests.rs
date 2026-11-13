use super::*;
use crate::{AnimationCompositionRequest, ManimRotationPivot, RateFunction, Scene};
use noon_core::{SemanticMutationTransaction, RIGHT, UP};

fn linear() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}

fn group_scene() -> (Scene, MobjectFamily, Mobject, Mobject) {
    let mut scene = Scene::new();
    let mut left = scene.square(1.0).unwrap();
    left.set_translation(-2.0, 0.0).unwrap();
    let mut right = scene.square(1.0).unwrap();
    right.set_translation(2.0, 0.0).unwrap();
    let family = scene.family(&[(&left).into(), (&right).into()]).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    (scene, family, left, right)
}

#[test]
fn group_requests_capture_at_admission_and_share_chained_pivots() {
    let (scene, family, left, right) = group_scene();
    let nodes = scene.integration_store().borrow().len();
    let retained = family
        .animate()
        .scale(0.5)
        .rotate(std::f64::consts::FRAC_PI_2)
        .set_fill(Color::BLUE)
        .shift(UP);
    let cloned = retained.clone();
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let first = activate(&mut live, family.animate().shift(RIGHT).into(), linear()).unwrap();
    finish(&mut live, first);
    let segment = activate(&mut live, retained.into(), linear()).unwrap();
    // Shared pivot after shift is (1, 0), not either member's own center.
    finish(&mut live, segment);
    for (object, y) in [(&left, 0.0), (&right, 2.0)] {
        let state = live.effective(object).unwrap();
        assert!((state.transform.translation.x - 1.0).abs() < 1e-6);
        assert!((state.transform.translation.y - y).abs() < 1e-6);
        assert_eq!(state.transform.scale, Vec2::new(0.5, 0.5));
        assert_eq!(state.style.fill, Some(Color::BLUE));
    }
    assert_eq!(
        family.own_style().unwrap().family_color(),
        Some(Color::BLUE)
    );
    let segment = activate(&mut live, cloned.into(), linear()).unwrap();
    finish(&mut live, segment);
    for (object, x) in [(&left, 1.5), (&right, 0.5)] {
        let state = live.effective(object).unwrap();
        assert!((state.transform.translation.x - x).abs() < 1e-6);
        assert!((state.transform.translation.y - 2.0).abs() < 1e-6);
        assert_eq!(state.transform.scale, Vec2::new(0.25, 0.25));
    }
}

#[test]
fn group_animation_preserves_alias_identity_and_unrelated_state() {
    let (mut scene, family, left, right) = group_scene();
    let alias = scene.family(&[(&left).into()]).unwrap();
    let untouched = scene.circle(0.25).unwrap();
    scene
        .add_many(&[(&alias).into(), (&untouched).into()])
        .unwrap();
    let original = untouched.state().unwrap();
    let mut session = scene.execution_session().unwrap();
    let untouched_id = session.execution_object_id(untouched.node_id());
    let mut live = scene.live(&mut session);
    let segment = activate(&mut live, family.animate().shift(UP).into(), linear()).unwrap();
    live.advance_segment_to(segment, 0.5).unwrap();
    assert_eq!(live.effective(&left).unwrap().transform.translation.y, 0.5);
    assert_eq!(live.effective(&right).unwrap().transform.translation.y, 0.5);
    finish(&mut live, segment);
    assert_eq!(alias.member_count().unwrap(), 1);
    assert_eq!(
        alias.member_at(0).unwrap().unwrap().node_id(),
        left.node_id()
    );
    assert_eq!(untouched.state().unwrap(), original);
    assert_eq!(
        session.execution_object_id(untouched.node_id()),
        untouched_id
    );
}

#[test]
fn group_and_independent_object_share_timing_admission_and_completion() {
    let (mut scene, family, left, right) = group_scene();
    let marker = scene.circle(0.25).unwrap(); // Admitted with the complete play.
    let alias = scene.family(&[(&left).into()]).unwrap();
    let mut session = scene.into_execution().unwrap();
    let left_id = session.session().execution_object_id(left.node_id());
    let segment = activate(
        &mut session.live(),
        (
            marker
                .animate()
                .shift(RIGHT * 2.0)
                .with_options(linear().run_time(4.0)),
            family
                .animate()
                .shift(UP)
                .set_color(Color::ORANGE)
                .with_options(linear().run_time(4.0).lag_ratio(1.0)),
        )
            .into_animation_request(),
        linear().run_time(2.0),
    )
    .unwrap();
    assert!(session.scene_mut().contains_direct(&marker).unwrap());
    assert_eq!(segment.end_time() - segment.start_time(), 2.0);
    session.live().advance_segment_to(segment, 0.5).unwrap();
    assert_eq!(
        session
            .live()
            .effective(&marker)
            .unwrap()
            .transform
            .translation
            .x,
        0.5
    );
    assert_eq!(
        session
            .live()
            .effective(&left)
            .unwrap()
            .transform
            .translation
            .y,
        0.5
    );
    assert_eq!(
        session
            .live()
            .effective(&right)
            .unwrap()
            .transform
            .translation
            .y,
        0.0
    );
    finish(&mut session.live(), segment);
    assert_eq!(marker.state().unwrap().transform.translation.x, 2.0);
    assert_eq!(right.state().unwrap().transform.translation.y, 1.0);
    assert_eq!(
        family.own_style().unwrap().family_color(),
        Some(Color::ORANGE)
    );

    let segment = activate(
        &mut session.live(),
        (family.animate().shift(UP), FadeOut::new(&marker)).into_animation_request(),
        linear(),
    )
    .unwrap();
    finish(&mut session.live(), segment);
    assert!(!session.scene_mut().contains_direct(&marker).unwrap());
    assert_eq!(right.state().unwrap().transform.translation.y, 2.0);
    assert_eq!(
        alias.member_at(0).unwrap().unwrap().node_id(),
        left.node_id()
    );
    assert_eq!(
        session.session().execution_object_id(left.node_id()),
        left_id
    );
}

#[test]
fn group_late_child_and_member_driver_failures_publish_nothing() {
    let (mut scene, family, left, right) = group_scene();
    let marker = scene.circle(0.25).unwrap();
    let mut session = scene.into_execution().unwrap();
    session.session_mut().take_frame_changes();
    let context = session.session().publication_context();
    let frame = session.session().frame().clone();
    let nodes = session.scene().integration_store().borrow().len();
    let states = (left.state().unwrap(), right.state().unwrap());
    let invalids: [AnimationRequest; 6] = [
        marker
            .animate()
            .with_options(AnimationOptions::new().run_time(-1.0))
            .into(),
        left.animate().shift(UP).into(),
        FadeOut::new(&right).into(),
        Create::new(&left).into(),
        family.animate().shift(RIGHT).into(),
        family.animate().scale(f64::NAN).into(),
    ];
    for invalid in invalids {
        for reverse in [false, true] {
            let mut children = vec![
                family.animate().shift(UP).into(),
                marker.animate().shift(RIGHT).into(),
                invalid.clone(),
            ];
            if reverse {
                children.reverse();
            }
            let error = activate(
                &mut session.live(),
                children.into_animation_request(),
                AnimationOptions::new(),
            );
            assert!(error.is_err(), "unexpected admission: {error:?}");
            assert_eq!(session.scene().integration_store().borrow().len(), nodes);
            assert_eq!(session.session().publication_context(), context);
            assert_eq!(session.session().frame(), &frame);
            assert!(session.session_mut().take_frame_changes().is_empty());
            assert_eq!((left.state().unwrap(), right.state().unwrap()), states);
            assert!(!session.scene_mut().contains_direct(&marker).unwrap());
        }
    }
}

#[test]
fn unsupported_nested_group_is_checked_at_admission() {
    let (mut scene, family, left, _) = group_scene();
    let request = family.animate().shift(UP);
    let nested = scene.family(&[(&left).into()]).unwrap();
    scene
        .add_family_members(&family, &[(&nested).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let nodes = scene.integration_store().borrow().len();
    let context = session.publication_context();
    let error = activate(&mut scene.live(&mut session), request.into(), linear()).unwrap_err();
    assert!(error.to_string().contains("flat vector family"), "{error}");
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(session.publication_context(), context);
}

#[test]
fn group_target_preserves_contraction_visibility_and_stroke_opacity() {
    let (mut scene, family, left, right) = group_scene();
    let target = scene.square(1.0).unwrap();
    let smaller = scene.family(&[(&target).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live
        .declare_and_activate_composition(
            &AnimationCompositionRequest::FamilyTransformTo {
                source: &family,
                target_state: &smaller,
                options: linear(),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    finish(&mut live, segment);
    let hidden = live.effective(&right).unwrap().style.opacity;
    assert_eq!(hidden, 0.0);
    let stroke_opacity = right.state().unwrap().style.stroke_opacity;
    let segment = activate(
        &mut live,
        family.animate().shift(UP).set_color(Color::ORANGE).into(),
        linear(),
    )
    .unwrap();
    finish(&mut live, segment);
    assert_eq!(live.effective(&right).unwrap().style.opacity, hidden);
    assert_eq!(live.effective(&left).unwrap().style.opacity, 1.0);
    assert_eq!(right.state().unwrap().style.stroke_opacity, stroke_opacity);
    assert_eq!(
        family.own_style().unwrap().family_color(),
        Some(Color::ORANGE)
    );
    assert_eq!(family.member_count().unwrap(), 2);
}

#[test]
fn group_member_lag_and_play_overrides_use_the_existing_family_clock() {
    let (scene, family, left, right) = group_scene();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = activate(
        &mut live,
        family
            .animate()
            .shift(UP)
            .with_options(linear().run_time(4.0).lag_ratio(1.0))
            .into(),
        linear().run_time(2.0),
    )
    .unwrap();
    assert_eq!(segment.end_time() - segment.start_time(), 2.0);
    live.advance_segment_to(segment, 0.5).unwrap();
    assert_eq!(live.effective(&left).unwrap().transform.translation.y, 0.5);
    assert_eq!(live.effective(&right).unwrap().transform.translation.y, 0.0);
    finish(&mut live, segment);
    assert_eq!(live.effective(&right).unwrap().transform.translation.y, 1.0);
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
    let mut session = scene.into_execution().unwrap();
    let entering = activate(
        &mut session.live(),
        (
            Create::new(&square),
            circle.animate().shift(RIGHT).set_fill(Color::BLUE),
        )
            .into_animation_request(),
        linear(),
    )
    .unwrap();
    assert!(session.scene_mut().contains_direct(&square).unwrap());
    assert!(session.scene_mut().contains_direct(&circle).unwrap());
    finish(&mut session.live(), entering);
    assert_eq!(square.node_id(), square_id);
    assert_eq!(circle.node_id(), circle_id);
    assert_eq!(
        session
            .live()
            .effective(&circle)
            .unwrap()
            .transform
            .translation,
        RIGHT
    );

    let leaving = activate(
        &mut session.live(),
        parallel([
            AnimationRequest::from(FadeOut::new(&square)),
            circle.animate().scale(2.0).shift(UP).into(),
        ]),
        linear(),
    )
    .unwrap();
    session
        .live()
        .advance_segment_to(leaving, leaving.start_time() + 0.5)
        .unwrap();
    assert!(session.scene_mut().contains_direct(&square).unwrap());
    assert!((session.live().effective(&square).unwrap().appearance - 0.5).abs() < 1e-6);
    finish(&mut session.live(), leaving);
    assert!(!session.scene_mut().contains_direct(&square).unwrap());
    assert!(session.scene_mut().contains_direct(&circle).unwrap());
    let final_circle = session.live().effective(&circle).unwrap();
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
        let mut running = scene.into_execution().unwrap();
        let mut live = running.live();
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
        let points = running
            .scene()
            .effective_point_array((&square).into())
            .unwrap();
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
        finish(&mut running.live(), segment);
        assert!(
            (square.state().unwrap().transform.planar_rotation().unwrap() - std::f64::consts::PI)
                .abs()
                < 1e-6
        );
    }
}
