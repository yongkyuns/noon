//! Acceptance tests for lifecycle animations beside an unrelated host updater.
//! These exercise the shared execution path used by Python, without a browser.

use std::{cell::RefCell, rc::Rc};

use noon::integration::HostCallbackId;
use noon::{
    AnimationCompositionRequest as Request, AnimationOptions, FadeEndpoint, Mobject, RateFunction,
    RustHostCallbackTable, Scene, SemanticAnimationCompositionKind as Kind, SemanticFadeDirection,
};
use noon_core::SemanticMutationTransaction;

const MOVE: HostCallbackId = HostCallbackId::new(501);

type CallbackTimes = Rc<RefCell<Vec<f64>>>;

fn install_motion(
    scene: &mut Scene,
    moving: &Mobject,
    times: &CallbackTimes,
) -> RustHostCallbackTable {
    let mut callbacks = RustHostCallbackTable::new();
    let times = Rc::clone(times);
    callbacks
        .insert(MOVE, move |context| {
            let mut transform = context.target_state().transform;
            transform.translation.x = context.time() as f32;
            context
                .set_target_transform(transform)
                .map_err(std::io::Error::other)?;
            times.borrow_mut().push(context.time());
            Ok::<(), std::io::Error>(())
        })
        .unwrap();
    callbacks
        .add_updater(
            &mut scene.integration_store().borrow_mut(),
            moving.node_id(),
            MOVE,
            0.0,
            None,
        )
        .unwrap();
    callbacks
}

fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}

#[derive(Clone, Copy, Debug)]
enum Lifecycle {
    FadeIn,
    FadeOut,
    Create,
    Uncreate,
}

impl Lifecycle {
    fn removes(self) -> bool {
        matches!(self, Self::FadeOut | Self::Uncreate)
    }

