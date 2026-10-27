//! Blur reaches the ordinary transaction, timeline and completion owners.
use super::*;
use crate::{
    effects::{Blur, Pixels},
    AnimationOptions, RateFunction,
};

fn radius(session: &crate::ExecutionSession) -> f64 {
    only_effect(session.frame().objects[0].effects.as_ref().unwrap())
        .definition
        .as_blur()
        .unwrap()
        .radius()
        .value()
}

#[test]
fn blur_animation_completion_seek_and_lifetime_use_the_existing_row() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    source.disable_stroke().unwrap();
    source.set_fill(0.2, 0.4, 0.8, 0.6).unwrap();
    source
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(1.25))).unwrap(),
            "soften",
        )
        .unwrap();
    scene.add(&source).unwrap();
    let original = source.get_effect("soften").unwrap();
    let mut target = source.target_editor().unwrap();
    target
        .set_effect("soften", EffectUpdate::default().radius(Pixels(5.25)))
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
    assert_eq!(radius(&session), 1.25);
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    for time in [0.75, 0.25, 1.0, 0.5] {
        session.seek(time).unwrap();
        assert_eq!(radius(&session), 1.25 + 4.0 * time);
        assert_eq!(
            session.frame().objects[0]
                .effects
                .as_ref()
                .unwrap()
                .get(original.node_id())
                .unwrap()
                .attachment,
            original.node_id()
        );
    }
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        original.authored_definition().unwrap().radius(),
        Pixels(5.25).into()
    );
    session.take_renderer_publication();
    scene
        .live(&mut session)
        .set_effect(
            &source,
            &original,
            EffectUpdate::default().radius(Pixels(0.0)),
        )
        .unwrap();
    assert_eq!(radius(&session), 0.0);
    let publication = session.take_renderer_publication();
    assert_eq!(publication.changes().object_indices(), &[0]);
    scene
        .live(&mut session)
        .remove_effect(&source, &original)
        .unwrap();
    assert!(session.frame().objects[0].effects.is_none());
    assert!(original.authored_definition().is_err());
    scene
        .live(&mut session)
        .add_effect(&source, Blur::default(), "soften")
        .unwrap();
    assert_ne!(
        source.get_effect("soften").unwrap().node_id(),
        original.node_id()
    );
    assert!(scene
        .live(&mut session)
        .set_effect(&source, &original, EffectUpdate::default().radius(1.0))
        .is_err());
}

#[test]
fn blur_rejects_glow_parameters_without_changing_publication() {
    let mut scene = Scene::new();
    let source = scene.circle(0.4).unwrap();
    scene
        .add_effect(&source, Blur::default(), "soften")
        .unwrap();
    scene.add(&source).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.publication_context();
    let revision = scene.revision();
    assert!(scene
        .live(&mut session)
        .set_effect(&source, "soften", EffectUpdate::default().intensity(0.0))
        .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(session.publication_context(), before);
}

#[test]
fn glow_conveniences_do_not_reinterpret_a_blur_named_glow() {
    let mut scene = Scene::new();
    let source = scene.circle(0.4).unwrap();
    scene.add_effect(&source, Blur::default(), "glow").unwrap();
    scene.add(&source).unwrap();
    let original = source.get_effect("glow").unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.publication_context();
    assert!(scene
        .live(&mut session)
        .set_glow(&source, EffectUpdate::default().radius(0.4))
        .is_err());
    assert!(scene.live(&mut session).remove_glow(&source).is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(
        source.get_effect("glow").unwrap().node_id(),
        original.node_id()
    );
    assert!(original.authored_definition().unwrap().as_blur().is_some());
}
