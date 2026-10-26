use std::sync::Arc;

use crate::{
    AnimationOptions, Color, ExecutionSession, GradientStop, LinearGradient, Mobject, RateFunction,
    Scene, SemanticPaint,
};

fn gradient(vertical: bool) -> Arc<LinearGradient> {
    Arc::new(
        LinearGradient::new(
            if vertical { [0.5, 0.0] } else { [0.0, 0.5] },
            if vertical { [0.5, 1.0] } else { [1.0, 0.5] },
            [
                GradientStop::new(
                    0.0,
                    if vertical {
                        Color::rgb(0.0, 1.0, 0.0)
                    } else {
                        Color::rgb(1.0, 0.0, 0.0)
                    },
                )
                .unwrap(),
                GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
            ],
        )
        .unwrap(),
    )
}
fn row<'a>(session: &'a ExecutionSession, source: &Mobject) -> &'a noon_runtime::FrameObjectState {
    let id = session.execution_object_id(source.node_id()).unwrap();
    session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == id)
        .unwrap()
}
fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}

#[test]
fn public_gradient_transform_samples_shared_fill_and_reconciles_exact_endpoint() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    source.disable_stroke().unwrap();
    source
        .set_fill_gradient(gradient(false), Some(0.25))
        .unwrap();
    scene.add(&source).unwrap();
    let target_paint = gradient(true);
    let mut target = source.target_editor().unwrap();
    target
        .set_fill_gradient(target_paint.clone(), Some(0.75))
        .unwrap();
    let animation = scene
        .declare_transform_to(&source, &target, options())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let middle = row(&session, &source);
    assert_eq!(middle.style.fill.unwrap().alpha, 0.5);
    let paint = middle.fill_gradient.as_ref().unwrap();
    assert_eq!(paint.start(), [0.25, 0.25]);
    assert_eq!(paint.end(), [0.75, 0.75]);
    assert_eq!(
        paint.sample_premultiplied([0.5, 0.5]).unwrap(),
        [0.25, 0.25, 0.5, 1.0]
    );
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    assert!(Arc::ptr_eq(
        row(&session, &source).fill_gradient.as_ref().unwrap(),
        &target_paint
    ));
    scene.live(&mut session).complete_segment(segment).unwrap();
    let state = scene.live(&mut session).authored(&source).unwrap();
    assert_eq!(
        state.style.fill,
        Some(SemanticPaint::LinearGradient(target_paint))
    );
    assert_eq!(state.style.fill_opacity, 0.75);
}

#[test]
fn public_solid_to_gradient_and_live_target_capture_keep_effective_paint() {
    let mut scene = Scene::new();
    let mut source = scene.square(1.0).unwrap();
    source.set_fill(1.0, 0.0, 0.0, 1.0).unwrap();
    scene.add(&source).unwrap();
    let mut target = source.target_editor().unwrap();
    target.set_fill_gradient(gradient(true), None).unwrap();
    let animation = scene
        .declare_transform_to(&source, &target, options())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let current = row(&session, &source).fill_gradient.clone().unwrap();
    let effective = scene.live(&mut session).effective(&source).unwrap();
    assert_eq!(effective.fill_gradient.as_ref(), Some(&current));
    assert!(
        scene
            .live(&mut session)
            .effective_fill_color(&source)
            .is_err(),
        "continuous paint has no single RGB color"
    );
    assert_eq!(
        current.sample_premultiplied([0.5, 0.5]).unwrap(),
        [0.5, 0.25, 0.25, 1.0]
    );
    let source_style = scene.live(&mut session).authored(&source).unwrap().style;
    assert_eq!(
        source_style
            .capture_effective_fill(row(&session, &source).style.fill, Some(&current))
            .0,
        Some(SemanticPaint::LinearGradient(current))
    );
    // Creating an authored target observes the ordinary segment completion barrier.
    assert!(scene.live(&mut session).target_editor(&source).is_err());
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    let current = row(&session, &source).fill_gradient.clone().unwrap();
    let captured = scene.live(&mut session).target_editor(&source).unwrap();
    let state = scene.live(&mut session).authored(&captured).unwrap();
    assert_eq!(
        state.style.fill,
        Some(SemanticPaint::LinearGradient(current))
    );
    assert_eq!(
        session.frame().objects.len(),
        1,
        "detached targets have no execution row"
    );
}

