use super::*;
use crate::{ContinuationStep, LiveSession, LiveSessionError, Mobject, Scene};
use noon_core::{HostCallbackId, SemanticMutationTransaction};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Pure {
    target: Mobject,
    calls: Rc<Cell<usize>>,
    fail: bool,
}
impl LiveContinuation for Pure {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.calls.get();
        self.calls.set(step + 1);
        Ok(match step {
            0 => ContinuationStep::Await(live.wait_segment(0.0)?),
            1 => ContinuationStep::Await(live.wait_segment(1.0)?),
            2 => {
                if self.fail {
                    return live.wait_segment(-1.0).map(ContinuationStep::Await);
                }
                live.set_fill(&self.target, 1.0, 0.0, 0.0, 1.0)?;
                ContinuationStep::Await(live.wait_segment(1.0)?)
            }
            3 => ContinuationStep::Finished,
            _ => panic!("source was restarted"),
        })
    }
}
fn pure(fail: bool) -> (PlaybackProgram<Pure>, Rc<Cell<usize>>) {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.5).unwrap();
    target.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    scene.add(&target).unwrap();
    let calls = Rc::new(Cell::new(0));
    (
        scene
            .into_live_program(Pure {
                target,
                calls: calls.clone(),
                fail,
            })
            .unwrap()
            .into_playback()
            .unwrap(),
        calls,
    )
}
#[test]
fn loading_yields_adopts_without_advancing_and_closes_the_exact_endpoint() {
    let (mut player, calls) = pure(false);
    let mut callbacks = RustHostCallbackTable::new();
    assert!(!player.presentation_ready());
    assert_eq!(calls.get(), 0);
    player.prepare_ready(&mut callbacks).unwrap();
    assert!(!player.presentation_ready());
    assert_eq!(calls.get(), 1, "zero wait yields once");
    player.prepare_ready(&mut callbacks).unwrap();
    assert!(player.presentation_ready());
    assert_eq!(calls.get(), 2);
    let identity = player.session().unwrap().runtime_identity();
    for _ in 0..3 {
        assert!(!player.prepare_ready(&mut callbacks).unwrap());
    }
    assert_eq!(calls.get(), 2, "satisfied lookahead invokes no source");
    player.advance_to(&mut callbacks, 0.25).unwrap();
    let baseline = player.session().unwrap().frame().clone();
    let publication = player.session().unwrap().publication_context();
    player.prepare_ready(&mut callbacks).unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(player.session().unwrap().frame(), &baseline);
    assert_eq!(player.session().unwrap().publication_context(), publication);
    assert_eq!(player.session().unwrap().runtime_identity(), identity);
    player.advance_to(&mut callbacks, 2.0).unwrap();
    assert!(
        player.buffering(),
        "an open endpoint is not a readable sample"
    );
    assert_eq!(player.session().unwrap().frame(), &baseline);
    player.prepare_ready(&mut callbacks).unwrap();
    assert!(!player.buffering());
    assert!(player.take_clock_reanchor());
    assert!(!player.take_clock_reanchor());
    assert_eq!(player.end_time(), Some(2.0));
    assert_eq!(player.session().unwrap().frame(), &baseline);
    player.advance_to(&mut callbacks, 2.0).unwrap();
    assert!(player.finished());
    assert!(player.can_seek_to(0.0));
    assert!(!player.prepare_ready(&mut callbacks).unwrap());
    player.seek(0.25).unwrap();
    assert_eq!(player.session().unwrap().frame(), &baseline);
    assert_eq!(calls.get(), 4);
}
#[test]
fn failure_preserves_cached_pause_and_seek_without_retrying_source() {
    let (mut player, calls) = pure(true);
    let mut callbacks = RustHostCallbackTable::new();
    player.prepare_ready(&mut callbacks).unwrap();
    player.prepare_ready(&mut callbacks).unwrap();
    player.advance_to(&mut callbacks, 0.25).unwrap();
    let baseline = player.session().unwrap().frame().clone();
    assert!(player.prepare_ready(&mut callbacks).is_err());
    assert_eq!(calls.get(), 3);
    assert_eq!(player.session().unwrap().frame(), &baseline);
    assert!(!player.source_active());
    assert!(!player.prepare_ready(&mut callbacks).unwrap());
    player.seek(0.0).unwrap();
    player.seek(0.25).unwrap();
    assert_eq!(player.session().unwrap().frame(), &baseline);
    assert_eq!(player.end_time(), None);
    assert_eq!(calls.get(), 3);
}

