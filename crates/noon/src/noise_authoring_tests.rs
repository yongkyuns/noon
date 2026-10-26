use crate::{AnimationOptions, Color, NoisePaint, RateFunction, Scene, SemanticPaint};
use std::sync::Arc;

fn paint(frequency: [f64; 2], phase: [f64; 2]) -> Arc<NoisePaint> {
    Arc::new(
        NoisePaint::new(
            7,
            frequency,
            phase,
            [Color::WHITE, Color::rgba(0.0, 0.0, 1.0, 0.0)],
        )
        .unwrap(),
    )
}

#[test]
fn noise_live_animation_keeps_identity_exact_completion_and_independent_opacity() {
    let mut scene = Scene::new();
    let mut source = scene.rectangle(4.0, 2.0).unwrap();
    source
        .set_fill_noise(paint([0.25; 2], [0.5; 2]), Some(0.25))
        .unwrap();
    source.set_object_opacity(0.5).unwrap();
    scene.add(&source).unwrap();
    let identity = source.node_id();
    let endpoint = paint([0.75; 2], [1.0; 2]);
    let mut target = source.target_editor().unwrap();
    target.set_fill_noise(endpoint.clone(), Some(0.75)).unwrap();
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
    let noise = definition.as_noise().unwrap();
    assert_eq!(noise.frequency(), [0.5; 2]);
    assert_eq!(noise.phase(), [0.75; 2]);
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
        Some(SemanticPaint::Noise(endpoint.clone()))
    );
    let effective = scene.live(&mut session).effective(&source).unwrap();
    assert!(Arc::ptr_eq(
        effective.fill_paint.as_ref().unwrap().as_noise().unwrap(),
        &endpoint
    ));
    assert_eq!(source.node_id(), identity);
}

#[test]
fn animated_seed_replacement_rejects_before_publication() {
    let mut scene = Scene::new();
    let mut source = scene.circle(1.0).unwrap();
    source
        .set_fill_noise(paint([8.0; 2], [0.0; 2]), Some(1.0))
        .unwrap();
    scene.add(&source).unwrap();
    let mut target = source.target_editor().unwrap();
    target
        .set_fill_noise(
            NoisePaint::new(8, [8.0; 2], [0.0; 2], [Color::WHITE, Color::BLACK]).unwrap(),
            None,
        )
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
}

#[test]
fn rejected_live_noise_profile_preserves_authored_frame_and_publication() {
    let mut scene = Scene::new();
    let mut source = scene.circle(1.0).unwrap();
    source
        .set_fill_noise(paint([0.5; 2], [0.5; 2]), Some(0.25))
        .unwrap();
    scene.add(&source).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.frame().clone();
    let publication = session.publication_context();
    let authored = scene.live(&mut session).authored(&source).unwrap();
    assert!(scene
        .live(&mut session)
        .set_fill_noise(&source, paint([1e-60, 0.5], [0.5; 2]), Some(0.75))
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(
        scene.live(&mut session).authored(&source).unwrap(),
        authored
    );
    scene
        .live(&mut session)
        .set_fill_noise(&source, paint([0.75; 2], [0.75; 2]), None)
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
fn non_solid_effective_zero_fill_uses_the_current_stroke_color() {
    for image in [false, true] {
        let mut scene = Scene::new();
        let mut source = scene.circle(1.0).unwrap();
        if image {
            source
                .set_fill_image(
                    crate::ImagePaintOptions::rgba8(1, 1, [255; 4]).unwrap(),
                    Some(1.0),
                )
                .unwrap();
        } else {
            source
                .set_fill_noise(paint([8.0; 2], [0.0; 2]), Some(1.0))
                .unwrap();
        }
        source.set_stroke_color(0.0, 1.0, 0.0, 1.0).unwrap();
        scene.add(&source).unwrap();
        let mut target = source.target_editor().unwrap();
        target.set_fill_opacity(0.0).unwrap();
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
            .advance_segment_to(segment, 1.0)
            .unwrap();
        assert_eq!(
            scene
                .live(&mut session)
                .authored(&source)
                .unwrap()
                .style
                .fill_opacity,
            1.0
        );
        assert_eq!(
            scene
                .live(&mut session)
                .effective_manim_color(&source)
                .unwrap(),
            Color::rgb(0.0, 1.0, 0.0)
        );
    }
}