#[test]
fn invalid_gradient_axis_transition_rolls_back_activation() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.5).unwrap();
    source.set_fill_gradient(gradient(false), None).unwrap();
    scene.add(&source).unwrap();
    let reversed = LinearGradient::new(
        [1.0, 0.5],
        [0.0, 0.5],
        [
            GradientStop::new(0.0, Color::WHITE).unwrap(),
            GradientStop::new(1.0, Color::BLACK).unwrap(),
        ],
    )
    .unwrap();
    let mut target = source.target_editor().unwrap();
    target.set_fill_gradient(reversed, None).unwrap();
    let animation = scene
        .declare_transform_to(&source, &target, options())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.frame().clone();
    let context = session.publication_context();
    assert!(scene.live(&mut session).play_animation(&animation).is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
}

#[test]
fn bounded_returning_and_reversed_paint_tracks_restore_exact_source() {
    for opts in [
        options().rate_func(RateFunction::ThereAndBack),
        options().reverse_rate_function(true),
    ] {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.5).unwrap();
        let original = gradient(false);
        source
            .set_fill_gradient(original.clone(), Some(0.123456789012345))
            .unwrap();
        scene.add(&source).unwrap();
        let mut target = source.target_editor().unwrap();
        target
            .set_fill_gradient(gradient(true), Some(0.75))
            .unwrap();
        let animation = scene.declare_transform_to(&source, &target, opts).unwrap();
        let mut session = scene.execution_session().unwrap();
        let segment = scene.live(&mut session).play_animation(&animation).unwrap();
        for time in [0.0, 0.25, 0.5, 0.75, 1.0] {
            scene
                .live(&mut session)
                .advance_segment_to(segment, time)
                .unwrap();
        }
        scene.live(&mut session).complete_segment(segment).unwrap();
        let state = scene.live(&mut session).authored(&source).unwrap();
        assert_eq!(state.style.fill_opacity, 0.123456789012345);
        assert_eq!(
            state.style.fill,
            Some(SemanticPaint::LinearGradient(original.clone()))
        );
        assert!(Arc::ptr_eq(
            row(&session, &source).fill_gradient.as_ref().unwrap(),
            &original
        ));
    }
}

#[test]
fn gradient_glow_shared_authoring_accepts_both_orders_and_samples_both_channels() {
    use crate::effects::EffectUpdate;
    for paint_first in [false, true] {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.5).unwrap();
        source.disable_stroke().unwrap();
        if paint_first {
            source
                .set_fill_gradient(gradient(false), Some(0.25))
                .unwrap();
        }
        source
            .set_glow(EffectUpdate::default().intensity(0.4))
            .unwrap();
        if !paint_first {
            source
                .set_fill_gradient(gradient(false), Some(0.25))
                .unwrap();
        }
        scene.add(&source).unwrap();
        let mut target = source.target_editor().unwrap();
        target
            .set_fill_gradient(gradient(true), Some(0.75))
            .unwrap();
        target
            .set_glow(EffectUpdate::default().intensity(1.4))
            .unwrap();
        let animation = scene
            .declare_transform_to(&source, &target, options())
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let segment = scene.live(&mut session).play_animation(&animation).unwrap();
        scene
            .live(&mut session)
            .advance_segment_to(segment, 0.5)
            .unwrap();
        let state = row(&session, &source);
        assert!((state.glow.as_ref().unwrap().definition.intensity() - 0.9).abs() < 1e-12);
        assert_eq!(state.style.fill.unwrap().alpha, 0.5);
        assert_eq!(state.fill_gradient.as_ref().unwrap().start(), [0.25, 0.25]);
        scene
            .live(&mut session)
            .advance_segment_to(segment, 1.0)
            .unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        scene
            .live(&mut session)
            .set_fill_gradient(&source, gradient(false), None)
            .unwrap();
        assert_eq!(row(&session, &source).style.fill.unwrap().alpha, 0.75);
        assert_eq!(
            row(&session, &source)
                .glow
                .as_ref()
                .unwrap()
                .definition
                .intensity(),
            1.4
        );
    }
}

#[test]
fn gradient_temporary_pulse_restores_exact_paint_and_retires_only_glow() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.5).unwrap();
    let original = gradient(false);
    source
        .set_fill_gradient(original.clone(), Some(0.375))
        .unwrap();
    scene.add(&source).unwrap();
    let pulse = scene
        .declare_glow_pulse(&source, 2.0, AnimationOptions::new())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&pulse).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    assert_eq!(
        row(&session, &source)
            .glow
            .as_ref()
            .unwrap()
            .definition
            .intensity(),
        2.0
    );
    assert!(Arc::ptr_eq(
        row(&session, &source).fill_gradient.as_ref().unwrap(),
        &original
    ));
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert!(row(&session, &source).glow.is_none());
    assert_eq!(row(&session, &source).style.fill.unwrap().alpha, 0.375);
    assert!(Arc::ptr_eq(
        row(&session, &source).fill_gradient.as_ref().unwrap(),
        &original
    ));
}
