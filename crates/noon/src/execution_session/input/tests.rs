use crate::integration::{
    NativeInputModifiers, NativePointerCancellation, NativePointerId, NativePointerInput,
    NativePointerInputKind, NativePointerInputToken, NativePointerPosition,
};
use noon_core::{
    NativeEventOccurrence, NativeEventSource, NativeInputValue, NativeStateSource, ReactiveValue,
    SemanticMutationTransaction, SemanticNativeInputSource, SemanticNodeCreation, SemanticNodeId,
    SemanticObjectProperty, SemanticObjectState, SemanticSignalExpr, SemanticSignalValue,
    SemanticStore, SemanticVec3, StoredGeometry, Vec2,
};

use super::{ExecutionSession, ExecutionSessionInputError, NativePointerInputPublication};

const POINTER: NativePointerId = NativePointerId { source: 11, pointer: 3 };
const VIEW: u64 = 7;

struct Fixture {
    store: SemanticStore,
    root: SemanticNodeId,
    unrelated: SemanticNodeId,
    position: SemanticNodeId,
    primary: SemanticNodeId,
    extra: SemanticNodeId,
    down: SemanticNodeId,
    up: SemanticNodeId,
}

fn native_signal(
    store: &mut SemanticStore,
    root: SemanticNodeId,
    source: SemanticNativeInputSource,
    initial: SemanticSignalValue,
) -> SemanticNodeId {
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(
        SemanticNodeCreation::native_input_signal(initial, source).unwrap(),
    );
    transaction.scope_signal(root, pending);
    transaction.apply(store).unwrap().resolve(pending).unwrap()
}

impl Fixture {
    fn new() -> Self {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let target = store.insert_semantic_object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 0.5 },
        ));
        let unrelated = store.insert_semantic_object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 0.5 },
        ));
        for object in [target, unrelated] {
            store.add_semantic_family_member(root, object).unwrap();
        }
        let position = native_signal(
            &mut store, root,
            SemanticNativeInputSource::State(NativeStateSource::PointerPosition),
            SemanticSignalValue::Vec3(SemanticVec3::ZERO),
        );
        let primary = native_signal(
            &mut store, root,
            SemanticNativeInputSource::State(NativeStateSource::PointerButton { button: 0 }),
            SemanticSignalValue::Bool(false),
        );
        let extra = native_signal(
            &mut store, root,
            SemanticNativeInputSource::State(NativeStateSource::PointerButton { button: 255 }),
            SemanticSignalValue::Bool(false),
        );
        let down = native_signal(
            &mut store, root,
            SemanticNativeInputSource::Event(NativeEventSource::PointerDown { button: 0 }),
            SemanticSignalValue::Scalar(0.0),
        );
        let up = native_signal(
            &mut store, root,
            SemanticNativeInputSource::Event(NativeEventSource::PointerUp { button: 0 }),
            SemanticSignalValue::Scalar(0.0),
        );
        store.bind_semantic_signal(position, target, SemanticObjectProperty::Translation).unwrap();
        store.bind_semantic_signal(down, target, SemanticObjectProperty::RotationZ).unwrap();
        Self { store, root, unrelated, position, primary, extra, down, up }
    }

    fn session(&self) -> ExecutionSession {
        let mut session = ExecutionSession::from_semantic_root(&self.store, self.root).unwrap();
        session.configure_native_pointer_input(POINTER, VIEW).unwrap();
        session.take_frame_changes();
        session
    }
}

fn position(x: f32, y: f32) -> NativePointerPosition {
    NativePointerPosition::new(Vec2::new(x, y), Vec2::new(x + 100.0, 100.0 - y)).unwrap()
}

fn record(token: &NativePointerInputToken, sequence: u64, kind: NativePointerInputKind) -> NativePointerInput {
    NativePointerInput::new(sequence, token.pointer(), token.context(), NativeInputModifiers {
        shift: true, ..NativeInputModifiers::default()
    }, kind)
}

fn submit(session: &mut ExecutionSession, sequence: u64, kind: NativePointerInputKind) -> NativePointerInputPublication {
    let token = session.native_pointer_input_token().unwrap();
    session.submit_native_pointer_input(&token, record(&token, sequence, kind)).unwrap()
}

fn press(x: f32, y: f32, button: u8) -> NativePointerInputKind {
    NativePointerInputKind::Press { position: position(x, y), button }
}