const CALLBACK: HostCallbackId = HostCallbackId::new(718);
struct CallbackSource {
    target: Mobject,
    calls: Rc<Cell<usize>>,
}
impl LiveContinuation for CallbackSource {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.calls.get();
        self.calls.set(step + 1);
        Ok(match step {
            0 => ContinuationStep::Await(live.wait_segment(1.0)?),
            1 => {
                let mut tx = SemanticMutationTransaction::new();
                tx.add_updater(self.target.node_id(), CALLBACK, 1.0, None);
                live.apply(tx)?;
                ContinuationStep::Await(live.wait_segment(1.0)?)
            }
            2 => ContinuationStep::Finished,
            _ => panic!("source was restarted"),
        })
    }
}
#[test]
fn frontier_handoff_keeps_observed_callback_times_inspection_and_foreign_receipt_rejection() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let calls = Rc::new(Cell::new(0));
    let mut player = scene
        .into_live_program(CallbackSource {
            target,
            calls: calls.clone(),
        })
        .unwrap()
        .into_playback()
        .unwrap();
    let phases = Rc::new(RefCell::new(Vec::new()));
    let recorded = phases.clone();
    let mut callbacks = RustHostCallbackTable::new();
    callbacks
        .insert(CALLBACK, move |ctx| {
            recorded.borrow_mut().push((ctx.time(), ctx.delta_time()));
            let mut transform = ctx.target_state().transform;
            transform.translation.x += 1.0;
            ctx.set_target_transform(transform)
        })
        .unwrap();
    player.prepare_ready(&mut callbacks).unwrap();
    player.advance_to(&mut callbacks, 0.5).unwrap();
    player.prepare_ready(&mut callbacks).unwrap();
    let identity = player.session().unwrap().runtime_identity();
    let view = player
        .session()
        .unwrap()
        .inspection_pointer_view(1, Vec2::new(800.0, 400.0))
        .unwrap();
    let frame = player
        .session()
        .unwrap()
        .capture_pointer_frame(view)
        .unwrap();
    assert!(player
        .scroll_inspection_view(&frame, view, Vec2::new(400.0, 200.0), 100.0)
        .unwrap());
    let camera = player.session().unwrap().inspection_camera().unwrap();
    let view = player
        .session()
        .unwrap()
        .inspection_pointer_view(1, Vec2::new(800.0, 400.0))
        .unwrap();
    let frame = player
        .session()
        .unwrap()
        .capture_pointer_frame(view)
        .unwrap();
    for target in [f64::NAN, f64::INFINITY, -1.0, 0.25] {
        assert!(player.advance_to(&mut callbacks, target).is_err());
        assert_eq!(player.session().unwrap().runtime_identity(), identity);
    }
    assert!(phases.borrow().is_empty());
    assert!(!player.prepare_ready(&mut callbacks).unwrap());
    player.advance_to(&mut callbacks, 0.75).unwrap();
    assert_eq!(player.session().unwrap().runtime_identity(), identity);
    player.advance_to(&mut callbacks, 1.25).unwrap();
    assert_ne!(player.session().unwrap().runtime_identity(), identity);
    assert_eq!(
        player.session().unwrap().inspection_camera().unwrap(),
        camera
    );
    assert!(frame
        .validate_presentation(player.session().unwrap(), frame.view())
        .is_err());
    assert_eq!(*phases.borrow(), vec![(1.0, 0.0), (1.25, 0.25)]);
    assert_eq!(calls.get(), 2);
    assert!(
        !player.can_seek_to(0.0),
        "effectful callbacks cannot be replayed"
    );
    player.advance_to(&mut callbacks, 2.0).unwrap();
    let context = player.take_renderer_publication().unwrap().context();
    player.admit_publication(context).unwrap();
    player.advance_to(&mut callbacks, 2.0).unwrap();
    assert!(player.finished());
    assert_eq!(calls.get(), 3);
    assert_eq!(
        *phases.borrow(),
        vec![(1.0, 0.0), (1.25, 0.25), (2.0, 0.75)]
    );
}

#[test]
fn initial_observed_callback_is_not_repeated_when_the_first_request_is_the_same_time() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let calls = Rc::new(Cell::new(0));
    let phases = Rc::new(RefCell::new(Vec::new()));
    let recorded = phases.clone();
    let mut callbacks = RustHostCallbackTable::new();
    callbacks
        .insert(CALLBACK, move |ctx| {
            recorded.borrow_mut().push((ctx.time(), ctx.delta_time()));
            let mut transform = ctx.target_state().transform;
            transform.translation.x += 1.0;
            ctx.set_target_transform(transform)
        })
        .unwrap();
    callbacks
        .add_updater(
            &mut scene.integration_store().borrow_mut(),
            target.node_id(),
            CALLBACK,
            0.0,
            None,
        )
        .unwrap();
    let mut player = scene
        .into_live_program(CallbackSource {
            target,
            calls: calls.clone(),
        })
        .unwrap()
        .into_playback()
        .unwrap();
    player.prepare_ready(&mut callbacks).unwrap();
    player.advance_to(&mut callbacks, 0.0).unwrap();
    player.advance_to(&mut callbacks, 0.25).unwrap();
    assert_eq!(*phases.borrow(), vec![(0.0, 0.0), (0.25, 0.25)]);
    assert_eq!(calls.get(), 1);
}

#[test]
fn attaching_an_already_advanced_program_preserves_its_owner_and_pending_segment() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let calls = Rc::new(Cell::new(0));
    let mut program = scene
        .into_live_program(CallbackSource {
            target,
            calls: calls.clone(),
        })
        .unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    program.resume().unwrap();
    program.drive_to(&mut callbacks, 0.5).unwrap();
    let identity = program.session().runtime_identity();
    let mut player = program.into_playback().unwrap();
    assert!(player.presentation_ready());
    assert_eq!(player.session().unwrap().runtime_identity(), identity);
    assert_eq!(player.session().unwrap().frame().time, 0.5);
    assert!(!player.can_seek_to(0.0));
    assert!(!player.prepare_ready(&mut callbacks).unwrap());
    player.advance_to(&mut callbacks, 0.75).unwrap();
    assert_eq!(player.session().unwrap().frame().time, 0.75);
    assert_eq!(player.session().unwrap().runtime_identity(), identity);
    assert_eq!(calls.get(), 1);
}
