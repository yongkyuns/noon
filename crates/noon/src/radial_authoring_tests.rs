use crate::{
    AnimationOptions, Color, GradientStop, RadialGradient, RateFunction, Scene, SemanticPaint,
};
use std::sync::Arc;

fn paint(center: [f64; 2], radii: [f64; 2]) -> Arc<RadialGradient> {
    Arc::new(
        RadialGradient::new(
            center,
            radii,
            [
                GradientStop::new(0.0, Color::WHITE).unwrap(),
                GradientStop::new(1.0, Color::rgba(0.0, 0.0, 1.0, 0.0)).unwrap(),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn radial_live_animation_keeps_identity_exact_completion_and_independent_opacity() {
    let mut scene = Scene::new();
    let mut source = scene.rectangle(4.0, 2.0).unwrap();
    source
        .set_fill_radial_gradient(paint([0.25; 2], [0.5; 2]), Some(0.25))
        .unwrap();
    source.set_object_opacity(0.5).unwrap();
    scene.add(&source).unwrap();
    let identity = source.node_id();
    let endpoint = paint([0.75; 2], [1.0; 2]);
    let mut target = source.target_editor().unwrap();
    target
        .set_fill_radial_gradient(endpoint.clone(), Some(0.75))
        .unwrap();
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
    let radial = definition.as_radial_gradient().unwrap();
    assert_eq!(radial.center(), [0.5; 2]);
    assert_eq!(radial.radii(), [0.75; 2]);
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
        Some(SemanticPaint::RadialGradient(endpoint.clone()))
    );
    let effective = scene.live(&mut session).effective(&source).unwrap();
    assert!(Arc::ptr_eq(
        effective
            .fill_paint
            .as_ref()
            .unwrap()
            .as_radial_gradient()
            .unwrap(),
        &endpoint
    ));
    assert_eq!(source.node_id(), identity);
}

#[test]
fn rejected_live_radial_profile_preserves_authored_frame_and_publication() {
    let mut scene = Scene::new();
    let mut source = scene.circle(1.0).unwrap();
    source
        .set_fill_radial_gradient(paint([0.5; 2], [0.5; 2]), Some(0.25))
        .unwrap();
    scene.add(&source).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.frame().clone();
    let publication = session.publication_context();
    let authored = scene.live(&mut session).authored(&source).unwrap();
    assert!(scene
        .live(&mut session)
        .set_fill_radial_gradient(&source, paint([0.5; 2], [1e-60, 0.5]), Some(0.75))
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(
        scene.live(&mut session).authored(&source).unwrap(),
        authored
    );
    scene
        .live(&mut session)
        .set_fill_radial_gradient(&source, paint([0.75; 2], [0.75; 2]), None)
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
