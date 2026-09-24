//! Acceptance tests for lifecycle animations beside an unrelated host updater.
//! These exercise the shared execution path used by Python, without a browser.

use std::{cell::RefCell, rc::Rc};

use noon::integration::HostCallbackId;
use noon::{
    AnimationCompositionRequest as Request, AnimationOptions, FadeEndpoint, Mobject,
    RateFunction, RustHostCallbackTable, Scene, SemanticAnimationCompositionKind as Kind,
    SemanticFadeDirection,
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

fn qualify_unrelated_callback(lifecycle: Lifecycle) {
    // Both a leaf request and a nested request must preserve the same semantics.
    for nested in [false, true] {
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
        let segment = scene
            .live(&mut execution)
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap_or_else(|error| {
                panic!("{lifecycle:?}, nested={nested}: unrelated updater blocked admission: {error}")
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