#[test]
fn press_publishes_state_and_event_once_at_paused_time_with_local_dirtiness() {
    let fixture = Fixture::new();
    let revision = fixture.store.scene_revision();
    let mut session = fixture.session();
    assert!(session.wake_state().is_quiescent());
    let before = session.publication_context();
    let receipt = submit(&mut session, 1, press(-2.0, 3.0, 0));
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(session.effective_signal_value(fixture.position), Some(&ReactiveValue::Vec2(Vec2::new(-2.0, 3.0))));
    assert_eq!(session.effective_signal_value(fixture.primary), Some(&ReactiveValue::Bool(true)));
    assert_eq!(session.effective_signal_value(fixture.down), Some(&ReactiveValue::Scalar(1.0)));
    assert_eq!(session.frame().objects[0].transform.translation, Vec2::new(-2.0, 3.0));
    assert_eq!(session.frame().objects[0].transform.rotation, 1.0);
    assert_eq!(receipt.previous_publication(), before);
    assert_eq!(receipt.publication().frame_epoch(), before.frame_epoch().checked_next().unwrap());
    assert_eq!(receipt.publication().scene_revision(), before.scene_revision());
    assert_eq!(receipt.publication().execution_revision(), before.execution_revision());
    assert_eq!(fixture.store.scene_revision(), revision);
    assert!(session.wake_state().frame_pending());
    assert_eq!(session.take_frame_changes().object_indices(), &[0]);
    assert!(session.wake_state().is_quiescent());
}

#[test]
fn receipt_keeps_press_at_a_after_motion_to_b_and_release() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let pressed = submit(&mut session, 10, press(1.0, 2.0, 0));
    let moved = submit(&mut session, 11, NativePointerInputKind::Move(position(8.0, 9.0)));
    let released = submit(&mut session, 12, NativePointerInputKind::Release { position: position(8.0, 9.0), button: 0 });
    assert_eq!(pressed.input().position(), Some(position(1.0, 2.0)));
    assert_eq!(moved.input().position(), Some(position(8.0, 9.0)));
    assert_eq!(pressed.input().sequence(), 10);
    assert_eq!(released.input().sequence(), 12);
    assert_eq!(pressed.input().pointer(), POINTER);
    assert!(pressed.input().modifiers().shift);
    assert_eq!(session.effective_signal_value(fixture.primary), Some(&ReactiveValue::Bool(false)));
    assert_eq!(session.effective_signal_value(fixture.down), Some(&ReactiveValue::Scalar(1.0)));
    assert_eq!(session.effective_signal_value(fixture.up), Some(&ReactiveValue::Scalar(1.0)));
}

#[test]
fn repeated_identical_occurrences_are_not_coalesced() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    for sequence in [1, 2] {
        submit(&mut session, sequence, press(0.0, 0.0, 0));
    }
    assert_eq!(session.effective_signal_value(fixture.down), Some(&ReactiveValue::Scalar(2.0)));
    for sequence in [3, 4] {
        submit(&mut session, sequence, NativePointerInputKind::Release { position: position(0.0, 0.0), button: 0 });
    }
    assert_eq!(session.effective_signal_value(fixture.up), Some(&ReactiveValue::Scalar(2.0)));
    let token = session.native_pointer_input_token().unwrap();
    let before = session.frame().clone();
    assert_eq!(session.submit_native_pointer_input(&token, record(&token, 4, press(0.0, 0.0, 0))), Err(ExecutionSessionInputError::NativeEventOutOfOrder { previous: 4, next: 4 }));
    assert_eq!(session.frame(), &before);
}

#[test]
fn unbound_pointer_records_and_other_native_events_share_one_sequence() {
    let store = SemanticStore::new();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    assert_eq!(session.native_pointer_input_token(), Err(ExecutionSessionInputError::PointerNotConfigured));
    session.configure_native_pointer_input(POINTER, VIEW).unwrap();
    session.take_frame_changes();
    session.emit_native_event(NativeEventOccurrence::new(10, NativeEventSource::Wheel)).unwrap();
    let token = session.native_pointer_input_token().unwrap();
    let before = session.publication_context();
    assert_eq!(session.submit_native_pointer_input(&token, record(&token, 10, NativePointerInputKind::Move(position(5.0, 6.0)))), Err(ExecutionSessionInputError::NativeEventOutOfOrder { previous: 10, next: 10 }));
    submit(&mut session, 11, NativePointerInputKind::Move(position(5.0, 6.0)));
    assert_eq!(session.emit_native_event(NativeEventOccurrence::new(11, NativeEventSource::Wheel)), Err(ExecutionSessionInputError::NativeEventOutOfOrder { previous: 11, next: 11 }));
    assert_eq!(session.publication_context(), before);
    assert!(session.wake_state().is_quiescent());
}

