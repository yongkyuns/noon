use super::*;
use crate::{LiveProgramStatus, RustHostCallbackTable};
use noon_core::{
    HostCallbackId, NativeInputModifiers, NativePointerId, NativePointerInput,
    NativePointerInputKind, NativePointerPosition, SemanticMutationTransaction,
};
use std::cell::RefCell;

const CALLBACK: HostCallbackId = HostCallbackId::new(701);
struct Script {
    target: Mobject,
    calls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
    drag: bool,
}
impl Drop for Script {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
impl LiveContinuation for Script {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.calls.get();
        self.calls.set(step + 1);
        match step {
            0 => Ok(ContinuationStep::Await(live.wait_segment(1.0)?)),
            1 => {
                if self.drag {
                    live.set_translation_drag_targets([&self.target])?;
                } else {
                    let mut transaction = SemanticMutationTransaction::new();
                    transaction.add_updater(self.target.node_id(), CALLBACK, 1.0, None);
                    live.apply(transaction)?;
                }
                Ok(ContinuationStep::Await(live.wait_segment(1.0)?))
            }
            2 => Ok(ContinuationStep::Finished),
            _ => panic!("original continuation executed twice"),
        }
    }
}
fn fixture(
    drag: bool,
    initial_callback: bool,
) -> (PreparedSource<Script>, Rc<Cell<usize>>, Rc<Cell<usize>>) {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.5).unwrap();
    target.set_fill(0.1, 0.4, 1.0, 1.0).unwrap();
    target.disable_stroke().unwrap();
    scene.add(&target).unwrap();
    if initial_callback {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(target.node_id(), CALLBACK, 0.0, None);
        transaction
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
    }
    let calls = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let source = scene
        .into_prepared_source(
            Script {
                target,
                calls: calls.clone(),
                drops: drops.clone(),
                drag,
            },
            ReplayLimits::default(),
        )
        .unwrap();
    (source, calls, drops)
}

