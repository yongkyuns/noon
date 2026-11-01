use std::sync::Arc;

use super::*;
use noon_core::{AnimationOptions, RateFunction, RevealPhaseMap, SemanticAnimationCompositionKind};

fn shape(context: &CanonicalAuthoringScene) -> noon::Mobject {
    noon::Mobject::from_manim_geometry(
        std::rc::Rc::clone(context.scene.integration_store()),
        noon::ManimGeometryOptions::path(
            noon::VectorPath::new()
                .move_to(noon::Vec2::new(-2.0, 0.0))
                .line_to(noon::Vec2::new(-1.0, 1.0))
                .line_to(noon::Vec2::new(3.0, -1.0)),
        )
        .unwrap(),
    )
    .unwrap()
}

fn child(
    entering_id: Option<ObjectId>,
    target: &noon::Mobject,
    keys: Vec<f64>,
    options: AnimationOptions,
) -> OrdinaryCompositionChild {
    OrdinaryCompositionChild::PhaseReveal {
        entering_id,
        target: target.clone(),
        phase_map: Arc::new(RevealPhaseMap::new(keys).unwrap()),
        options,
    }
}

fn begin(
    context: &mut CanonicalAuthoringScene,
    children: &[OrdinaryCompositionChild],
) -> Result<f64, AuthoringFailure> {
    context.begin_ordinary_mixed_composition(
        SemanticAnimationCompositionKind::Parallel,
        children,
        AnimationOptions::new().rate_func(RateFunction::Linear),
        AnimationOptions::new(),
    )
}

fn complete(context: &mut CanonicalAuthoringScene, end: f64) {
    let player = context.active_live_player().unwrap();
    player.live_advance_segment_to(end).unwrap();
    player.live_complete_segment().unwrap();
}

#[test]
fn phase_reveal_adapter_uses_one_player_and_preserves_existing_wrapper_on_reversal() {
    let mut context = CanonicalAuthoringScene::default();
    let target = shape(&context);
    let id = ObjectId::new(17);
    let options = AnimationOptions::new()
        .run_time(2.0)
        .rate_func(RateFunction::Linear);
    let end = begin(
        &mut context,
        &[child(Some(id), &target, vec![0.0, 0.2, 1.0], options)],
    )
    .unwrap();
    assert_eq!(end, 2.0);
    assert_eq!(context.bindings.get(&id), Some(&target.node_id()));
    assert_eq!(context.identities.get(&target.node_id()), Some(&id));
    let player = context.active_live_player().unwrap();
    let owner = player.ownership_identity();
    assert!(!player.has_required_callbacks());
    player.live_advance_segment_to(0.4).unwrap();
    assert_eq!(player.session_mut_for_test().frame().reveal(0), 0.5);
    complete(&mut context, end);
    assert!(context.live_contains_mobject(&target).unwrap());

    let end = begin(
        &mut context,
        &[child(
            None,
            &target,
            vec![0.0, 0.2, 1.0],
            options.reverse_rate_function(true).remover(true),
        )],
    )
    .unwrap();
    assert_eq!(end, 4.0);
    let player = context.active_live_player().unwrap();
    assert_eq!(player.ownership_identity(), owner);
    player.live_advance_segment_to(3.6).unwrap();
    assert!((player.session_mut_for_test().frame().reveal(0) - 0.5).abs() < 1e-6);
    complete(&mut context, end);
    assert!(!context.live_contains_mobject(&target).unwrap());
    context.live_add_mobject(id, &target).unwrap();
    assert_eq!(context.bindings.len(), 1);
    assert_eq!(context.identities.len(), 1);
    assert_eq!(
        context.active_live_player().unwrap().ownership_identity(),
        owner
    );
}

#[test]
fn phase_reveal_adapter_rejects_mixed_candidate_before_binding_or_player_publication() {
    let mut context = CanonicalAuthoringScene::default();
    let valid = shape(&context);
    let invalid = shape(&context);
    let revision = context.scene.revision();
    let root = context.root_membership_keys().unwrap();
    let options = AnimationOptions::new().rate_func(RateFunction::Linear);
    let children = [
        child(Some(ObjectId::new(7)), &valid, vec![0.0, 0.2, 1.0], options),
        child(Some(ObjectId::new(9)), &invalid, vec![0.0, 1.0], options),
    ];
    assert!(begin(&mut context, &children).is_err());
    assert!(context.player_ownership.is_unstarted());
    assert!(context.bindings.is_empty());
    assert!(context.identities.is_empty());
    assert_eq!(context.scene.revision(), revision);
    assert_eq!(context.root_membership_keys().unwrap(), root);
    let foreign = shape(&CanonicalAuthoringScene::default());
    assert!(begin(
        &mut context,
        &[child(
            Some(ObjectId::new(9)),
            &foreign,
            vec![0.0, 0.2, 1.0],
            options
        )]
    )
    .is_err());
    assert_eq!(context.scene.revision(), revision);
    assert!(context.player_ownership.is_unstarted());
    let end = begin(&mut context, &children[..1]).unwrap();
    complete(&mut context, end);
    assert_eq!(context.bindings.len(), 1);
    assert!(context.live_contains_mobject(&valid).unwrap());
}
