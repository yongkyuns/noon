use super::*;

fn shape(context: &CanonicalAuthoringScene, x: f64) -> noon::Mobject {
    let path = noon::VectorPath::new()
        .move_to(noon::Vec2::new(-1.0, -1.0))
        .line_to(noon::Vec2::new(1.0, -0.5))
        .line_to(noon::Vec2::new(-0.25, 1.0))
        .close();
    let mut object = noon::Mobject::from_manim_geometry(
        std::rc::Rc::clone(context.scene.integration_store()),
        noon::ManimGeometryOptions::path(path).unwrap(),
    )
    .unwrap();
    object.set_translation(x, 0.0).unwrap();
    object
}

fn fixture() -> (
    CanonicalAuthoringScene,
    noon::Mobject,
    noon::MobjectFamily,
    noon::Mobject,
    noon::MobjectFamily,
) {
    let mut context = CanonicalAuthoringScene::default();
    let source_leaf = shape(&context, -2.0);
    let target_leaf = shape(&context, 4.0);
    let source = context.scene.family(&[(&source_leaf).into()]).unwrap();
    let target = context.scene.family(&[(&target_leaf).into()]).unwrap();
    context
        .edit_membership(SceneMembershipBatch {
            kind: SceneMembershipBatchKind::Add,
            members: vec![OwnedSceneMembershipMember::Family(source.clone())],
            bindings: vec![(ObjectId::new(0), source_leaf.clone())],
        })
        .unwrap();
    (context, source_leaf, source, target_leaf, target)
}

fn binding_batch(bindings: Vec<(ObjectId, noon::Mobject)>) -> SceneMembershipBatch {
    SceneMembershipBatch {
        kind: SceneMembershipBatchKind::Add,
        members: vec![],
        bindings,
    }
}

fn begin_matching(
    context: &mut CanonicalAuthoringScene,
    source: &noon::MobjectFamily,
    target: &noon::MobjectFamily,
) -> f64 {
    context
        .begin_ordinary_mixed_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[OrdinaryCompositionChild::MatchingFamilyTransformTo {
                source: source.clone(),
                target_state: target.clone(),
                options: noon_core::AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(noon_core::RateFunction::Linear),
            }],
            noon_core::AnimationOptions::new(),
            noon_core::AnimationOptions::new(),
        )
        .unwrap()
}

fn complete(context: &mut CanonicalAuthoringScene, end: f64) {
    let player = context.active_live_player().unwrap();
    player.live_advance_segment_to(end).unwrap();
    player.live_complete_segment().unwrap();
}

#[test]
fn completed_binding_preserves_matching_identity_getters_and_second_animation() {
    let (mut context, source_leaf, source, target_leaf, target) = fixture();
    let end = begin_matching(&mut context, &source, &target);
    assert!(context.mobject_layout(&target_leaf).is_err());
    complete(&mut context, end);
    assert!(!context.contains_mobject(&source_leaf).unwrap());
    assert!(context.contains_mobject(&target_leaf).unwrap());
    let revision = context.scene.revision();
    let roots = context.root_membership_keys().unwrap();
    let player_id = context.active_live_player().unwrap().ownership_identity();
    context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf.clone())]))
        .unwrap();
    assert_eq!(context.scene.revision(), revision);
    assert_eq!(context.root_membership_keys().unwrap(), roots);
    assert_eq!(
        context.active_live_player().unwrap().ownership_identity(),
        player_id
    );
    assert_eq!(context.bindings[&ObjectId::new(1)], target_leaf.node_id());
    assert_eq!(context.identities[&target_leaf.node_id()], ObjectId::new(1));
    assert!((context.mobject_layout(&target_leaf).unwrap().0 - 4.0).abs() < 1e-6);
    let end = context
        .begin_ordinary_mixed_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[OrdinaryCompositionChild::FamilyIndicate {
                target,
                indication: noon::IndicateOptions::default(),
                options: noon_core::AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(noon_core::RateFunction::ThereAndBack),
            }],
            noon_core::AnimationOptions::new(),
            noon_core::AnimationOptions::new(),
        )
        .unwrap();
    complete(&mut context, end);
    assert!((context.mobject_layout(&target_leaf).unwrap().0 - 4.0).abs() < 1e-6);
    context
        .bind_mobject(ObjectId::new(0), &source_leaf)
        .unwrap();
    assert!(context.contains_mobject(&source_leaf).unwrap());
    assert_eq!(context.identities[&source_leaf.node_id()], ObjectId::new(0));
    assert!((context.mobject_layout(&source_leaf).unwrap().0 + 2.0).abs() < 1e-6);
}