#[test]
fn late_callback_handoff_preserves_original_source_and_observed_phase_order() {
    for requested in [&[2.0][..], &[1.25, 1.5, 2.0][..]] {
        let (mut source, calls, drops) = fixture(false, false);
        let identity = source
            .program
            .as_ref()
            .unwrap()
            .session()
            .runtime_identity();
        assert!(source.take_execution_program().is_none());
        source.prepare_next().unwrap();
        assert!(
            source.take_execution_program().is_none(),
            "pending reader cannot be lost"
        );
        let mut reader = source.take_reader().unwrap();
        reader.seek(0.5).unwrap();
        let baseline = reader.frame().clone();
        let required =
            PreparedSourceStatus::ExecutionRequired(SourceExecutionRequirement::HostCallbacks);
        assert_eq!(source.prepare_next().unwrap(), required);
        assert_eq!(calls.get(), 2);
        assert_eq!(drops.get(), 0);
        for _ in 0..2 {
            assert_eq!(source.prepare_until(100.0).unwrap(), Some(required));
            assert_eq!(calls.get(), 2);
        }
        let before = source.program.as_ref().unwrap().session().frame().clone();
        let publication = source
            .program
            .as_ref()
            .unwrap()
            .session()
            .publication_context();
        let mut program = source.take_execution_program().unwrap();
        assert_eq!(program.session().runtime_identity(), identity);
        assert_eq!(program.session().frame(), &before);
        assert_eq!(program.session().publication_context(), publication);
        assert!(!program.session().replay_scope_active());
        assert_eq!(source.status(), PreparedSourceStatus::Transferred);
        assert!(source.take_execution_program().is_none());
        assert_eq!(
            source.prepare_next().unwrap(),
            PreparedSourceStatus::Transferred
        );
        source.cancel();
        drop(source);
        assert_eq!(
            drops.get(),
            0,
            "original continuation remains owned by the program"
        );

        let phases = Rc::new(RefCell::new(Vec::new()));
        let observed = phases.clone();
        let mut callbacks = RustHostCallbackTable::new();
        callbacks
            .insert(CALLBACK, move |context| {
                observed
                    .borrow_mut()
                    .push((context.time(), context.delta_time()));
                let mut transform = context.target_state().transform;
                transform.translation.x += 1.0; // Deliberately not an endpoint function.
                context.set_target_transform(transform)
            })
            .unwrap();
        for &time in requested {
            program.drive_to(&mut callbacks, time).unwrap();
        }
        let expected = if requested.len() == 1 {
            vec![(1.0, 0.0), (2.0, 1.0)]
        } else {
            vec![(1.0, 0.0), (1.25, 0.25), (1.5, 0.25), (2.0, 0.5)]
        };
        assert_eq!(*phases.borrow(), expected);
        assert_eq!(
            program.session().frame().objects[0].transform.translation.x,
            expected.len() as f32
        );
        assert_eq!(calls.get(), 2);
        assert!(matches!(
            program.status(),
            LiveProgramStatus::PublicationPending(_)
        ));
        assert!(
            program.resume().is_err(),
            "handoff must not fabricate publication admission"
        );
        assert_eq!(calls.get(), 2);
        let admitted = program.take_renderer_publication().context();
        program.admit_publication(admitted).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(calls.get(), 3);
        for _ in 0..2 {
            reader.seek(0.5).unwrap();
            assert_eq!(reader.frame(), &baseline);
        }
        assert_eq!(
            *phases.borrow(),
            expected,
            "old reader never invokes callbacks"
        );
        assert_eq!(reader.frontier(), 1.0);
        assert_eq!(reader.end_time(), None);
        drop(program);
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn late_causal_handoff_keeps_the_canonical_scene_for_real_drag_input() {
    let (mut source, calls, drops) = fixture(true, false);
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::ExecutionRequired(SourceExecutionRequirement::NativeInput)
    );
    let mut program = source.take_execution_program().unwrap();
    drop(source);
    assert_eq!(calls.get(), 2);
    assert_eq!(program.session().frame().time, 1.0);
    program
        .configure_native_pointer_input(
            NativePointerId {
                source: 9,
                pointer: 1,
            },
            1,
        )
        .unwrap();
    let position =
        |x| NativePointerPosition::new(Vec2::new(x, 0.0), Vec2::new(x * 20.0, 100.0)).unwrap();
    for (sequence, kind) in [
        (
            1,
            NativePointerInputKind::Press {
                position: position(0.0),
                button: 0,
            },
        ),
        (2, NativePointerInputKind::Move(position(1.5))),
        (
            3,
            NativePointerInputKind::Release {
                position: position(1.5),
                button: 0,
            },
        ),
    ] {
        let token = program.native_pointer_input_token().unwrap();
        let input = NativePointerInput::new(
            sequence,
            token.pointer(),
            token.context(),
            NativeInputModifiers::default(),
            kind,
        );
        program.submit_native_pointer_input(&token, input).unwrap();
    }
    assert_eq!(
        program.session().frame().objects[0].transform.translation.x,
        1.5
    );
    program
        .drive_to(&mut RustHostCallbackTable::new(), 2.0)
        .unwrap();
    if matches!(program.status(), LiveProgramStatus::PublicationPending(_)) {
        let publication = program.take_renderer_publication().context();
        program.admit_publication(publication).unwrap();
    }
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    reader.seek(0.5).unwrap();
    assert_eq!(
        reader.frame(),
        &baseline,
        "causal head edits cannot rewrite promised history"
    );
    assert_eq!(calls.get(), 3);
    drop(program);
    assert_eq!(drops.get(), 1);
}

#[test]
fn initial_callback_handoff_does_not_start_or_reconstruct_the_continuation() {
    let (mut source, calls, drops) = fixture(false, true);
    let required =
        PreparedSourceStatus::ExecutionRequired(SourceExecutionRequirement::HostCallbacks);
    assert_eq!(source.status(), required);
    assert_eq!(source.prepare_next().unwrap(), required);
    assert_eq!(calls.get(), 0);
    assert!(source.take_reader().is_none());
    let mut program = source.take_execution_program().unwrap();
    drop(source);
    assert_eq!(calls.get(), 0);
    assert_eq!(drops.get(), 0);
    assert!(!program.session().replay_scope_active());
    program.resume().unwrap();
    assert_eq!(calls.get(), 1);
    let invocations = Rc::new(Cell::new(0));
    let counter = invocations.clone();
    let mut callbacks = RustHostCallbackTable::new();
    callbacks
        .insert(CALLBACK, move |context| {
            counter.set(counter.get() + 1);
            let mut transform = context.target_state().transform;
            transform.translation.x += 1.0;
            context.set_target_transform(transform)
        })
        .unwrap();
    program.drive_to(&mut callbacks, 0.25).unwrap();
    assert_eq!(invocations.get(), 2); // Activation at zero, then the requested time.
    assert_eq!(
        program.session().frame().objects[0].transform.translation.x,
        2.0
    );
    assert_eq!(calls.get(), 1);
    drop(program);
    assert_eq!(drops.get(), 1);
}
