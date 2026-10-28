use noon::{
    effects::{EffectUpdate, NoiseDissolve},
    AnimationCompositionRequest, AnimationOptions, ExecutionSession, RateFunction, Scene,
    SemanticAnimationCompositionKind, TransformToRequest,
};
use noon_core::SemanticNodeId;

fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}
fn effective(
    session: &ExecutionSession,
    owner: SemanticNodeId,
    attachment: SemanticNodeId,
) -> NoiseDissolve {
    let execution = session.execution_object_id(owner).unwrap();
    session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == execution)
        .unwrap()
        .effects
        .as_ref()
        .unwrap()
        .get(attachment)
        .unwrap()
        .definition
        .as_noise_dissolve()
        .unwrap()
}

#[test]
fn independent_mask_targets_seek_complete_and_retire_exact_generations() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    for (name, seed) in [("first", 7), ("second", 11)] {
        source
            .add_effect(NoiseDissolve::new(seed, 0.08).unwrap(), name)
            .unwrap();
    }
    let a = source.get_effect("first").unwrap();
    let b = source.get_effect("second").unwrap();
    scene.add(&source).unwrap();
    let mut first = source.target_editor().unwrap();
    first
        .set_effect("first", EffectUpdate::default().progress(0.8).softness(0.4))
        .unwrap();
    let mut second = source.target_editor().unwrap();
    second
        .set_effect("second", EffectUpdate::default().progress(0.6))
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            &[
                AnimationCompositionRequest::TransformTo(
                    TransformToRequest::new(&source, &first, options()).method_target(),
                ),
                AnimationCompositionRequest::TransformTo(
                    TransformToRequest::new(&source, &second, options()).method_target(),
                ),
            ],
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .unwrap();
    for time in [0.75, 0.25, 0.9, 0.5] {
        session.seek(time).unwrap();
        let first = effective(&session, source.node_id(), a.node_id());
        let second = effective(&session, source.node_id(), b.node_id());
        assert_eq!(first.progress(), 0.8 * time);
        assert!((first.softness() - (0.08 * (1.0 - time) + 0.4 * time)).abs() < 1e-14);
        assert_eq!(
            (first.seed(), second.seed(), second.softness()),
            (7, 11, 0.08)
        );
        assert_eq!(second.progress(), 0.6 * time);
    }
    session.advance_segment_to(segment, 1.0).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        a.authored_definition()
            .unwrap()
            .as_noise_dissolve()
            .unwrap()
            .progress(),
        0.8
    );
    scene.live(&mut session).remove_effect(&source, &a).unwrap();
    assert_eq!(
        b.authored_definition()
            .unwrap()
            .as_noise_dissolve()
            .unwrap()
            .progress(),
        0.6
    );
    assert!(a.authored_definition().is_err());
    scene
        .live(&mut session)
        .add_effect(&source, NoiseDissolve::default(), "first")
        .unwrap();
    let replacement = source.get_effect("first").unwrap();
    assert_ne!(replacement.node_id(), a.node_id());
    let before = session.frame().clone();
    let context = session.publication_context();
    let revision = scene.revision();
    assert!(scene
        .live(&mut session)
        .set_effect(&source, &a, EffectUpdate::default().progress(0.3))
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        effective(&session, source.node_id(), replacement.node_id()).progress(),
        0.0
    );
}