#[test]
fn every_cancellation_reason_clears_buttons_without_release_or_fake_position() {
    for reason in [NativePointerCancellation::Cancelled, NativePointerCancellation::CaptureLost, NativePointerCancellation::CaptureFailed, NativePointerCancellation::FocusLost] {
        let fixture = Fixture::new();
        let mut session = fixture.session();
        submit(&mut session, 1, press(2.0, 3.0, 0));
        submit(&mut session, 2, press(4.0, 5.0, 255));
        let token = session.native_pointer_input_token().unwrap();
        session.advance_to(4.0).unwrap();
        let before = session.publication_context();
        let input = record(&token, 3, NativePointerInputKind::Cancel(reason));
        let receipt = session.submit_native_pointer_input(&token, input).unwrap();
        assert_eq!(receipt.input(), input);
        assert_eq!(receipt.input().position(), None);
        assert_eq!(receipt.previous_publication(), before);
        assert_eq!(session.frame().time, 4.0);
        for signal in [fixture.primary, fixture.extra] {
            assert_eq!(session.effective_signal_value(signal), Some(&ReactiveValue::Bool(false)));
        }
        assert_eq!(session.effective_signal_value(fixture.position), Some(&ReactiveValue::Vec2(Vec2::new(4.0, 5.0))));
        assert_eq!(session.effective_signal_value(fixture.down), Some(&ReactiveValue::Scalar(1.0)));
        assert_eq!(session.effective_signal_value(fixture.up), Some(&ReactiveValue::Scalar(0.0)));
    }
}

#[test]
fn configuring_resets_preexisting_buttons_and_rebinding_rejects_old_tokens() {
    let fixture = Fixture::new();
    let mut session = ExecutionSession::from_semantic_root(&fixture.store, fixture.root).unwrap();
    session.set_native_state_input(NativeStateSource::PointerButton { button: 255 }, NativeInputValue::Bool(true)).unwrap();
    let first = session.configure_native_pointer_input(POINTER, VIEW).unwrap();
    assert_eq!(session.effective_signal_value(fixture.extra), Some(&ReactiveValue::Bool(false)));
    submit(&mut session, 1, press(0.0, 0.0, 0));
    let second = session.configure_native_pointer_input(POINTER, VIEW).unwrap();
    assert_ne!(first, second);
    assert_eq!(session.effective_signal_value(fixture.primary), Some(&ReactiveValue::Bool(false)));
    submit(&mut session, 2, press(0.0, 0.0, 0));
    assert_eq!(session.submit_native_pointer_input(&first, record(&first, 3, NativePointerInputKind::Cancel(NativePointerCancellation::FocusLost))), Err(ExecutionSessionInputError::StalePointerBinding));
    assert_eq!(session.effective_signal_value(fixture.primary), Some(&ReactiveValue::Bool(true)));
    let replacement = NativePointerId { source: POINTER.source + 1, pointer: POINTER.pointer };
    session.configure_native_pointer_input(replacement, VIEW + 1).unwrap();
    let token = session.native_pointer_input_token().unwrap();
    let wrong = NativePointerInput::new(3, POINTER, token.context(), NativeInputModifiers::default(), press(1.0, 1.0, 0));
    assert_eq!(session.submit_native_pointer_input(&token, wrong), Err(ExecutionSessionInputError::WrongPointer { expected: replacement, actual: POINTER }));
    assert_eq!(session.last_native_event_sequence, Some(2));
}

