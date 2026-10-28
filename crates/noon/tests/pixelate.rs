use noon::{
    effects::{EffectRadius, EffectUpdate, Pixelate, Pixels},
    AnimationCompositionRequest, AnimationOptions, ExecutionSession, RateFunction, Scene,
    SemanticAnimationCompositionKind, TransformToRequest,
};
use noon_core::SemanticNodeId;
fn value(
    session: &ExecutionSession,
    owner: SemanticNodeId,
    binding: SemanticNodeId,
) -> EffectRadius {
    let id = session.execution_object_id(owner).unwrap();
    session
        .frame()
        .objects
        .iter()
        .find(|r| r.id == id)
        .unwrap()
        .effects
        .as_ref()
        .unwrap()
        .get(binding)
        .unwrap()
        .definition
        .as_pixelate()
        .unwrap()
        .size()
}
#[test]
fn independent_size_channels_seek_complete_and_retire_exact_generations() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    for (name, size) in [("a", 13.0), ("b", 5.0)] {
        source
            .add_effect(
                Pixelate::new(EffectUpdate::default().size(Pixels(size))).unwrap(),
                name,
            )
            .unwrap();
    }
    let a = source.get_effect("a").unwrap();
    let b = source.get_effect("b").unwrap();
    scene.add(&source).unwrap();
    let mut ta = source.target_editor().unwrap();
    ta.set_effect("a", EffectUpdate::default().size(Pixels(1.0)))
        .unwrap();
    let mut tb = source.target_editor().unwrap();
    tb.set_effect("b", EffectUpdate::default().size(Pixels(9.0)))
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            &[
                AnimationCompositionRequest::TransformTo(
                    TransformToRequest::new(
                        &source,
                        &ta,
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    )
                    .method_target(),
                ),
                AnimationCompositionRequest::TransformTo(
                    TransformToRequest::new(
                        &source,
                        &tb,
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    )
                    .method_target(),
                ),
            ],
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .unwrap();
    for time in [0.75, 0.25, 0.875, 0.5] {
        session.seek(time).unwrap();
        assert_eq!(
            value(&session, source.node_id(), a.node_id()),
            EffectRadius::Pixels(13.0 * (1.0 - time) + time)
        );
        assert_eq!(
            value(&session, source.node_id(), b.node_id()),
            EffectRadius::Pixels(5.0 * (1.0 - time) + 9.0 * time)
        );
    }
    session.advance_segment_to(segment, 1.0).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert!(a.authored_definition().unwrap().is_neutral());
    let saved = scene.live(&mut session).copy_mobject(&source).unwrap();
    scene.live(&mut session).remove_effect(&source, &a).unwrap();
    scene
        .live(&mut session)
        .add_effect(&source, Pixelate::default(), "a")
        .unwrap();
    let replacement = source.get_effect("a").unwrap();
    assert_ne!(a.node_id(), replacement.node_id());
    let before = session.frame().clone();
    let context = session.publication_context();
    let revision = scene.revision();
    assert!(scene
        .live(&mut session)
        .set_effect(&source, &a, EffectUpdate::default().size(Pixels(3.0)))
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
    assert_eq!(scene.revision(), revision);
    scene
        .live(&mut session)
        .become_mobject(&source, saved.root(), Default::default())
        .unwrap();
    assert!(replacement.authored_definition().is_err());
    let restored = source.get_effect("a").unwrap();
    assert!(restored.authored_definition().unwrap().is_neutral());
    assert_eq!(
        value(
            &session,
            source.node_id(),
            source.get_effect("b").unwrap().node_id()
        ),
        EffectRadius::Pixels(9.0)
    );
}
