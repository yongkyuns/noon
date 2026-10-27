use noon::{
    effects::{Blur, EffectUpdate, Pixels},
    AnimationCompositionRequest, AnimationOptions, ExecutionSession, RateFunction, Scene,
    SemanticAnimationCompositionKind, TransformToRequest,
};

fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}

fn radius(session: &ExecutionSession, attachment: noon_core::SemanticNodeId) -> f64 {
    session.frame().objects[0]
        .effects
        .as_ref()
        .unwrap()
        .get(attachment)
        .unwrap()
        .definition
        .radius()
        .value()
}

#[test]
fn ordinary_parallel_targets_drive_two_blurs_and_preserve_order_through_seek_and_removal() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    source.disable_stroke().unwrap();
    source
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(1.0))).unwrap(),
            "first",
        )
        .unwrap();
    source
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "second",
        )
        .unwrap();
    let a = source.get_effect("first").unwrap();
    let b = source.get_effect("second").unwrap();
    scene.add(&source).unwrap();
    let mut first = source.target_editor().unwrap();
    first
        .set_effect("first", EffectUpdate::default().radius(Pixels(5.0)))
        .unwrap();
    let mut second = source.target_editor().unwrap();
    second
        .set_effect("second", EffectUpdate::default().radius(Pixels(10.0)))
        .unwrap();
    let children = [
        AnimationCompositionRequest::TransformTo(
            TransformToRequest::new(&source, &first, options()).method_target(),
        ),
        AnimationCompositionRequest::TransformTo(
            TransformToRequest::new(&source, &second, options()).method_target(),
        ),
    ];
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            &children,
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .unwrap();
    for time in [0.75, 0.25, 1.0, 0.5] {
        session.seek(time).unwrap();
        assert_eq!(radius(&session, a.node_id()), 1.0 + 4.0 * time);
        assert_eq!(radius(&session, b.node_id()), 2.0 + 8.0 * time);
        assert_eq!(
            session.frame().objects[0]
                .effects
                .as_ref()
                .unwrap()
                .attachments(),
            &[a.node_id(), b.node_id()]
        );
    }
    scene
        .live(&mut session)
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        a.authored_definition().unwrap().radius(),
        Pixels(5.0).into()
    );
    assert_eq!(
        b.authored_definition().unwrap().radius(),
        Pixels(10.0).into()
    );
    scene.live(&mut session).remove_effect(&source, &a).unwrap();
    assert_eq!(radius(&session, b.node_id()), 10.0);
    assert_eq!(
        session.frame().objects[0]
            .effects
            .as_ref()
            .unwrap()
            .attachments(),
        &[b.node_id()]
    );
    scene
        .live(&mut session)
        .add_effect(&source, Blur::default(), "first")
        .unwrap();
    let replacement = source.get_effect("first").unwrap();
    assert_ne!(replacement.node_id(), a.node_id());
    assert_eq!(
        session.frame().objects[0]
            .effects
            .as_ref()
            .unwrap()
            .attachments(),
        &[b.node_id(), replacement.node_id()]
    );
    let before = session.publication_context();
    let frame = session.frame().clone();
    assert!(scene
        .live(&mut session)
        .set_effect(&source, &a, EffectUpdate::default().radius(Pixels(7.0)))
        .is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.frame(), &frame);
}
