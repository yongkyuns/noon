use crate::{AnimationOptions, Color, NoisePaint, RateFunction, Scene, SemanticPaint};
use std::sync::Arc;

fn fractal(phase: [f64; 2], gain: f64) -> NoisePaint {
    NoisePaint::fractal(Color::BLACK, Color::WHITE)
        .unwrap()
        .with_seed(7)
        .with_phase(phase)
        .unwrap()
        .with_detail(4, 2.0, gain)
        .unwrap()
}

#[test]
fn fractal_scene_script_combines_movement_and_paint_without_mutating_reused_value() {
    let mut program = crate::example_scenes::fractal_noise::program().unwrap();
    assert_eq!(program.session().frame().objects.len(), 3);
    let initial = program.session().frame().objects[2].fill_paint.clone();
    let identities = program
        .session()
        .frame()
        .objects
        .iter()
        .map(|object| object.id)
        .collect::<Vec<_>>();
    program.resume().unwrap();
    let mut callbacks = crate::RustHostCallbackTable::new();
    program.drive_to(&mut callbacks, 0.5).unwrap();
    let publication = program.take_renderer_publication().context();
    program.admit_publication(publication).unwrap();
    program.resume().unwrap();
    program.drive_to(&mut callbacks, 2.0).unwrap();
    let frame = program.session().frame();
    assert!((frame.objects[0].transform.translation.y - 0.2).abs() < 1e-6);
    let noise = frame.objects[0]
        .fill_paint
        .as_ref()
        .unwrap()
        .as_noise()
        .unwrap();
    assert_eq!(noise.phase(), [0.75, 0.0]);
    assert_eq!(frame.objects[2].fill_paint, initial);
    assert_eq!(
        frame
            .objects
            .iter()
            .map(|object| object.id)
            .collect::<Vec<_>>(),
        identities
    );
    program.drive_to(&mut callbacks, 3.5).unwrap();
    let publication = program.take_renderer_publication().context();
    program.admit_publication(publication).unwrap();
    program.resume().unwrap();
    assert_eq!(
        program.drive_to(&mut callbacks, 4.0).unwrap(),
        crate::LiveProgramStatus::ReadyToResume
    );
    assert_eq!(
        program.resume().unwrap(),
        crate::LiveProgramStatus::Finished
    );
}

#[test]
fn fractal_family_animation_rejects_before_any_sibling_movement_is_published() {
    let mut scene = Scene::new();
    let first = scene.circle(0.5).unwrap();
    let second = scene.circle(0.5).unwrap();
    let family = scene.family(&[(&first).into()]).unwrap();
    scene
        .add_many(&[(&family).into(), (&second).into()])
        .unwrap();
    let mut running = scene.into_execution().unwrap();
    let before = running.session().frame().clone();
    let publication = running.session().publication_context();
    let nodes = running.scene().integration_store().borrow().len();
    let request = crate::parallel((
        family.animate().set_fill_noise(fractal([0.0; 2], 0.5)),
        second.animate().shift(crate::Vec2::new(1.0, 0.0)),
    ));
    assert!(crate::animation_request::activate(
        &mut running.live(),
        request,
        AnimationOptions::new(),
    )
    .is_err());
    assert_eq!(running.session().frame(), &before);
    assert_eq!(running.session().publication_context(), publication);
    assert_eq!(running.scene().integration_store().borrow().len(), nodes);
}