    fn request(self, target: &Mobject) -> Request<'_> {
        match self {
            Self::FadeIn | Self::FadeOut => Request::Fade {
                target,
                direction: if self.removes() {
                    SemanticFadeDirection::Out
                } else {
                    SemanticFadeDirection::In
                },
                endpoint: FadeEndpoint::default(),
                options: options(),
            },
            Self::Create => Request::Create {
                target,
                options: options(),
            },
            Self::Uncreate => Request::Uncreate {
                target,
                options: options(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ActivationRoute {
    Direct,
    Leaf,
    Nested,
}

fn qualify_unrelated_callback(lifecycle: Lifecycle) {
    // Direct, leaf-composition and nested requests must preserve the same semantics.
    for route in [
        ActivationRoute::Direct,
        ActivationRoute::Leaf,
        ActivationRoute::Nested,
    ] {
        let nested = matches!(route, ActivationRoute::Nested);
        let mut scene = Scene::new();
        let moving = scene.circle(0.25).unwrap();
        let target = scene.square(1.0).unwrap();
        scene.add(&moving).unwrap();
        if lifecycle.removes() {
            scene.add(&target).unwrap();
        }
        let times = Rc::new(RefCell::new(Vec::new()));
        let mut callbacks = install_motion(&mut scene, &moving, &times);
        let mut execution = scene.execution_session().unwrap();
        let identity = execution.runtime_identity();
        callbacks.advance_to(&mut execution, 0.0).unwrap();
        assert_eq!(*times.borrow(), [0.0]);

        let leaf = lifecycle.request(&target);
        let request = if nested {
            Request::Composition {
                kind: Kind::Parallel,
                children: vec![leaf],
                options: options(),
            }
        } else {
            leaf
        };
        let result = if matches!(route, ActivationRoute::Direct) {
            let mut live = scene.live(&mut execution);
            match lifecycle {
                Lifecycle::FadeIn | Lifecycle::FadeOut => live
                    .declare_and_activate_fade_with_endpoint(
                        &target,
                        if lifecycle.removes() {
                            SemanticFadeDirection::Out
                        } else {
                            SemanticFadeDirection::In
                        },
                        FadeEndpoint::default(),
                        options(),
                    ),
                Lifecycle::Create => live.declare_and_activate_create(&target, options()),
                Lifecycle::Uncreate => live.declare_and_activate_uncreate(&target, options()),
            }
        } else {
            scene
                .live(&mut execution)
                .declare_and_activate_composition(&request, AnimationOptions::new())
        };
        let segment = result.unwrap_or_else(|error| {
            panic!("{lifecycle:?}, route={route:?}: unrelated updater blocked admission: {error}")
        });
        assert!(scene.live(&mut execution).contains(&target).unwrap());
        for time in [0.25, 0.5, 1.0] {
            callbacks
                .advance_segment_to(&mut execution, segment, time)
                .unwrap();
            let moving_state = scene.live(&mut execution).effective(&moving).unwrap();
            assert_eq!(moving_state.transform.translation.x, time as f32);
            assert_eq!(execution.frame().time, time);
            assert_eq!(execution.runtime_identity(), identity);
            assert_eq!(execution.frame().objects.len(), 2);
            // The fixture deliberately binds the updater first and the lifecycle
            // target second. Read the renderer's actual appearance/reveal value.
            let progress = match lifecycle {
                Lifecycle::FadeIn | Lifecycle::FadeOut => execution.frame().objects[1].appearance,
                Lifecycle::Create | Lifecycle::Uncreate => execution.frame().reveal(1),
            };
            let expected = if lifecycle.removes() {
                1.0 - time
            } else {
                time
            };
            assert!((progress - expected as f32).abs() < 1.0e-6);
        }
        scene
            .live(&mut execution)
            .complete_segment(segment)
            .unwrap();
        assert_eq!(
            scene.live(&mut execution).contains(&target).unwrap(),
            !lifecycle.removes()
        );
        assert!(scene.live(&mut execution).contains(&moving).unwrap());

        let hold = scene.live(&mut execution).wait_segment(0.5).unwrap();
        callbacks
            .advance_segment_to(&mut execution, hold, 1.5)
            .unwrap();
        scene.live(&mut execution).complete_segment(hold).unwrap();
        assert_eq!(
            scene
                .live(&mut execution)
                .effective(&moving)
                .unwrap()
                .transform
                .translation
                .x,
            1.5
        );
        assert_eq!(times.borrow().last().copied(), Some(1.5));
        assert_eq!(execution.runtime_identity(), identity);
    }
}

#[test]
fn unrelated_callback_does_not_block_fade_in() {
    qualify_unrelated_callback(Lifecycle::FadeIn);
}

#[test]
fn unrelated_callback_does_not_block_fade_out() {
    qualify_unrelated_callback(Lifecycle::FadeOut);
}

#[test]
fn unrelated_callback_does_not_block_create() {
    qualify_unrelated_callback(Lifecycle::Create);
}

#[test]
fn unrelated_callback_does_not_block_uncreate() {
    qualify_unrelated_callback(Lifecycle::Uncreate);
}

#[test]
fn removed_callback_history_does_not_block_a_later_fade() {
    let mut scene = Scene::new();
    let moving = scene.circle(0.25).unwrap();
    let target = scene.square(1.0).unwrap();
    scene.add(&moving).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = install_motion(&mut scene, &moving, &times);
    let mut execution = scene.execution_session().unwrap();
    let first = scene.live(&mut execution).wait_segment(1.0).unwrap();
    callbacks
        .advance_segment_to(&mut execution, first, 1.0)
        .unwrap();
    scene.live(&mut execution).complete_segment(first).unwrap();

    let mut removal = SemanticMutationTransaction::new();
    removal.remove_updater(moving.node_id(), MOVE, 1.0);
    scene.live(&mut execution).apply(removal).unwrap();
    let call_count = times.borrow().len();
    let frozen = scene.live(&mut execution).effective(&moving).unwrap();
    let segment = scene
        .live(&mut execution)
        .declare_and_activate_composition(
            &Lifecycle::FadeIn.request(&target),
            AnimationOptions::new(),
        )
        .expect("completed callback history must not forbid unrelated lifecycle admission");
    for time in [1.5, 2.0] {
        callbacks
            .advance_segment_to(&mut execution, segment, time)
            .unwrap();
        assert_eq!(times.borrow().len(), call_count);
        assert_eq!(
            scene
                .live(&mut execution)
                .effective(&moving)
                .unwrap()
                .transform,
            frozen.transform
        );
    }
    scene
        .live(&mut execution)
        .complete_segment(segment)
        .unwrap();
    assert!(scene.live(&mut execution).contains(&target).unwrap());
}

#[test]
fn rejected_composition_preserves_membership_and_callback_execution() {
    let mut scene = Scene::new();
    let moving = scene.circle(0.25).unwrap();
    let target = scene.square(1.0).unwrap();
    let foreign = Scene::new().square(1.0).unwrap();
    scene.add(&moving).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = install_motion(&mut scene, &moving, &times);
    let mut execution = scene.execution_session().unwrap();
    callbacks.advance_to(&mut execution, 0.0).unwrap();
    let before = execution.publication_context();
    let before_calls = times.borrow().clone();
    let before_objects = execution.frame().objects.clone();
    let request = Request::Composition {
        kind: Kind::Parallel,
        children: vec![
            Lifecycle::FadeIn.request(&target),
            Lifecycle::Create.request(&foreign),
        ],
        options: options(),
    };
    assert!(scene
        .live(&mut execution)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .is_err());
    assert_eq!(execution.publication_context(), before);
    assert_eq!(scene.revision(), before.scene_revision());
    assert_eq!(execution.frame().objects, before_objects);
    assert_eq!(*times.borrow(), before_calls);
    assert!(!scene.live(&mut execution).contains(&target).unwrap());

    // A rejected request must not consume or corrupt the next callback phase.
    let hold = scene.live(&mut execution).wait_segment(0.5).unwrap();
    callbacks
        .advance_segment_to(&mut execution, hold, 0.5)
        .unwrap();
    scene.live(&mut execution).complete_segment(hold).unwrap();
    assert_eq!(times.borrow().last().copied(), Some(0.5));
    assert_eq!(
        scene
            .live(&mut execution)
            .effective(&moving)
            .unwrap()
            .transform
            .translation
            .x,
        0.5
    );
}

#[test]
fn own_current_or_future_updater_is_rejected_before_any_admission() {
    for lifecycle in [
        Lifecycle::FadeIn,
        Lifecycle::FadeOut,
        Lifecycle::Create,
        Lifecycle::Uncreate,
    ] {
        for active_from in [0.0, 0.5, 2.0] {
            let mut scene = Scene::new();
            let target = scene.square(1.0).unwrap();
            if lifecycle.removes() {
                scene.add(&target).unwrap();
            }
            let mut tx = SemanticMutationTransaction::new();
            tx.add_updater(target.node_id(), MOVE, active_from, None);
            tx.apply(&mut scene.integration_store().borrow_mut())
                .unwrap();
            let mut execution = scene.execution_session().unwrap();
            let before = execution.publication_context();
            let objects = execution.frame().objects.clone();
            let error = scene
                .live(&mut execution)
                .declare_and_activate_composition(
                    &lifecycle.request(&target),
                    AnimationOptions::new(),
                )
                .unwrap_err();
            assert!(error.to_string().contains("host updaters"), "{error}");
            assert_eq!(execution.publication_context(), before);
            assert_eq!(scene.revision(), before.scene_revision());
            assert_eq!(execution.frame().objects, objects);
            assert_eq!(
                scene.live(&mut execution).contains(&target).unwrap(),
                lifecycle.removes()
            );
        }
    }
}

#[test]
fn family_fade_coexists_with_an_unrelated_updater() {
    for direction in [SemanticFadeDirection::In, SemanticFadeDirection::Out] {
        let mut scene = Scene::new();
        let moving = scene.circle(0.25).unwrap();
        let a = scene.square(1.0).unwrap();
        let b = scene.square(0.5).unwrap();
        let nested = scene.family(&[(&b).into()]).unwrap();
        let family = scene.family(&[(&a).into(), (&nested).into()]).unwrap();
        scene.add(&moving).unwrap();
        if direction == SemanticFadeDirection::Out {
            scene.add_many(&[(&family).into()]).unwrap();
        }
        let times = Rc::new(RefCell::new(Vec::new()));
        let mut callbacks = install_motion(&mut scene, &moving, &times);
        let mut execution = scene.execution_session().unwrap();
        callbacks.advance_to(&mut execution, 0.0).unwrap();
        let identity = execution.runtime_identity();
        let segment = scene
            .live(&mut execution)
            .declare_and_activate_composition(
                &Request::FamilyFade {
                    target: &family,
                    direction,
                    options: options(),
                },
                AnimationOptions::new(),
            )
            .unwrap();
        callbacks
            .advance_segment_to(&mut execution, segment, 0.5)
            .unwrap();
        assert_eq!(execution.frame().objects.len(), 3);
        assert_eq!(execution.frame().objects[0].transform.translation.x, 0.5);
        assert_eq!(execution.frame().objects[1].appearance, 0.5);
        assert_eq!(execution.frame().objects[2].appearance, 0.5);
        callbacks
            .advance_segment_to(&mut execution, segment, 1.0)
            .unwrap();
        scene
            .live(&mut execution)
            .complete_segment(segment)
            .unwrap();
        assert_eq!(
            execution.frame().is_present(1),
            direction == SemanticFadeDirection::In
        );
        assert_eq!(
            execution.frame().is_present(2),
            direction == SemanticFadeDirection::In
        );
        let hold = scene.live(&mut execution).wait_segment(0.5).unwrap();
        callbacks
            .advance_segment_to(&mut execution, hold, 1.5)
            .unwrap();
        scene.live(&mut execution).complete_segment(hold).unwrap();
        assert_eq!(execution.frame().objects[0].transform.translation.x, 1.5);
        assert_eq!(execution.runtime_identity(), identity);
    }
}

#[test]
fn family_or_descendant_updaters_are_rejected_without_partial_admission() {
    for callback_target in 0..3 {
        let mut scene = Scene::new();
        let leaf = scene.square(1.0).unwrap();
        let nested = scene.family(&[(&leaf).into()]).unwrap();
        let family = scene.family(&[(&nested).into()]).unwrap();
        let target = [family.node_id(), nested.node_id(), leaf.node_id()][callback_target];
        let mut tx = SemanticMutationTransaction::new();
        tx.add_updater(target, MOVE, 0.5, None);
        tx.apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let mut execution = scene.execution_session().unwrap();
        let before = execution.publication_context();
        let error = scene
            .live(&mut execution)
            .declare_and_activate_composition(
                &Request::FamilyFade {
                    target: &family,
                    direction: SemanticFadeDirection::In,
                    options: options(),
                },
                AnimationOptions::new(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("host updaters"), "{error}");
        assert_eq!(execution.publication_context(), before);
        assert_eq!(scene.revision(), before.scene_revision());
        assert!(execution.frame().objects.is_empty());
    }
}

#[test]
fn pending_callback_excludes_lifecycle_admission_and_can_still_commit() {
    let mut scene = Scene::new();
    let moving = scene.circle(0.25).unwrap();
    let target = scene.square(1.0).unwrap();
    scene.add(&moving).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = install_motion(&mut scene, &moving, &times);
    let mut execution = scene.execution_session().unwrap();
    let overlay = match execution.advance_to_callback_barrier(0.0).unwrap() {
        noon::integration::CallbackAdvance::HostRequired { overlay, .. } => overlay,
        noon::integration::CallbackAdvance::Ready(_) => {
            panic!("expected an unpublished callback phase")
        }
    };
    let token = execution.pending_callback_token();
    let before = execution.publication_context();
    let frame = execution.frame().clone();
    let error = scene
        .live(&mut execution)
        .declare_and_activate_composition(
            &Lifecycle::FadeIn.request(&target),
            AnimationOptions::new(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        noon::LiveSessionError::Activation(
            noon::ExecutionSessionAnimationError::RequiredCallbackPending
        )
    ));
    assert_eq!(execution.pending_callback_token(), token);
    assert_eq!(execution.publication_context(), before);
    assert_eq!(scene.revision(), before.scene_revision());
    assert_eq!(execution.frame(), &frame);
    assert!(!scene.live(&mut execution).contains(&target).unwrap());
    execution
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    let segment = scene
        .live(&mut execution)
        .declare_and_activate_composition(
            &Lifecycle::FadeIn.request(&target),
            AnimationOptions::new(),
        )
        .unwrap();
    callbacks
        .advance_segment_to(&mut execution, segment, 1.0)
        .unwrap();
    scene
        .live(&mut execution)
        .complete_segment(segment)
        .unwrap();
    assert_eq!(execution.frame().objects[0].transform.translation.x, 1.0);
    assert!(scene.live(&mut execution).contains(&target).unwrap());
}

#[test]
fn later_same_store_failure_rolls_back_staged_admission_and_allows_retry() {
    let mut scene = Scene::new();
    let moving = scene.circle(0.25).unwrap();
    let target = scene.square(1.0).unwrap();
    let attached = scene.square(0.5).unwrap();
    scene.add(&moving).unwrap();
    scene.add(&attached).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = install_motion(&mut scene, &moving, &times);
    let mut execution = scene.execution_session().unwrap();
    callbacks.advance_to(&mut execution, 0.0).unwrap();
    let before = execution.publication_context();
    let frame = execution.frame().clone();
    let calls = times.borrow().clone();
    let nodes = scene.integration_store().borrow().len();
    let request = Request::Composition {
        kind: Kind::Parallel,
        children: vec![
            Lifecycle::FadeIn.request(&target),
            Lifecycle::FadeIn.request(&attached),
        ],
        options: options(),
    };
    let error = scene
        .live(&mut execution)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap_err();
    assert!(error.to_string().contains("detached"), "{error}");
    assert_eq!(execution.publication_context(), before);
    assert_eq!(scene.revision(), before.scene_revision());
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(execution.frame(), &frame);
    assert_eq!(*times.borrow(), calls);
    assert!(!scene.live(&mut execution).contains(&target).unwrap());
    let segment = scene
        .live(&mut execution)
        .declare_and_activate_composition(
            &Lifecycle::FadeIn.request(&target),
            AnimationOptions::new(),
        )
        .unwrap();
    callbacks
        .advance_segment_to(&mut execution, segment, 1.0)
        .unwrap();
    scene
        .live(&mut execution)
        .complete_segment(segment)
        .unwrap();
    assert!(scene.live(&mut execution).contains(&target).unwrap());
    assert!(scene.live(&mut execution).contains(&attached).unwrap());
    assert_eq!(times.borrow().last().copied(), Some(1.0));
}

#[test]
fn unrelated_future_activation_is_not_skipped_by_lifecycle_progression() {
    let mut scene = Scene::new();
    let moving = scene.circle(0.25).unwrap();
    let target = scene.square(1.0).unwrap();
    scene.add(&moving).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = install_motion(&mut scene, &moving, &times);
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_updater(moving.node_id(), MOVE, 0.0);
    tx.apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    callbacks
        .add_updater(
            &mut scene.integration_store().borrow_mut(),
            moving.node_id(),
            MOVE,
            0.5,
            None,
        )
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    callbacks.advance_to(&mut execution, 0.0).unwrap();
    assert!(times.borrow().is_empty());
    let segment = scene
        .live(&mut execution)
        .declare_and_activate_composition(
            &Lifecycle::FadeIn.request(&target),
            AnimationOptions::new(),
        )
        .unwrap();
    callbacks
        .advance_segment_to(&mut execution, segment, 1.0)
        .unwrap();
    assert_eq!(*times.borrow(), [0.5, 1.0]);
    assert_eq!(execution.frame().objects[0].transform.translation.x, 1.0);
    assert_eq!(execution.frame().objects[1].appearance, 1.0);
    scene
        .live(&mut execution)
        .complete_segment(segment)
        .unwrap();
}

#[test]
fn a_target_with_completed_updater_history_can_fade_out_without_restarting_it() {
    let mut scene = Scene::new();
    let target = scene.square(1.0).unwrap();
    scene.add(&target).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = install_motion(&mut scene, &target, &times);
    let mut execution = scene.execution_session().unwrap();
    let wait = scene.live(&mut execution).wait_segment(1.0).unwrap();
    callbacks
        .advance_segment_to(&mut execution, wait, 1.0)
        .unwrap();
    scene.live(&mut execution).complete_segment(wait).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_updater(target.node_id(), MOVE, 1.0);
    scene.live(&mut execution).apply(tx).unwrap();
    let calls = times.borrow().clone();
    let frozen = scene
        .live(&mut execution)
        .effective(&target)
        .unwrap()
        .transform;
    let segment = scene
        .live(&mut execution)
        .declare_and_activate_composition(
            &Lifecycle::FadeOut.request(&target),
            AnimationOptions::new(),
        )
        .unwrap();
    for time in [1.5, 2.0] {
        callbacks
            .advance_segment_to(&mut execution, segment, time)
            .unwrap();
        assert_eq!(*times.borrow(), calls);
        assert_eq!(execution.frame().objects[0].transform, frozen);
    }
    scene
        .live(&mut execution)
        .complete_segment(segment)
        .unwrap();
    assert!(!scene.live(&mut execution).contains(&target).unwrap());
    assert_eq!(*times.borrow(), calls);
}
