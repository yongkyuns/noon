use noon_core::{
    HostCallbackId, NativeInputModifiers, NativePointerCancellation, NativePointerPosition,
    SemanticMutationTransaction, SemanticNodeId, SemanticObjectProperty, SemanticObjectState,
    SemanticSignalExpr, SemanticStore, SemanticVec3, StoredGeometry, Vec2,
};
use noon_runtime::TimelineWakeState;

use super::*;

const POINTER: NativePointerId = NativePointerId {
    source: 7,
    pointer: 1,
};

struct Fixture {
    store: SemanticStore,
    session: ExecutionSession,
    runtime: RuntimeIdentity,
    position: SemanticNodeId,
    pressed: SemanticNodeId,
    extra_pressed: SemanticNodeId,
    down: SemanticNodeId,
    up: SemanticNodeId,
    moving: SemanticNodeId,
}

impl Fixture {
    fn new(unrelated: usize) -> Self {
        let mut store = SemanticStore::new();
        let position = store
            .insert_semantic_input_signal(SemanticVec3::new(0.0, 0.0, 0.0))
            .unwrap();
        let pressed = store.insert_semantic_input_signal(false).unwrap();
        let extra_pressed = store.insert_semantic_input_signal(false).unwrap();
        let down = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let up = store.insert_semantic_input_signal(0.0_f64).unwrap();
        for (signal, source) in [
            (position, NativeStateSource::PointerPosition),
            (pressed, NativeStateSource::PointerButton { button: 0 }),
            (
                extra_pressed,
                NativeStateSource::PointerButton { button: 255 },
            ),
        ] {
            store
                .bind_semantic_native_state_input(signal, source)
                .unwrap();
        }
        store
            .bind_semantic_native_event_input(down, NativeEventSource::PointerDown { button: 0 })
            .unwrap();
        store
            .bind_semantic_native_event_input(up, NativeEventSource::PointerUp { button: 0 })
            .unwrap();
        let product = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
                Box::new(SemanticSignalExpr::signal(position)),
                Box::new(SemanticSignalExpr::signal(down)),
            ))
            .unwrap();
        let doubled = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
                Box::new(SemanticSignalExpr::signal(product)),
                Box::new(SemanticSignalExpr::scalar(2.0)),
            ))
            .unwrap();
        let moving = circle(&mut store);
        store
            .bind_semantic_signal(doubled, moving, SemanticObjectProperty::Translation)
            .unwrap();
        store
            .bind_semantic_signal(up, moving, SemanticObjectProperty::RotationZ)
            .unwrap();
        for signal in [pressed, extra_pressed] {
            let object = circle(&mut store);
            store
                .bind_semantic_signal(signal, object, SemanticObjectProperty::Presence)
                .unwrap();
        }
        for _ in 0..unrelated {
            circle(&mut store);
        }
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.configure_native_pointer_input(POINTER, 0).unwrap();
        session.take_frame_changes();
        let runtime = session.runtime_identity();
        Self {
            store,
            session,
            runtime,
            position,
            pressed,
            extra_pressed,
            down,
            up,
            moving,
        }
    }

    fn input(&self, sequence: u64, kind: NativePointerInputKind) -> NativePointerInput {
        record(
            sequence,
            POINTER,
            self.session.native_pointer_context().unwrap(),
            kind,
        )
    }

    fn accept(&mut self, sequence: u64, kind: NativePointerInputKind) -> NativePointerInputReceipt {
        let input = self.input(sequence, kind);
        self.session
            .apply_native_pointer_input(self.runtime, input)
            .unwrap()
    }

    fn value(&self, signal: SemanticNodeId) -> ReactiveValue {
        self.session.effective_signal_value(signal).unwrap().clone()
    }
}

fn circle(store: &mut SemanticStore) -> SemanticNodeId {
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    object
}

fn position(x: f32, y: f32) -> NativePointerPosition {
    NativePointerPosition::new(Vec2::new(x, y), Vec2::new(100.0 + x, 200.0 + y)).unwrap()
}

fn press(x: f32, y: f32, button: u8) -> NativePointerInputKind {
    NativePointerInputKind::Press {
        position: position(x, y),
        button,
    }
}

fn record(
    sequence: u64,
    pointer: NativePointerId,
    context: NativePointerContext,
    kind: NativePointerInputKind,
) -> NativePointerInput {
    NativePointerInput::new(
        sequence,
        pointer,
        context,
        NativeInputModifiers {
            shift: true,
            ..Default::default()
        },
        kind,
    )
}