#[test]
fn fractal_animation_uses_normal_fill_identity_and_completes_exactly() {
    let mut scene = Scene::new();
    let mut source = scene.rectangle(4.0, 2.0).unwrap();
    source
        .set_fill_noise(fractal([0.0; 2], 0.25), Some(0.4))
        .unwrap();
    scene.add(&source).unwrap();
    let identity = source.node_id();
    let endpoint = Arc::new(fractal([1.0, -1.0], 0.75));
    let mut target = source.target_editor().unwrap();
    target.set_fill_noise(endpoint.clone(), None).unwrap();
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
    assert_eq!(noise.octaves(), 4);
    assert_eq!(noise.phase(), [0.5, -0.5]);
    assert_eq!(noise.gain(), 0.5);
    assert_eq!(middle.style.fill.unwrap().alpha, 0.4);
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    let effective = scene.live(&mut session).effective(&source).unwrap();
    assert!(Arc::ptr_eq(
        effective.fill_paint.as_ref().unwrap().as_noise().unwrap(),
        &endpoint
    ));
    assert_eq!(source.node_id(), identity);
    assert_eq!(
        scene.authored(&source).unwrap().style.fill,
        Some(SemanticPaint::Noise(endpoint))
    );
}

#[test]
fn fractal_discrete_detail_and_unsupported_upper_octave_reject_atomically() {
    for replacement in [
        fractal([0.0; 2], 0.5).with_detail(5, 2.0, 0.5).unwrap(),
        fractal([0.0; 2], 0.5).with_detail(4, 1.5, 0.5).unwrap(),
    ] {
        let mut scene = Scene::new();
        let mut source = scene.circle(1.0).unwrap();
        source.set_fill_noise(fractal([0.0; 2], 0.5), None).unwrap();
        scene.add(&source).unwrap();
        let mut target = source.target_editor().unwrap();
        target.set_fill_noise(replacement, None).unwrap();
        let animation = scene
            .declare_transform_to(&source, &target, AnimationOptions::new())
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.frame().clone();
        let publication = session.publication_context();
        let authored = scene.authored(&source).unwrap();
        assert!(scene.live(&mut session).play_animation(&animation).is_err());
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(scene.authored(&source).unwrap(), authored);
    }

    let mut scene = Scene::new();
    let mut source = scene.circle(1.0).unwrap();
    source.set_fill_noise(fractal([0.0; 2], 0.5), None).unwrap();
    scene.add(&source).unwrap();
    let mut running = scene.into_execution().unwrap();
    let before = running.session().frame().clone();
    let publication = running.session().publication_context();
    let authored = running.scene().authored(&source).unwrap();
    // The base frequency is admitted, but its fourth octave exceeds the profile.
    let unsupported = fractal([0.0; 2], 0.5).with_frequency([40.0; 2]).unwrap();
    assert!(running
        .scene_mut()
        .set_fill_noise(&source, unsupported, None)
        .is_err());
    assert_eq!(running.session().frame(), &before);
    assert_eq!(running.session().publication_context(), publication);
    assert_eq!(running.scene().authored(&source).unwrap(), authored);
}

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
    let authored = scene.authored(&source).unwrap();
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
    let mut running = scene.into_execution().unwrap();
    let before = running.session().frame().clone();
    let publication = running.session().publication_context();
    let authored = running.scene().authored(&source).unwrap();
    assert!(running
        .scene_mut()
        .set_fill_noise(&source, paint([1e-60, 0.5], [0.5; 2]), Some(0.75))
        .is_err());
    assert_eq!(running.session().frame(), &before);
    assert_eq!(running.session().publication_context(), publication);
    assert_eq!(running.scene().authored(&source).unwrap(), authored);
    running
        .scene_mut()
        .set_fill_noise(&source, paint([0.75; 2], [0.75; 2]), None)
        .unwrap();
    assert_eq!(
        running
            .scene()
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
        let mut running = scene.into_execution().unwrap();
        let segment = running.live().play_animation(&animation).unwrap();
        running.live().advance_segment_to(segment, 1.0).unwrap();
        assert_eq!(
            running
                .scene()
                .authored(&source)
                .unwrap()
                .style
                .fill_opacity,
            1.0
        );
        assert_eq!(
            running.scene().effective_manim_color(&source).unwrap(),
            Color::rgb(0.0, 1.0, 0.0)
        );
    }
}
