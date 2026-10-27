use crate::{
    AnimationOptions, Color, HatchPaint, HatchPaintOptions, RateFunction, Scene, SemanticPaint,
};
use std::sync::Arc;

fn paint(spacing: f64, phase: [f64; 2], cross: bool) -> Arc<HatchPaint> {
    Arc::new(
        HatchPaint::new(
            Color::WHITE,
            Color::TRANSPARENT,
            HatchPaintOptions {
                spacing,
                width: spacing * 0.25,
                phase,
                cross,
                angle: 0.0,
            },
        )
        .unwrap(),
    )
}

#[test]
fn hatch_target_uses_shared_fill_identity_opacity_and_exact_completion() {
    let mut scene = Scene::new();
    let mut source = scene.rectangle(4.0, 2.0).unwrap();
    source
        .set_fill_hatch(paint(0.25, [0.0; 2], false), Some(0.25))
        .unwrap();
    source.set_object_opacity(0.5).unwrap();
    scene.add(&source).unwrap();
    let identity = source.node_id();
    let endpoint = paint(0.75, [2.0, -1.0], false);
    let mut target = source.target_editor().unwrap();
    target.set_fill_hatch(endpoint.clone(), Some(0.75)).unwrap();
    let animation = scene
        .declare_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let middle = scene.live(&mut session).effective(&source).unwrap();
    let definition = middle.fill_paint.unwrap();
    let hatch = definition.as_hatch().unwrap();
    assert_eq!(hatch.spacing(), 0.5);
    assert_eq!(hatch.width(), 0.125);
    assert_eq!(hatch.phase(), [1.0, -0.5]);
    assert_eq!(middle.style.fill.unwrap().alpha, 0.5);
    assert_eq!(middle.style.opacity, 0.5);
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    let authored = scene.live(&mut session).authored(&source).unwrap();
    assert_eq!(
        authored.style.fill,
        Some(SemanticPaint::Hatch(endpoint.clone()))
    );
    let effective = scene.live(&mut session).effective(&source).unwrap();
    assert!(Arc::ptr_eq(
        effective.fill_paint.as_ref().unwrap().as_hatch().unwrap(),
        &endpoint
    ));
    assert_eq!(source.node_id(), identity);
}

#[test]
fn animated_cross_topology_rejects_atomically_but_immediate_edit_is_valid() {
    let mut scene = Scene::new();
    let mut source = scene.circle(1.0).unwrap();
    source
        .set_fill_hatch(paint(0.25, [0.0; 2], false), Some(1.0))
        .unwrap();
    scene.add(&source).unwrap();
    let mut target = source.target_editor().unwrap();
    target
        .set_fill_hatch(paint(0.25, [0.0; 2], true), None)
        .unwrap();
    let animation = scene
        .declare_transform_to(&source, &target, AnimationOptions::new())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.frame().clone();
    let context = session.publication_context();
    assert!(scene.live(&mut session).play_animation(&animation).is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
    scene
        .live(&mut session)
        .set_fill_hatch(&source, paint(0.25, [0.0; 2], true), None)
        .unwrap();
    assert!(scene
        .live(&mut session)
        .authored(&source)
        .unwrap()
        .style
        .fill
        .unwrap()
        .as_hatch()
        .unwrap()
        .cross());
}

#[test]
fn invalid_local_hatch_profile_preserves_authored_and_effective_publications() {
    let mut scene = Scene::new();
    let mut source = scene.circle(1.0).unwrap();
    source
        .set_fill_hatch(paint(0.5, [0.0; 2], false), Some(0.25))
        .unwrap();
    scene.add(&source).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.frame().clone();
    let publication = session.publication_context();
    let authored = scene.live(&mut session).authored(&source).unwrap();
    for spacing in [1e-60, 1e60, 1e-10] {
        assert!(scene
            .live(&mut session)
            .set_fill_hatch(&source, paint(spacing, [0.0; 2], false), Some(0.75))
            .is_err());
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(
            scene.live(&mut session).authored(&source).unwrap(),
            authored
        );
    }
    scene
        .live(&mut session)
        .set_fill_hatch(&source, paint(0.75, [4.25, -8.5], true), None)
        .unwrap();
    assert_eq!(
        scene
            .live(&mut session)
            .authored(&source)
            .unwrap()
            .style
            .fill_opacity,
        0.25
    );
}

#[test]
fn zero_hatch_fill_observes_current_solid_stroke_color() {
    let mut scene = Scene::new();
    let mut source = scene.circle(1.0).unwrap();
    source
        .set_fill_hatch(paint(0.5, [0.0; 2], false), Some(1.0))
        .unwrap();
    source.set_stroke_color(0.0, 1.0, 0.0, 1.0).unwrap();
    scene.add(&source).unwrap();
    let mut target = source.target_editor().unwrap();
    target.set_fill_opacity(0.0).unwrap();
    let animation = scene
        .declare_transform_to(&source, &target, AnimationOptions::new().run_time(1.0))
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    assert_eq!(
        scene
            .live(&mut session)
            .effective_manim_color(&source)
            .unwrap(),
        Color::rgb(0.0, 1.0, 0.0)
    );
}