#[test]
fn explicit_zero_mask_writes_conflict_atomically_and_seed_edits_stay_discrete() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    source
        .add_effect(NoiseDissolve::new(7, 0.08).unwrap(), "mask")
        .unwrap();
    scene.add(&source).unwrap();
    let a = source.target_editor().unwrap();
    let b = source.target_editor().unwrap();
    a.clone()
        .set_effect("mask", EffectUpdate::default().progress(0.8))
        .unwrap();
    b.clone()
        .set_effect("mask", EffectUpdate::default().progress(0.0))
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.frame().clone();
    let context = session.publication_context();
    let revision = scene.revision();
    assert!(scene
        .live(&mut session)
        .declare_and_activate_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            &[
                AnimationCompositionRequest::TransformTo(
                    TransformToRequest::new(&source, &a, options()).method_target()
                ),
                AnimationCompositionRequest::TransformTo(
                    TransformToRequest::new(&source, &b, options()).method_target()
                ),
            ],
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
    assert_eq!(scene.revision(), revision);
    let attachment = source.get_effect("mask").unwrap();
    scene
        .live(&mut session)
        .set_effect(&source, &attachment, EffectUpdate::default().seed(u32::MAX))
        .unwrap();
    assert_eq!(
        effective(&session, source.node_id(), attachment.node_id()).seed(),
        u32::MAX
    );
    let target = scene.live(&mut session).copy_mobject(&source).unwrap();
    scene
        .live(&mut session)
        .set_effect(target.root(), "mask", EffectUpdate::default().seed(0))
        .unwrap();
    let before = session.frame().clone();
    let context = session.publication_context();
    let revision = scene.revision();
    assert!(scene
        .live(&mut session)
        .declare_and_activate_composition(
            &AnimationCompositionRequest::TransformTo(
                TransformToRequest::new(&source, target.root(), options()).method_target()
            ),
            AnimationOptions::new(),
        )
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
    assert_eq!(scene.revision(), revision);
}

#[test]
fn composed_mask_captures_activation_values_and_rejects_rebound_names() {
    let mut scene = Scene::new();
    let leaf = scene.circle(0.4).unwrap();
    let mut family = scene.family(&[(&leaf).into()]).unwrap();
    family
        .add_effect(NoiseDissolve::new(7, 0.08).unwrap(), "mask")
        .unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let target = family.copy_family().unwrap();
    target
        .root()
        .clone()
        .set_effect("mask", EffectUpdate::default().progress(0.8))
        .unwrap();
    let declaration = scene
        .declare_animation(
            noon_core::SemanticAnimationIntent::EffectTargetTo {
                target: family.node_id(),
                target_state: target.root().node_id(),
                method_target: true,
            },
            options(),
        )
        .unwrap();
    let attachment = family.get_effect("mask").unwrap();
    let mut session = scene.execution_session().unwrap();
    let owner = session.execution_object_id(family.node_id()).unwrap();
    let source_plan = session
        .frame()
        .objects
        .iter()
        .find_map(|row| match &row.content {
            noon_core::ObjectContentRef::Composed(source) if row.id == owner => {
                Some(std::sync::Arc::clone(source))
            }
            _ => None,
        })
        .unwrap();
    scene
        .live(&mut session)
        .set_effect(&family, &attachment, EffectUpdate::default().progress(0.4))
        .unwrap();
    let segment = scene
        .live(&mut session)
        .play_animation(&declaration)
        .unwrap();
    session.advance_segment_to(segment, 0.5).unwrap();
    assert!(
        (effective(&session, family.node_id(), attachment.node_id()).progress() - 0.6).abs()
            < 1e-14
    );
    let row = session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == owner)
        .unwrap();
    match &row.content {
        noon_core::ObjectContentRef::Composed(source) => {
            assert!(std::sync::Arc::ptr_eq(&source_plan, source))
        }
        _ => panic!("family keeps its composed source"),
    }
    session.advance_segment_to(segment, 1.0).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        attachment
            .authored_definition()
            .unwrap()
            .as_noise_dissolve()
            .unwrap()
            .progress(),
        0.8
    );
    scene
        .live(&mut session)
        .remove_effect(&family, &attachment)
        .unwrap();
    scene
        .live(&mut session)
        .add_effect(&family, NoiseDissolve::default(), "mask")
        .unwrap();
    assert_ne!(
        family.get_effect("mask").unwrap().node_id(),
        attachment.node_id()
    );
    let before = session.frame().clone();
    let context = session.publication_context();
    let revision = scene.revision();
    assert!(scene
        .live(&mut session)
        .play_animation(&declaration)
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
    assert_eq!(scene.revision(), revision);
}
