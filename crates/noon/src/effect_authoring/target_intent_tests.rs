//! Copied target parameter intent through the public coherent publication path.
use super::*;
use crate::{
    AnimationCompositionRequest, AnimationOptions, ExecutionSession, RateFunction,
    TransformToRequest,
};
use noon_core::{EffectParameter, Pixels, SemanticAnimationCompositionKind};

fn fixture() -> (Scene, Mobject) {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    source.disable_stroke().unwrap();
    source.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    source
        .set_glow(EffectUpdate::default().radius(Pixels(3.25)).intensity(0.4))
        .unwrap();
    scene.add(&source).unwrap();
    (scene, source)
}

fn options(duration: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Linear)
}

fn row<'a>(session: &'a ExecutionSession, object: &Mobject) -> &'a noon_runtime::FrameObjectState {
    let id = session.execution_object_id(object.node_id()).unwrap();
    session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == id)
        .unwrap()
}

#[test]
fn copied_target_intent_only_write_has_one_coherent_publication_without_render_dirtiness() {
    let (mut scene, source) = fixture();
    let target = source.target_editor().unwrap();
    scene.add(&target).unwrap();
    let target_effect = target.get_effect("glow").unwrap().node_id();
    let source_effect = source.get_effect("glow").unwrap().node_id();
    let mut session = scene.execution_session().unwrap();
    session.take_renderer_publication();
    let before = session.publication_context();
    let objects = session.frame().objects.clone();

    scene
        .live(&mut session)
        .set_glow(&target, EffectUpdate::default().intensity(0.4))
        .unwrap();
    assert_eq!(
        scene.revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(
        session.publication_context().scene_revision(),
        scene.revision()
    );
    assert_eq!(session.frame().objects, objects);
    assert!(session.take_renderer_publication().changes().is_empty());
    let copy = scene
        .integration_store()
        .borrow()
        .semantic_effect_state(target_effect)
        .unwrap()
        .copy_provenance()
        .unwrap();
    assert_eq!(copy.source_attachment(), source_effect);
    assert!(copy.writes_parameter(EffectParameter::Intensity));
    assert!(!copy.writes_parameter(EffectParameter::Radius));

    let committed = session.publication_context();
    scene
        .live(&mut session)
        .set_glow(&target, EffectUpdate::default().intensity(0.4))
        .unwrap();
    assert_eq!(session.publication_context(), committed);

    let mut invalid = SemanticMutationTransaction::new();
    invalid.update_effect(target_effect, EffectUpdate::default().radius(Pixels(3.25)));
    invalid.update_effect(source_effect, EffectUpdate::default().intensity(-1.0));
    assert!(scene.live(&mut session).apply(invalid).is_err());
    assert_eq!(session.publication_context(), committed);
    assert_eq!(session.frame().objects, objects);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_effect_state(target_effect)
            .unwrap()
            .copy_provenance(),
        Some(copy),
        "a rejected value suffix must not publish an intent-only prefix"
    );
}

#[test]
fn copied_motion_and_radius_target_preserves_later_intensity_during_parallel_pulse() {
    for pulse_first in [false, true] {
        let (scene, source) = fixture();
        let mut target = source.target_editor().unwrap();
        target.shift(2.0, 0.0).unwrap();
        target
            .set_glow(EffectUpdate::default().radius(Pixels(6.5)))
            .unwrap();
        let mut raised = source.target_editor().unwrap();
        raised
            .set_glow(EffectUpdate::default().intensity(1.2))
            .unwrap();
        let raise = scene
            .declare_transform_to(&source, &raised, options(1.0))
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let first = scene.live(&mut session).play_animation(&raise).unwrap();
        scene
            .live(&mut session)
            .advance_segment_to(first, first.end_time())
            .unwrap();
        scene.live(&mut session).complete_segment(first).unwrap();

        let pulse = AnimationCompositionRequest::GlowPulse {
            target: &source,
            intensity: 2.0,
            options: AnimationOptions::new().run_time(0.6),
        };
        let full_target = AnimationCompositionRequest::TransformTo(TransformToRequest::new(
            &source,
            &target,
            options(0.6),
        ));
        let full_children = if pulse_first {
            vec![pulse.clone(), full_target]
        } else {
            vec![full_target, pulse.clone()]
        };
        let before = session.publication_context();
        let error = scene
            .live(&mut session)
            .declare_and_activate_animation_composition(
                SemanticAnimationCompositionKind::Parallel,
                &full_children,
                AnimationOptions::new(),
                AnimationOptions::new(),
            )
            .unwrap_err();
        assert!(
            format!("{error:?}").contains("GlowDriverConflict"),
            "{error:?}"
        );
        assert_eq!(session.publication_context(), before);
        assert_eq!(scene.revision(), before.scene_revision());

        let motion = AnimationCompositionRequest::TransformTo(
            TransformToRequest::new(&source, &target, options(0.6)).method_target(),
        );
        let children = if pulse_first {
            vec![pulse, motion]
        } else {
            vec![motion, pulse]
        };
        let segment = scene
            .live(&mut session)
            .declare_and_activate_animation_composition(
                SemanticAnimationCompositionKind::Parallel,
                &children,
                AnimationOptions::new(),
                AnimationOptions::new(),
            )
            .unwrap();
        scene
            .live(&mut session)
            .advance_segment_to(segment, segment.end_time() - 0.3)
            .unwrap();
        let middle = row(&session, &source);
        assert!(
            (only_effect(middle.effects.as_ref().unwrap())
                .definition
                .as_glow()
                .unwrap()
                .intensity()
                - 2.0)
                .abs()
                < 1e-12
        );
        assert!(
            (only_effect(middle.effects.as_ref().unwrap())
                .definition
                .radius()
                .value()
                - 4.875)
                .abs()
                < 1e-12
        );
        assert!((middle.transform.translation.x - 1.0).abs() < 1e-6);
        scene
            .live(&mut session)
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        let final_row = row(&session, &source);
        assert_eq!(
            only_effect(final_row.effects.as_ref().unwrap())
                .definition
                .as_glow()
                .unwrap()
                .intensity(),
            1.2
        );
        assert_eq!(
            only_effect(final_row.effects.as_ref().unwrap())
                .definition
                .radius(),
            Pixels(6.5).into()
        );
        assert_eq!(final_row.transform.translation.x, 2.0);
    }
}

#[test]
fn explicit_same_valued_copied_intensity_still_conflicts_with_pulse_atomically() {
    for pulse_first in [false, true] {
        let (scene, source) = fixture();
        let mut target = source.target_editor().unwrap();
        target
            .set_glow(EffectUpdate::default().intensity(0.4))
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let frame = session.frame().clone();
        let pulse = AnimationCompositionRequest::GlowPulse {
            target: &source,
            intensity: 2.0,
            options: AnimationOptions::new(),
        };
        let explicit = AnimationCompositionRequest::TransformTo(TransformToRequest::new(
            &source,
            &target,
            options(1.0),
        ));
        let children = if pulse_first {
            vec![pulse, explicit]
        } else {
            vec![explicit, pulse]
        };
        let error = scene
            .live(&mut session)
            .declare_and_activate_animation_composition(
                SemanticAnimationCompositionKind::Parallel,
                &children,
                AnimationOptions::new(),
                AnimationOptions::new(),
            )
            .unwrap_err();
        assert!(
            format!("{error:?}").contains("GlowDriverConflict"),
            "{error:?}"
        );
        assert_eq!(session.publication_context(), before);
        assert_eq!(scene.revision(), before.scene_revision());
        assert_eq!(session.frame(), &frame);
    }
}