#[test]
fn press_commits_position_button_and_event_in_one_dependency_evaluation() {
    let mut f = Fixture::new(0);
    let before = f.session.publication_context();
    let store_revision = f.store.scene_revision();
    let slot = f.session.execution_object_id(f.moving);
    let receipt = f.accept(0, press(2.0, -1.0, 0));
    assert_eq!(
        f.value(f.position),
        ReactiveValue::Vec2(Vec2::new(2.0, -1.0))
    );
    assert_eq!(f.value(f.pressed), ReactiveValue::Bool(true));
    assert_eq!(f.value(f.down), ReactiveValue::Scalar(1.0));
    assert_eq!(
        f.session.frame().objects[0].transform.translation,
        Vec2::new(4.0, -2.0)
    );
    assert_eq!(f.session.last_reactive_stats().derived_signals_evaluated, 2);
    assert_eq!(
        receipt.publication().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(
        receipt.publication().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(
        receipt.publication().execution_revision(),
        before.execution_revision()
    );
    assert_eq!(receipt.runtime(), f.runtime);
    assert_eq!(f.store.scene_revision(), store_revision);
    assert_eq!(f.session.execution_object_id(f.moving), slot);
    assert_eq!(f.session.frame().time, 0.0);
}

#[test]
fn receipt_preserves_press_location_after_move_and_release() {
    let mut f = Fixture::new(0);
    let press_receipt = f.accept(1, press(2.0, 3.0, 0));
    let move_receipt = f.accept(2, NativePointerInputKind::Move(position(9.0, 8.0)));
    let release_receipt = f.accept(
        3,
        NativePointerInputKind::Release {
            position: position(-4.0, 5.0),
            button: 0,
        },
    );
    assert_eq!(press_receipt.input().position(), Some(position(2.0, 3.0)));
    assert_eq!(move_receipt.input().position(), Some(position(9.0, 8.0)));
    assert_eq!(
        release_receipt.input().position(),
        Some(position(-4.0, 5.0))
    );
    assert!(press_receipt.input().modifiers().shift);
    assert_eq!(
        f.value(f.position),
        ReactiveValue::Vec2(Vec2::new(-4.0, 5.0))
    );
    assert_eq!(f.value(f.pressed), ReactiveValue::Bool(false));
    assert_eq!(f.value(f.down), ReactiveValue::Scalar(1.0));
    assert_eq!(f.value(f.up), ReactiveValue::Scalar(1.0));
}

#[test]
fn identical_presses_remain_distinct_subscribed_occurrences() {
    let mut f = Fixture::new(0);
    for sequence in 0..8 {
        let before = f.session.publication_context();
        let receipt = f.accept(sequence, press(1.0, 0.0, 0));
        assert_eq!(receipt.input().sequence(), sequence);
        assert_eq!(
            f.value(f.down),
            ReactiveValue::Scalar((sequence + 1) as f32)
        );
        assert_eq!(
            receipt.publication().frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
    }
}

#[test]
fn every_cancellation_clears_all_buttons_without_release_or_fake_position() {
    for reason in [
        NativePointerCancellation::Cancelled,
        NativePointerCancellation::CaptureLost,
        NativePointerCancellation::CaptureFailed,
        NativePointerCancellation::FocusLost,
    ] {
        let mut f = Fixture::new(0);
        let _ = f.accept(0, press(1.0, 2.0, 0));
        let _ = f.accept(1, press(3.0, 4.0, 255));
        let before = f.session.publication_context();
        let receipt = f.accept(2, NativePointerInputKind::Cancel(reason));
        assert_eq!(
            receipt.input().kind(),
            NativePointerInputKind::Cancel(reason)
        );
        assert_eq!(receipt.input().position(), None);
        assert_eq!(receipt.input().button_event(), None);
        assert_eq!(
            f.value(f.position),
            ReactiveValue::Vec2(Vec2::new(3.0, 4.0))
        );
        assert_eq!(f.value(f.pressed), ReactiveValue::Bool(false));
        assert_eq!(f.value(f.extra_pressed), ReactiveValue::Bool(false));
        assert_eq!(f.value(f.up), ReactiveValue::Scalar(0.0));
        assert_eq!(
            receipt.publication().frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
        f.session
            .configure_native_pointer_input(POINTER, 1)
            .unwrap();
    }
}

#[test]
fn reactive_failure_rolls_back_the_entire_press_and_allows_same_sequence_retry() {
    let mut f = Fixture::new(0);
    let before = f.session.publication_context();
    let frame = f.session.frame().clone();
    let input = f.input(7, press(3.0e38, 0.0, 0));
    assert!(matches!(
        f.session.apply_native_pointer_input(f.runtime, input),
        Err(ExecutionSessionPointerInputError::Input(
            ExecutionSessionInputError::Evaluation(_)
        ))
    ));
    assert_eq!(f.session.publication_context(), before);
    assert_eq!(f.session.frame(), &frame);
    assert_eq!(
        f.value(f.position),
        ReactiveValue::Vec2(Vec2::new(0.0, 0.0))
    );
    assert_eq!(f.value(f.down), ReactiveValue::Scalar(0.0));
    assert_eq!(f.value(f.pressed), ReactiveValue::Bool(false));
    assert!(f.session.take_frame_changes().is_empty());
    // A failed press did not acquire even an unsubscribed physical button latch.
    f.session
        .configure_native_pointer_input(POINTER, 1)
        .unwrap();
    let receipt = f.accept(7, press(2.0, 0.0, 0));
    assert_eq!(receipt.input().sequence(), 7);
    assert_eq!(f.value(f.down), ReactiveValue::Scalar(1.0));
}

#[test]
fn pointer_and_keyboard_occurrences_share_one_ingress_sequence() {
    let mut f = Fixture::new(0);
    let _ = f.accept(10, NativePointerInputKind::Move(position(1.0, 0.0)));
    let key = NativeEventSource::KeyPress {
        code: "Space".to_owned(),
    };
    assert_eq!(
        f.session
            .emit_native_event(NativeEventOccurrence::new(10, key.clone())),
        Err(ExecutionSessionInputError::NativeEventOutOfOrder {
            previous: 10,
            next: 10
        })
    );
    f.session
        .emit_native_event(NativeEventOccurrence::new(11, key))
        .unwrap();
    let input = f.input(11, press(2.0, 0.0, 0));
    assert_eq!(
        f.session.apply_native_pointer_input(f.runtime, input),
        Err(ExecutionSessionPointerInputError::Input(
            ExecutionSessionInputError::NativeEventOutOfOrder {
                previous: 11,
                next: 11
            }
        ))
    );
    let _ = f.accept(12, press(2.0, 0.0, 0));
}

#[test]
fn cancellation_and_unbound_occurrences_still_consume_full_width_sequences() {
    let store = SemanticStore::new();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.configure_native_pointer_input(POINTER, 0).unwrap();
    session.take_frame_changes();
    let context = session.native_pointer_context().unwrap();
    let runtime = session.runtime_identity();
    for (sequence, kind) in [
        (u64::MAX - 1, press(0.0, 0.0, 128)),
        (
            u64::MAX,
            NativePointerInputKind::Cancel(NativePointerCancellation::Cancelled),
        ),
    ] {
        let receipt = session
            .apply_native_pointer_input(runtime, record(sequence, POINTER, context, kind))
            .unwrap();
        assert_eq!(receipt.input().sequence(), sequence);
        assert_eq!(receipt.publication(), context.publication);
    }
    assert!(matches!(
        session
            .apply_native_pointer_input(runtime, record(0, POINTER, context, press(0.0, 0.0, 0))),
        Err(ExecutionSessionPointerInputError::Input(
            ExecutionSessionInputError::NativeEventOutOfOrder {
                previous: u64::MAX,
                next: 0
            }
        ))
    ));
    assert!(session.take_frame_changes().is_empty());
    session.configure_native_pointer_input(POINTER, 1).unwrap();
}

#[test]
fn different_sources_with_the_same_pointer_number_cannot_drive_one_lane() {
    let mut f = Fixture::new(0);
    let other = NativePointerId {
        source: POINTER.source + 1,
        pointer: POINTER.pointer,
    };
    let before = f.session.publication_context();
    let input = record(
        1,
        other,
        f.session.native_pointer_context().unwrap(),
        press(2.0, 0.0, 0),
    );
    assert_eq!(
        f.session.apply_native_pointer_input(f.runtime, input),
        Err(ExecutionSessionPointerInputError::PointerMismatch {
            expected: POINTER,
            actual: other
        })
    );
    assert_eq!(f.session.publication_context(), before);
    let _ = f.accept(1, press(2.0, 0.0, 0));
}

#[test]
fn clone_and_independent_runtime_reject_the_collectors_old_identity() {
    let f = Fixture::new(0);
    let mut independent = ExecutionSession::from_semantic_store(&f.store).unwrap();
    independent
        .configure_native_pointer_input(POINTER, 0)
        .unwrap();
    for mut session in [f.session.clone(), independent] {
        assert_eq!(
            session.publication_context(),
            f.session.publication_context()
        );
        let input = f.input(0, press(2.0, 0.0, 0));
        assert!(matches!(
            session.apply_native_pointer_input(f.runtime, input),
            Err(ExecutionSessionPointerInputError::ForeignRuntime { .. })
        ));
        let receipt = session
            .apply_native_pointer_input(session.runtime_identity(), input)
            .unwrap();
        assert_eq!(receipt.input(), input);
    }
}

#[test]
fn all_publication_domains_and_the_view_are_validated_before_admission() {
    let mut f = Fixture::new(0);
    let context = f.session.native_pointer_context().unwrap();
    let p = context.publication;
    let wrong = [
        NativePointerContext {
            view_revision: 1,
            ..context
        },
        NativePointerContext {
            publication: PublicationContext::new(
                p.scene_revision().checked_next().unwrap(),
                p.execution_revision(),
                p.frame_epoch(),
            ),
            ..context
        },
        NativePointerContext {
            publication: p.with_execution_revision(p.execution_revision().checked_next().unwrap()),
            ..context
        },
        NativePointerContext {
            publication: p.with_frame_epoch(p.frame_epoch().checked_next().unwrap()),
            ..context
        },
    ];
    for actual in wrong {
        let input = record(0, POINTER, actual, press(1.0, 0.0, 0));
        assert_eq!(
            f.session.apply_native_pointer_input(f.runtime, input),
            Err(ExecutionSessionPointerInputError::StaleContext {
                expected: context,
                actual
            })
        );
        assert_eq!(f.session.publication_context(), p);
        assert!(f.session.take_frame_changes().is_empty());
    }
    let _ = f.accept(0, press(1.0, 0.0, 0));
}

#[test]
fn a_stale_buffered_position_is_not_relabelled_as_the_current_frame() {
    let mut f = Fixture::new(0);
    let old = f.input(2, press(3.0, 0.0, 0));
    let _ = f.accept(1, NativePointerInputKind::Move(position(5.0, 0.0)));
    let before = f.session.publication_context();
    assert!(matches!(
        f.session.apply_native_pointer_input(f.runtime, old),
        Err(ExecutionSessionPointerInputError::StaleContext { .. })
    ));
    assert_eq!(f.session.publication_context(), before);
    assert_eq!(f.value(f.down), ReactiveValue::Scalar(0.0));
    assert_eq!(
        f.value(f.position),
        ReactiveValue::Vec2(Vec2::new(5.0, 0.0))
    );
}

#[test]
fn changing_source_or_view_requires_release_then_a_newer_revision() {
    let mut f = Fixture::new(0);
    let other = NativePointerId {
        pointer: 2,
        ..POINTER
    };
    let _ = f.accept(0, press(1.0, 0.0, 0));
    f.session
        .configure_native_pointer_input(POINTER, 0)
        .unwrap();
    assert_eq!(
        f.session.configure_native_pointer_input(other, 1),
        Err(ExecutionSessionPointerInputError::PointerStillPressed)
    );
    assert_eq!(
        f.session.configure_native_pointer_input(POINTER, 1),
        Err(ExecutionSessionPointerInputError::PointerStillPressed)
    );
    let _ = f.accept(
        1,
        NativePointerInputKind::Cancel(NativePointerCancellation::CaptureLost),
    );
    assert!(matches!(
        f.session.configure_native_pointer_input(other, 0),
        Err(ExecutionSessionPointerInputError::ViewRevisionNotIncreasing { .. })
    ));
    f.session.configure_native_pointer_input(other, 1).unwrap();
    assert_eq!(f.session.native_pointer_context().unwrap().view_revision, 1);
    assert!(matches!(
        f.session.configure_native_pointer_input(POINTER, 0),
        Err(ExecutionSessionPointerInputError::ViewRevisionNotIncreasing { .. })
    ));
}

#[test]
fn an_unsubscribed_held_button_cannot_be_lost_during_rebinding() {
    let mut f = Fixture::new(0);
    let _ = f.accept(0, press(1.0, 0.0, 128));
    assert_eq!(
        f.session.configure_native_pointer_input(POINTER, 1),
        Err(ExecutionSessionPointerInputError::PointerStillPressed)
    );
    let _ = f.accept(
        1,
        NativePointerInputKind::Release {
            position: position(1.0, 0.0),
            button: 128,
        },
    );
    f.session
        .configure_native_pointer_input(POINTER, 1)
        .unwrap();
}

#[test]
fn contextual_mode_rejects_separate_pointer_state_and_button_events() {
    let mut f = Fixture::new(0);
    let before = f.session.publication_context();
    for (source, value) in [
        (
            NativeStateSource::PointerPosition,
            NativeInputValue::Vec2(Vec2::new(1.0, 0.0)),
        ),
        (
            NativeStateSource::PointerButton { button: 0 },
            NativeInputValue::Bool(true),
        ),
    ] {
        assert_eq!(
            f.session.set_native_state_input(source, value),
            Err(ExecutionSessionInputError::ContextualPointerInputRequired)
        );
    }
    for source in [
        NativeEventSource::PointerDown { button: 0 },
        NativeEventSource::PointerUp { button: 0 },
    ] {
        assert_eq!(
            f.session
                .emit_native_event(NativeEventOccurrence::new(0, source)),
            Err(ExecutionSessionInputError::ContextualPointerInputRequired)
        );
    }
    assert_eq!(f.session.publication_context(), before);
    let _ = f.accept(0, press(1.0, 0.0, 0));
}

#[test]
fn registering_does_not_adopt_a_held_legacy_signal() {
    let mut f = Fixture::new(0);
    let mut session = ExecutionSession::from_semantic_store(&f.store).unwrap();
    session
        .set_native_state_input(
            NativeStateSource::PointerButton { button: 0 },
            NativeInputValue::Bool(true),
        )
        .unwrap();
    let before = session.publication_context();
    assert_eq!(
        session.configure_native_pointer_input(POINTER, 0),
        Err(ExecutionSessionPointerInputError::PointerStillPressed)
    );
    assert_eq!(session.publication_context(), before);
    assert!(session.native_pointer_context().is_none());
    session
        .set_native_state_input(
            NativeStateSource::PointerButton { button: 0 },
            NativeInputValue::Bool(false),
        )
        .unwrap();
    session.configure_native_pointer_input(POINTER, 0).unwrap();
    f.session = session;
    f.runtime = f.session.runtime_identity();
    let _ = f.accept(0, press(1.0, 0.0, 0));
}

#[test]
fn pending_callback_backpressure_preserves_press_and_sequence_for_retry() {
    let mut f = Fixture::new(0);
    let input = f.input(0, press(1.0, 0.0, 0));
    let before = f.session.publication_context();
    let overlay = f
        .session
        .begin_required_callback_phase(0.0, [f.moving])
        .unwrap();
    for _ in 0..1000 {
        assert_eq!(
            f.session.apply_native_pointer_input(f.runtime, input),
            Err(ExecutionSessionPointerInputError::Input(
                ExecutionSessionInputError::RequiredCallbackPending
            ))
        );
    }
    assert_eq!(f.session.publication_context(), before);
    assert_eq!(f.value(f.down), ReactiveValue::Scalar(0.0));
    assert_eq!(f.value(f.pressed), ReactiveValue::Bool(false));
    assert_eq!(f.session.pending_callback_token(), Some(overlay.token()));
    f.session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    // Empty same-time callback keeps the original occurrence's captured frame valid.
    assert_eq!(f.session.publication_context(), before);
    let receipt = f
        .session
        .apply_native_pointer_input(f.runtime, input)
        .unwrap();
    assert_eq!(receipt.input(), input);
    assert_eq!(f.value(f.down), ReactiveValue::Scalar(1.0));
}

#[test]
fn pending_callback_does_not_half_cancel_a_held_pointer() {
    let mut f = Fixture::new(0);
    let _ = f.accept(0, press(1.0, 0.0, 0));
    let input = f.input(
        1,
        NativePointerInputKind::Cancel(NativePointerCancellation::FocusLost),
    );
    let overlay = f
        .session
        .begin_required_callback_phase(0.0, [f.moving])
        .unwrap();
    assert!(matches!(
        f.session.apply_native_pointer_input(f.runtime, input),
        Err(ExecutionSessionPointerInputError::Input(
            ExecutionSessionInputError::RequiredCallbackPending
        ))
    ));
    assert_eq!(f.value(f.pressed), ReactiveValue::Bool(true));
    assert_eq!(
        f.session.configure_native_pointer_input(POINTER, 1),
        Err(ExecutionSessionPointerInputError::PointerStillPressed)
    );
    f.session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    let receipt = f
        .session
        .apply_native_pointer_input(f.runtime, input)
        .unwrap();
    assert_eq!(receipt.input().kind(), input.kind());
    assert_eq!(f.value(f.pressed), ReactiveValue::Bool(false));
    assert_eq!(f.value(f.up), ReactiveValue::Scalar(0.0));
}

#[test]
fn configured_callbacks_remain_explicitly_unsupported_by_direct_pointer_input() {
    let f = Fixture::new(0);
    let mut store = f.store;
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(f.moving, HostCallbackId::new(1), 0.0, None);
    transaction.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.configure_native_pointer_input(POINTER, 0).unwrap();
    let input = record(
        0,
        POINTER,
        session.native_pointer_context().unwrap(),
        press(1.0, 0.0, 0),
    );
    let before = session.publication_context();
    assert_eq!(
        session.apply_native_pointer_input(session.runtime_identity(), input),
        Err(ExecutionSessionPointerInputError::Input(
            ExecutionSessionInputError::RequiredCallbacksConfigured
        ))
    );
    assert_eq!(session.publication_context(), before);
}

#[test]
fn paused_input_wakes_only_affected_rows_and_settles_without_advancing_time() {
    let mut f = Fixture::new(10_000);
    let unrelated_before = f.session.frame().objects[3..].to_vec();
    assert!(!f.session.wake_state().frame_pending());
    assert_eq!(
        f.session.wake_state().timeline(),
        TimelineWakeState::Quiescent
    );
    let _ = f.accept(0, press(2.0, 1.0, 0));
    assert_eq!(f.session.frame().time, 0.0);
    assert_eq!(&f.session.frame().objects[3..], unrelated_before.as_slice());
    assert_eq!(f.session.last_reactive_stats().derived_signals_evaluated, 2);
    assert!(f.session.wake_state().frame_pending());
    let changes = f.session.take_frame_changes();
    assert!(!changes.is_all());
    assert_eq!(changes.object_indices().len(), 2);
    assert!(!f.session.wake_state().frame_pending());
    assert_eq!(
        f.session.wake_state().timeline(),
        TimelineWakeState::Quiescent
    );
}

#[test]
fn a_failed_callback_cannot_be_resumed_by_pointer_input() {
    let mut f = Fixture::new(0);
    let input = f.input(0, press(1.0, 0.0, 0));
    let before = f.session.publication_context();
    let overlay = f
        .session
        .begin_required_callback_phase(0.0, [f.moving])
        .unwrap();
    f.session
        .fail_required_callback_phase(overlay.token())
        .unwrap();
    assert!(matches!(
        f.session.apply_native_pointer_input(f.runtime, input),
        Err(ExecutionSessionPointerInputError::Input(
            ExecutionSessionInputError::RequiredCallbackTerminated(_)
        ))
    ));
    assert_eq!(f.session.publication_context(), before);
    assert_eq!(f.value(f.down), ReactiveValue::Scalar(0.0));
    assert_eq!(f.value(f.pressed), ReactiveValue::Bool(false));
    assert!(f.session.take_frame_changes().is_empty());
}

#[test]
fn unconfigured_input_is_rejected_without_selecting_a_pointer_implicitly() {
    let f = Fixture::new(0);
    let mut session = ExecutionSession::from_semantic_store(&f.store).unwrap();
    let before = session.publication_context();
    assert_eq!(
        session
            .apply_native_pointer_input(session.runtime_identity(), f.input(0, press(1.0, 0.0, 0))),
        Err(ExecutionSessionPointerInputError::Unconfigured)
    );
    assert_eq!(session.publication_context(), before);
    assert!(session.native_pointer_context().is_none());
}