#[test]
fn completed_binding_rejects_pending_or_transferred_player_without_bookkeeping() {
    let (mut context, _, source, target_leaf, target) = fixture();
    let end = begin_matching(&mut context, &source, &target);
    let before = context.bindings.clone();
    assert!(context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf.clone())]))
        .is_err());
    assert_eq!(context.bindings, before);
    assert!(!context.identities.contains_key(&target_leaf.node_id()));
    complete(&mut context, end);
    let player = context.take_execution_player(end, 83).unwrap();
    assert!(context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf.clone())]))
        .is_err());
    assert_eq!(context.bindings, before);
    assert!(context.player_ownership.is_transferred());
    context.return_execution_player(player).unwrap();
    context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf)]))
        .unwrap();
    assert!(context.player_ownership.is_returned());
}

#[test]
fn completed_binding_validates_entire_batch_before_committing_either_registry() {
    let (mut context, source_leaf, source, target_leaf, target) = fixture();
    let unpublished = shape(&context, 9.0);
    let foreign_context = CanonicalAuthoringScene::default();
    let foreign = shape(&foreign_context, 9.0);
    let end = begin_matching(&mut context, &source, &target);
    complete(&mut context, end);
    let bindings = context.bindings.clone();
    let identities = context.identities.clone();
    let revision = context.scene.revision();
    let roots = context.root_membership_keys().unwrap();
    for invalid in [unpublished, foreign, source_leaf, target_leaf.clone()] {
        assert!(context
            .associate_published_mobjects(binding_batch(vec![
                (ObjectId::new(1), target_leaf.clone()),
                (ObjectId::new(2), invalid),
            ]))
            .is_err());
        assert_eq!(context.bindings, bindings);
        assert_eq!(context.identities, identities);
        assert_eq!(context.scene.revision(), revision);
        assert_eq!(context.root_membership_keys().unwrap(), roots);
    }
    context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf.clone())]))
        .unwrap();
    // Repeating the exact association is harmless; assigning another wrapper ID is not.
    context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf.clone())]))
        .unwrap();
    assert!(context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(7), target_leaf)]))
        .is_err());
    assert_eq!(context.bindings.len(), 2);
}

#[test]
fn matching_nested_rate_scopes_preserve_child_defaults_and_outer_duration() {
    use noon_core::{AnimationOptions, RateFunction, SemanticAnimationCompositionKind};
    // Independent values from ManimCE v0.21 smooth at alpha=.25 and smooth(smooth(.25)).
    for (inner, outer, progress) in [
        (RateFunction::Linear, RateFunction::Linear, 0.25),
        (
            RateFunction::Smooth,
            RateFunction::Linear,
            0.07010371654510815,
        ),
        (
            RateFunction::Linear,
            RateFunction::Smooth,
            0.07010371654510815,
        ),
        (
            RateFunction::Smooth,
            RateFunction::Smooth,
            0.0067987788242606885,
        ),
    ] {
        let (mut context, source_leaf, source, target_leaf, target) = fixture();
        let end = context
            .begin_ordinary_mixed_composition(
                SemanticAnimationCompositionKind::Parallel,
                &[OrdinaryCompositionChild::Composition {
                    kind: SemanticAnimationCompositionKind::Parallel,
                    children: vec![OrdinaryCompositionChild::MatchingFamilyTransformTo {
                        source,
                        target_state: target,
                        options: AnimationOptions::new().run_time(3.0).rate_func(inner),
                    }],
                    options: AnimationOptions::new().run_time(2.0).rate_func(outer),
                }],
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new(),
            )
            .unwrap();
        assert!((end - 2.0).abs() < 1e-9);
        context
            .active_live_player()
            .unwrap()
            .live_advance_segment_to(0.5)
            .unwrap();
        // Public layout getters intentionally reject active path morphs.
        // Observe the same canonical frame rows consumed by the renderer.
        let session = context.active_live_player().unwrap().session_mut_for_test();
        assert_eq!(session.painter_order().len(), 1);
        let index = session.painter_order()[0] as usize;
        let actual = f64::from(session.frame().objects[index].transform.translation.x);
        assert!(
            (actual - (-2.0 + 6.0 * progress)).abs() < 1e-5,
            "{inner:?}/{outer:?}: {actual}"
        );
        complete(&mut context, end);
        assert!(!context.contains_mobject(&source_leaf).unwrap());
        assert!(context.contains_mobject(&target_leaf).unwrap());
    }
}
