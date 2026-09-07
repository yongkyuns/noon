//! Regression for the unmodified AddWaitLaggedStartMap authoring sequence.
//! Python/browser coverage remains in semantic-preview-smoke.mjs; these tests
//! trace the shared native semantic and execution publications, not a mock model.

use super::*;
use crate::Scene;
use noon_core::RateFunction;

fn linear(duration: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Linear)
}

fn trace(live: &LiveSession<'_>, phase: &str) -> PublicationContext {
    let store = live.store.borrow();
    let publication = live.session.publication_context();
    assert_eq!(
        store.scene_revision(),
        publication.scene_revision(),
        "{phase}"
    );
    live.session.require_published_store(&store).unwrap();
    eprintln!(
        "late-family phase={phase} semantic={} execution={} frame={} roots={:?}",
        publication.scene_revision().get(),
        publication.execution_revision().get(),
        publication.frame_epoch().get(),
        store.semantic_family_members_checked(live.root).unwrap(),
    );
    publication
}

fn finish(live: &mut LiveSession<'_>, segment: ExecutionSegment) {
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
}

#[test]
fn late_family_after_succession_enters_lagged_fade_without_premature_membership() {
    let scene = Scene::new();
    let mut left = scene.circle(0.35).unwrap();
    let mut right = scene.circle(0.35).unwrap();
    left.set_translation(-2.0, 0.0).unwrap();
    right.set_translation(2.0, 0.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    trace(&live, "initial");
    // Succession(Wait(0.4), Add(left), Wait(0.6), Add(right)).
    let first = AnimationCompositionRequest::Composition {
        kind: SemanticAnimationCompositionKind::Sequence,
        options: AnimationOptions::new(),
        children: vec![
            AnimationCompositionRequest::Wait { duration: 0.4 },
            AnimationCompositionRequest::Add {
                target: &left,
                options: linear(0.0),
            },
            AnimationCompositionRequest::Wait { duration: 0.6 },
            AnimationCompositionRequest::Add {
                target: &right,
                options: linear(0.0),
            },
        ],
    };
    let first = live
        .declare_and_activate_composition(&first, AnimationOptions::new())
        .unwrap();
    assert_eq!(first.end_time(), 1.0);
    finish(&mut live, first);
    trace(&live, "succession-complete");
    let left_slot = live.session.execution_object_id(left.node_id()).unwrap();
    let right_slot = live.session.execution_object_id(right.node_id()).unwrap();

    let first_square = live
        .create_manim_geometry(crate::ManimGeometryOptions::square(0.7).unwrap())
        .unwrap();
    trace(&live, "first-square-created");
    live.set_translation(&first_square, -0.6, -1.5).unwrap();
    trace(&live, "first-square-shifted");
    let second_square = live
        .create_manim_geometry(crate::ManimGeometryOptions::square(0.7).unwrap())
        .unwrap();
    trace(&live, "second-square-created");
    live.set_translation(&second_square, 0.6, -1.5).unwrap();
    let before_family = trace(&live, "second-square-shifted");
    let mapped = live
        .family(&[
            MobjectFamilyMember::Mobject(&first_square),
            MobjectFamilyMember::Mobject(&second_square),
        ])
        .unwrap();
    let after_family = trace(&live, "group-published");
    assert_eq!(
        after_family.scene_revision(),
        before_family.scene_revision().checked_next().unwrap()
    );
    for square in [&first_square, &second_square] {
        assert!(!live.contains(square).unwrap());
        assert!(live.session.execution_object_id(square.node_id()).is_none());
        assert!(live.effective(square).is_err());
    }
    assert_eq!(live.session.frame().objects.len(), 2);
    // LaggedStartMap(FadeIn, mapped, run_time=2.2, lag_ratio=0.1, rate_func=linear).
    let fade = AnimationCompositionRequest::Composition {
        kind: SemanticAnimationCompositionKind::Parallel,
        options: linear(2.2).lag_ratio(0.1),
        children: [&first_square, &second_square]
            .into_iter()
            .map(|target| AnimationCompositionRequest::Fade {
                target,
                direction: SemanticFadeDirection::In,
                endpoint: FadeEndpoint::default(),
                options: AnimationOptions::new(),
            })
            .collect(),
    };
    let fade = live
        .declare_and_activate_composition(&fade, AnimationOptions::new())
        .unwrap();
    assert_eq!(fade.start_time(), 1.0);
    assert!((fade.end_time() - 3.2).abs() < 1e-12);
    trace(&live, "lagged-fade-activated");
    for square in [&first_square, &second_square] {
        assert!(live.contains(square).unwrap());
        assert_eq!(live.effective(square).unwrap().appearance, 0.0);
    }
    live.advance_segment_to(fade, 2.1).unwrap();
    trace(&live, "lagged-fade-middle");
    let first_appearance = live.effective(&first_square).unwrap().appearance;
    let second_appearance = live.effective(&second_square).unwrap().appearance;
    assert!(
        0.0 < second_appearance && second_appearance < first_appearance && first_appearance < 1.0
    );
    finish(&mut live, fade);
    trace(&live, "lagged-fade-complete");
    assert_eq!(live.session.frame().objects.len(), 4);
    for square in [&first_square, &second_square] {
        assert_eq!(live.effective(square).unwrap().appearance, 1.0);
    }
    assert_eq!(
        live.session.execution_object_id(left.node_id()),
        Some(left_slot)
    );
    assert_eq!(
        live.session.execution_object_id(right.node_id()),
        Some(right_slot)
    );
    let store = live.store.borrow();
    assert_eq!(
        store.semantic_family_members_checked(live.root).unwrap(),
        &[
            left.node_id(),
            right.node_id(),
            first_square.node_id(),
            second_square.node_id()
        ]
    );
    assert_eq!(
        store
            .semantic_family_members_checked(mapped.node_id())
            .unwrap(),
        &[first_square.node_id(), second_square.node_id()]
    );
    assert!(store.node(mapped.node_id()).unwrap().parents().is_empty());
}

#[test]
fn late_family_single_leaf_fades_preserve_unmounted_nested_and_shared_edges() {
    let scene = Scene::new();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let leaf = live
        .create_manim_geometry(crate::ManimGeometryOptions::square(0.7).unwrap())
        .unwrap();
    let first = live.family(&[MobjectFamilyMember::Mobject(&leaf)]).unwrap();
    let second = live.family(&[MobjectFamilyMember::Mobject(&leaf)]).unwrap();
    let nested = live.family(&[MobjectFamilyMember::Family(&first)]).unwrap();
    let identity = leaf.node_id();
    for _ in 0..2 {
        let entering = live
            .declare_and_activate_fade(&leaf, SemanticFadeDirection::In, linear(1.0))
            .unwrap();
        finish(&mut live, entering);
        assert!(live.contains(&leaf).unwrap());
        let exiting = live
            .declare_and_activate_fade(&leaf, SemanticFadeDirection::Out, linear(1.0))
            .unwrap();
        finish(&mut live, exiting);
        trace(&live, "single-leaf-detached");
        assert!(!live.contains(&leaf).unwrap());
        assert_eq!(leaf.node_id(), identity);
        let store = live.store.borrow();
        assert_eq!(
            store.node(identity).unwrap().parents(),
            &[first.node_id(), second.node_id()]
        );
        assert_eq!(
            store
                .semantic_family_members_checked(nested.node_id())
                .unwrap(),
            &[first.node_id()]
        );
    }
}

#[test]
fn late_family_mounted_or_aliased_leaf_fades_still_fail_atomically() {
    for direct_root_edge in [false, true] {
        let mut scene = Scene::new();
        let leaf = scene.square(0.7).unwrap();
        let family = scene.family(&[&leaf]).unwrap();
        scene
            .add_many(&[MobjectFamilyMember::Family(&family)])
            .unwrap();
        if direct_root_edge {
            // Scene.add deliberately restructures existing family paths. Build a
            // valid aliased DAG explicitly to test the fade rejection boundary.
            let mut transaction = SemanticMutationTransaction::new();
            transaction.add_member(scene.root(), leaf.node_id());
            transaction.apply(&mut scene.store().borrow_mut()).unwrap();
        }
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let before = trace(&live, "mounted-before-rejection");
        let before_nodes = live.store.borrow().len();
        for direction in [SemanticFadeDirection::In, SemanticFadeDirection::Out] {
            assert!(live
                .declare_and_activate_fade(&leaf, direction, linear(1.0))
                .is_err());
            assert_eq!(trace(&live, "mounted-after-rejection"), before);
            assert_eq!(live.store.borrow().len(), before_nodes);
            assert_eq!(live.contains(&leaf).unwrap(), direct_root_edge);
            assert!(live.effective(&leaf).is_ok());
        }
    }
}

#[test]
fn late_family_duplicate_fade_admission_still_fails_atomically() {
    let scene = Scene::new();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let leaf = live
        .create_manim_geometry(crate::ManimGeometryOptions::square(0.7).unwrap())
        .unwrap();
    let _family = live.family(&[MobjectFamilyMember::Mobject(&leaf)]).unwrap();
    let before = trace(&live, "duplicate-before");
    let before_nodes = live.store.borrow().len();
    let request = AnimationCompositionRequest::Composition {
        kind: SemanticAnimationCompositionKind::Parallel,
        options: linear(1.0),
        children: (0..2)
            .map(|_| AnimationCompositionRequest::Fade {
                target: &leaf,
                direction: SemanticFadeDirection::In,
                endpoint: FadeEndpoint::default(),
                options: AnimationOptions::new(),
            })
            .collect(),
    };
    assert!(live
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .is_err());
    assert_eq!(trace(&live, "duplicate-after"), before);
    assert_eq!(live.store.borrow().len(), before_nodes);
    assert!(!live.contains(&leaf).unwrap());
}

#[test]
fn late_family_fade_does_not_accept_an_unpublished_semantic_revision() {
    let scene = Scene::new();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let leaf = live
        .create_manim_geometry(crate::ManimGeometryOptions::square(0.7).unwrap())
        .unwrap();
    let _family = live.family(&[MobjectFamilyMember::Mobject(&leaf)]).unwrap();
    let before = trace(&live, "stale-before");
    let _unpublished = scene.circle(0.2).unwrap();
    let changed = live.store.borrow().scene_revision();
    assert_ne!(changed, before.scene_revision());
    assert!(matches!(
        live.declare_and_activate_fade(&leaf, SemanticFadeDirection::In, linear(1.0)),
        Err(LiveSessionError::Activation(
            ExecutionSessionAnimationError::StaleSceneRevision { .. }
        ))
    ));
    assert_eq!(live.session.publication_context(), before);
    assert_eq!(live.store.borrow().scene_revision(), changed);
}