#[test]
fn cloned_and_replaced_runtimes_reject_foreign_tokens_even_when_revisions_match() {
    let fixture = Fixture::new();
    let mut original = fixture.session();
    submit(&mut original, 1, press(1.0, 1.0, 0));
    let token = original.native_pointer_input_token().unwrap();
    let mut cloned = original.clone();
    assert_eq!(cloned.publication_context(), original.publication_context());
    let cancel = record(&token, 2, NativePointerInputKind::Cancel(NativePointerCancellation::FocusLost));
    assert_eq!(cloned.submit_native_pointer_input(&token, cancel), Err(ExecutionSessionInputError::ForeignPointerRuntime));
    submit(&mut cloned, 2, NativePointerInputKind::Cancel(NativePointerCancellation::FocusLost));
    assert_eq!(cloned.effective_signal_value(fixture.primary), Some(&ReactiveValue::Bool(false)));
    assert_eq!(original.effective_signal_value(fixture.primary), Some(&ReactiveValue::Bool(true)));
    let mut replacement = fixture.session();
    assert_eq!(replacement.submit_native_pointer_input(&token, cancel), Err(ExecutionSessionInputError::ForeignPointerRuntime));
}

#[test]
fn stale_publication_and_mismatched_view_do_not_acknowledge_input() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let old = session.native_pointer_input_token().unwrap();
    session.advance_to(1.0).unwrap();
    let before = session.publication_context();
    assert_eq!(session.submit_native_pointer_input(&old, record(&old, 7, press(1.0, 2.0, 0))), Err(ExecutionSessionInputError::StalePointerPublication { expected: before, actual: old.context().publication }));
    let token = session.native_pointer_input_token().unwrap();
    let mut context = token.context();
    context.view_revision += 1;
    let wrong = NativePointerInput::new(7, POINTER, context, NativeInputModifiers::default(), press(1.0, 2.0, 0));
    assert_eq!(session.submit_native_pointer_input(&token, wrong), Err(ExecutionSessionInputError::PointerContextMismatch));
    assert_eq!(session.last_native_event_sequence, None);
    assert_eq!(session.effective_signal_value(fixture.primary), Some(&ReactiveValue::Bool(false)));
    assert_eq!(session.publication_context(), before);
    submit(&mut session, 7, press(1.0, 2.0, 0));
}

#[test]
fn required_callback_stall_rejects_bursts_and_reconfiguration_without_buffering() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let token = session.native_pointer_input_token().unwrap();
    let overlay = session.begin_required_callback_phase(0.0, [fixture.unrelated]).unwrap();
    let before = session.frame().clone();
    for sequence in 0..512 {
        assert_eq!(session.submit_native_pointer_input(&token, record(&token, sequence, press(1.0, 2.0, 0))), Err(ExecutionSessionInputError::RequiredCallbackPending));
    }
    assert_eq!(session.configure_native_pointer_input(POINTER, VIEW + 1), Err(ExecutionSessionInputError::RequiredCallbackPending));
    assert_eq!(session.native_pointer_input_token().unwrap(), token);
    assert_eq!(session.last_native_event_sequence, None);
    assert_eq!(session.frame(), &before);
    session.commit_required_callback_phase(overlay.finish()).unwrap();
    submit(&mut session, 0, press(1.0, 2.0, 0));
    assert_eq!(session.effective_signal_value(fixture.down), Some(&ReactiveValue::Scalar(1.0)));
}

#[test]
fn unbound_input_cannot_bypass_a_required_callback_barrier() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 }));
    store.attach_to_scene(object).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let token = session.configure_native_pointer_input(POINTER, VIEW).unwrap();
    let overlay = session.begin_required_callback_phase(0.0, [object]).unwrap();
    let before = session.publication_context();
    assert_eq!(session.submit_native_pointer_input(&token, record(&token, 1, press(2.0, 3.0, 0))), Err(ExecutionSessionInputError::RequiredCallbackPending));
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.last_native_event_sequence, None);
    session.commit_required_callback_phase(overlay.finish()).unwrap();
}

