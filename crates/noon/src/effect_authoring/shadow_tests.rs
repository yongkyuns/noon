use super::*;
use crate::{
    effects::{EffectOffset, Pixels, Shadow},
    AnimationOptions, Color, RateFunction, TransformToRequest,
};

#[test]
fn shadow_channels_capture_complete_seek_and_retire_through_one_attachment() {
    let mut scene = Scene::new();
    let source = scene.circle(0.4).unwrap();
    scene
        .add_effect(
            &source,
            Shadow::new(
                EffectUpdate::default()
                    .radius(Pixels(1.25))
                    .offset(EffectOffset::Pixels([2.25, -3.5])),
            )
            .unwrap(),
            "depth",
        )
        .unwrap();
    scene.add(&source).unwrap();
    let binding = source.get_effect("depth").unwrap();
    let mut target = source.target_editor().unwrap();
    target
        .set_effect(
            "depth",
            EffectUpdate::default()
                .radius(Pixels(5.25))
                .offset(EffectOffset::Pixels([-5.75, 4.5])),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    // An offset/radius-only target must preserve an independent live color edit.
    let color = Color::rgba(0.3, 0.6, 0.9, 0.4);
    scene
        .live(&mut session)
        .set_effect(&source, "depth", EffectUpdate::default().color(color))
        .unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[TransformToRequest::new(
                &source,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .method_target()],
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .unwrap();
    for time in [0.0, 0.75, 1.0, 0.25, 0.5] {
        session.seek(time).unwrap();
        let value = session.frame().objects[0].effect.as_ref().unwrap();
        assert_eq!(value.attachment, binding.node_id());
        let shadow = value.definition.as_shadow().unwrap();
        assert_eq!(
            shadow.offset(),
            EffectOffset::Pixels([2.25 - 8.0 * time, -3.5 + 8.0 * time])
        );
        assert_eq!(shadow.radius(), Pixels(1.25 + 4.0 * time).into());
        assert_eq!(shadow.color(), color);
    }
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    let authored = binding.authored_definition().unwrap().as_shadow().unwrap();
    assert_eq!(authored.offset(), EffectOffset::Pixels([-5.75, 4.5]));
    assert_eq!(authored.color(), color);
    let context = session.publication_context();
    assert!(scene
        .live(&mut session)
        .set_effect(&source, "depth", EffectUpdate::default().intensity(1.0))
        .is_err());
    assert_eq!(session.publication_context(), context);
    session.take_renderer_publication();
    scene
        .live(&mut session)
        .set_effect(
            &source,
            "depth",
            EffectUpdate::default().offset(EffectOffset::Pixels([0.25, -0.5])),
        )
        .unwrap();
    assert_eq!(
        session
            .take_renderer_publication()
            .changes()
            .object_indices(),
        &[0]
    );
    scene
        .live(&mut session)
        .remove_effect(&source, "depth")
        .unwrap();
    assert!(binding.authored_definition().is_err());
    assert!(session.frame().objects[0].effect.is_none());
}

#[test]
fn an_explicit_unchanged_offset_still_owns_its_method_target_channel() {
    let mut scene = Scene::new();
    let source = scene.circle(0.4).unwrap();
    scene
        .add_effect(
            &source,
            Shadow::new(EffectUpdate::default().offset([0.25, -0.5])).unwrap(),
            "depth",
        )
        .unwrap();
    scene.add(&source).unwrap();
    let mut target = source.target_editor().unwrap();
    target
        .set_effect("depth", EffectUpdate::default().offset([0.25, -0.5]))
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .set_effect(
            &source,
            "depth",
            EffectUpdate::default().offset([0.75, 1.0]),
        )
        .unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[TransformToRequest::new(
                &source,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .method_target()],
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let value = session.frame().objects[0]
        .effect
        .as_ref()
        .unwrap()
        .definition
        .as_shadow()
        .unwrap();
    assert_eq!(value.offset(), EffectOffset::Scene([0.5, 0.25]));
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        source
            .get_effect("depth")
            .unwrap()
            .authored_definition()
            .unwrap()
            .as_shadow()
            .unwrap()
            .offset(),
        EffectOffset::Scene([0.25, -0.5])
    );
}
