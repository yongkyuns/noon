//! Code regions remain ordinary geometry through the shared animation lifecycle.
#![cfg(all(feature = "code", feature = "bundled-fonts"))]

use noon::{
    AnimationCompositionRequest as Request, AnimationOptions, AuthoringError, Code,
    CodeAuthoringError, CodeHighlightOptions, CodeOptions, ManimRotationPivot, RateFunction, Scene,
    TransformToRequest,
};

fn plain() -> CodeOptions {
    CodeOptions {
        language: Some("text".into()),
        ..Default::default()
    }
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0e-5,
        "expected {expected}, observed {actual}"
    );
}

fn exercise_range_transform(declare_live: bool) {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "alpha\n\n    beta\nomega", plain()).unwrap();
    code.family().scale(-1.25, 0.8).unwrap();
    code.family()
        .rotate(0.18, ManimRotationPivot::Center)
        .unwrap();
    code.family().shift(0.35, -0.1).unwrap();
    let focus = code
        .highlight_lines(1..2, CodeHighlightOptions::default())
        .unwrap();
    let focus_id = focus.node_id();
    let content = focus.state().unwrap().content;
    let initial = focus.layout_bounds().unwrap().unwrap();
    let target_options = CodeHighlightOptions {
        include_line_numbers: true,
        padding: 0.08,
        ..Default::default()
    };
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let cold_target =
        (!declare_live).then(|| code.get_highlighted_lines(2..4, target_options).unwrap());
    scene.add_many(&[code.family().into()]).unwrap();
    let declaration = cold_target
        .as_ref()
        .map(|target| scene.declare_transform_to(&focus, target, options).unwrap());
    let mut session = scene.execution_session().unwrap();
    let execution_id = session.execution_object_id(focus_id).unwrap();
    let target;
    let expected;
    {
        let mut live = scene.live(&mut session);
        target = match cold_target {
            Some(target) => target,
            None => code
                .get_highlighted_lines_in_live_session(&mut live, 2..4, target_options)
                .unwrap(),
        };
        expected = target.layout_bounds().unwrap().unwrap();
        assert!(expected.width() > initial.width());
        assert!(expected.height() > initial.height());
        assert_eq!(target.state().unwrap().content, content);
        let segment = match declaration {
            Some(animation) => live.play_animation(&animation).unwrap(),
            None => live
                .declare_and_activate_composition(
                    &Request::TransformTo(TransformToRequest::new(&focus, &target, options)),
                    AnimationOptions::new(),
                )
                .unwrap(),
        };
        live.advance_segment_to(segment, 0.5).unwrap();
        let halfway = live.effective_layout(&focus).unwrap();
        close(
            halfway.center.0,
            (initial.min_x + initial.max_x + expected.min_x + expected.max_x) * 0.25,
        );
        close(
            halfway.center.1,
            (initial.min_y + initial.max_y + expected.min_y + expected.max_y) * 0.25,
        );
        close(halfway.width, (initial.width() + expected.width()) * 0.5);
        close(halfway.height, (initial.height() + expected.height()) * 0.5);
        assert_eq!(focus.state().unwrap().content, content);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        let completed = live.effective_layout(&focus).unwrap();
        close(completed.center.0, (expected.min_x + expected.max_x) * 0.5);
        close(completed.center.1, (expected.min_y + expected.max_y) * 0.5);
        close(completed.width, expected.width());
        close(completed.height, expected.height());
    }
    let authored = focus.layout_bounds().unwrap().unwrap();
    close(authored.min_x, expected.min_x);
    close(authored.min_y, expected.min_y);
    close(authored.max_x, expected.max_x);
    close(authored.max_y, expected.max_y);
    assert_eq!(focus.node_id(), focus_id);
    assert_eq!(session.execution_object_id(focus_id), Some(execution_id));
    assert_eq!(focus.state().unwrap().content, content);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(code.highlights().node_id())
            .unwrap(),
        vec![focus_id],
    );
    assert_ne!(target.node_id(), focus_id);
}

#[test]
fn predeclared_transform_resizes_a_code_region_without_replacing_identity() {
    exercise_range_transform(false);
}

#[test]
fn live_transform_resizes_a_code_region_without_replacing_identity() {
    exercise_range_transform(true);
}

#[test]
fn regions_preserve_zero_scale_and_reject_extent_scale_overflow_atomically() {
    let mut scene = Scene::new();
    let flattened = Code::new(&mut scene, "one\ntwo", plain()).unwrap();
    flattened.family().scale(0.0, 1.0).unwrap();
    let region = flattened
        .get_highlighted_lines(0..2, CodeHighlightOptions::default())
        .unwrap();
    let bounds = region.layout_bounds().unwrap().unwrap();
    close(bounds.width(), 0.0);
    assert!(bounds.height() > 0.0);

    let large = Code::new(&mut scene, "one", plain()).unwrap();
    large.family().scale(1.0e20, 1.0e20).unwrap();
    let before_revision = scene.revision();
    let before_nodes = scene.integration_store().borrow().len();
    let before_resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    let result = large.highlight_lines(
        0..1,
        CodeHighlightOptions {
            padding: 1.0e20,
            ..Default::default()
        },
    );
    assert!(matches!(
        result,
        Err(CodeAuthoringError::Semantic(
            AuthoringError::InvalidRenderNumber { .. }
        ))
    ));
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(scene.integration_store().borrow().len(), before_nodes);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        before_resources
    );
    assert!(scene
        .integration_store()
        .borrow()
        .semantic_family_members_checked(large.highlights().node_id())
        .unwrap()
        .is_empty());
}