#[test]
fn evaluation_failure_rolls_back_position_buttons_counter_and_sequence() {
    let mut fixture = Fixture::new();
    let gain = fixture.store.insert_semantic_input_signal(1.0e20_f64).unwrap();
    let scaled = fixture.store.insert_semantic_derived_signal(SemanticSignalExpr::Mul(
        Box::new(SemanticSignalExpr::signal(fixture.down)),
        Box::new(SemanticSignalExpr::signal(gain)),
    )).unwrap();
    let squared = fixture.store.insert_semantic_derived_signal(SemanticSignalExpr::Mul(
        Box::new(SemanticSignalExpr::signal(scaled)),
        Box::new(SemanticSignalExpr::signal(scaled)),
    )).unwrap();
    fixture.store.bind_semantic_signal(squared, fixture.unrelated, SemanticObjectProperty::RotationZ).unwrap();
    let mut session = fixture.session();
    let token = session.native_pointer_input_token().unwrap();
    let before = session.frame().clone();
    let publication = session.publication_context();
    assert!(matches!(session.submit_native_pointer_input(&token, record(&token, 5, press(3.0, 4.0, 0))), Err(ExecutionSessionInputError::Evaluation(_))));
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame(), &before);
    assert_eq!(session.last_native_event_sequence, None);
    assert_eq!(session.effective_signal_value(fixture.position), Some(&ReactiveValue::Vec2(Vec2::ZERO)));
    assert_eq!(session.effective_signal_value(fixture.primary), Some(&ReactiveValue::Bool(false)));
    assert_eq!(session.effective_signal_value(fixture.down), Some(&ReactiveValue::Scalar(0.0)));
    assert!(session.take_frame_changes().is_empty());
    session.set_reactive_input(gain, 1.0_f32).unwrap();
    submit(&mut session, 5, press(3.0, 4.0, 0));
    assert_eq!(session.effective_signal_value(fixture.down), Some(&ReactiveValue::Scalar(1.0)));
}

#[test]
fn contextual_mode_rejects_the_uncontextualized_pointer_lane() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let before = session.frame().clone();
    for (source, value) in [
        (NativeStateSource::PointerPosition, NativeInputValue::Vec2(Vec2::new(2.0, 3.0))),
        (NativeStateSource::PointerButton { button: 0 }, NativeInputValue::Bool(true)),
    ] {
        assert_eq!(session.set_native_state_input(source, value), Err(ExecutionSessionInputError::ContextualPointerRequired));
    }
    for source in [NativeEventSource::PointerDown { button: 0 }, NativeEventSource::PointerUp { button: 0 }] {
        assert_eq!(session.emit_native_event(NativeEventOccurrence::new(1, source)), Err(ExecutionSessionInputError::ContextualPointerRequired));
    }
    assert_eq!(session.frame(), &before);
    assert_eq!(session.last_native_event_sequence, None);
    session.emit_native_event(NativeEventOccurrence::new(1, NativeEventSource::Wheel)).unwrap();
    session.set_native_state_input(NativeStateSource::ViewportSize, NativeInputValue::Vec2(Vec2::new(640.0, 480.0))).unwrap();
    submit(&mut session, 2, press(1.0, 2.0, 0));
}

#[test]
fn binding_and_ingress_sequence_exhaustion_do_not_wrap_or_clear_pressed_state() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    session.pointer_input.next_generation = Some(u64::MAX);
    session.configure_native_pointer_input(POINTER, VIEW).unwrap();
    submit(&mut session, u64::MAX, press(1.0, 1.0, 0));
    let token = session.native_pointer_input_token().unwrap();
    assert_eq!(session.configure_native_pointer_input(POINTER, VIEW + 1), Err(ExecutionSessionInputError::PointerBindingSequenceExhausted));
    assert_eq!(session.native_pointer_input_token().unwrap(), token);
    assert_eq!(session.effective_signal_value(fixture.primary), Some(&ReactiveValue::Bool(true)));
    assert_eq!(session.submit_native_pointer_input(&token, record(&token, 0, press(2.0, 2.0, 0))), Err(ExecutionSessionInputError::NativeEventOutOfOrder { previous: u64::MAX, next: 0 }));
}

#[test]
fn motion_out_and_back_remains_two_ordered_deliveries() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    submit(&mut session, 0, press(0.0, 0.0, 0));
    let away = submit(&mut session, 1, NativePointerInputKind::Move(position(50.0, 0.0)));
    let back = submit(&mut session, 2, NativePointerInputKind::Move(position(0.0, 0.0)));
    assert_eq!(away.input().position(), Some(position(50.0, 0.0)));
    assert_eq!(back.input().position(), Some(position(0.0, 0.0)));
    assert_ne!(away.publication(), back.publication());
    assert_eq!(session.effective_signal_value(fixture.down), Some(&ReactiveValue::Scalar(1.0)));
}
