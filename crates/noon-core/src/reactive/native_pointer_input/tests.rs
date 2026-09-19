use crate::{
    NativeEventOccurrence, NativeEventSource, NativeInputModifiers, NativeInputRuntimeError,
    NativeInputValue, NativePointerCancellation, NativePointerContext, NativePointerId,
    NativePointerInput, NativePointerInputKind, NativePointerPosition, NativeStateSource,
    PublicationContext, Vec2,
};

fn context(view_revision: u64) -> NativePointerContext {
    NativePointerContext {
        publication: PublicationContext::default(),
        view_revision,
    }
}

fn position(x: f32, y: f32) -> NativePointerPosition {
    NativePointerPosition::new(Vec2::new(x, y), Vec2::new(x * 100.0, -y * 100.0)).unwrap()
}

fn input(sequence: u64, kind: NativePointerInputKind) -> NativePointerInput {
    NativePointerInput::new(
        sequence,
        NativePointerId {
            source: 1,
            pointer: 7,
        },
        context(3),
        NativeInputModifiers::default(),
        kind,
    )
}

#[test]
fn rejects_non_finite_values_in_every_coordinate_lane() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for lane in 0..4 {
            let mut values = [0.0; 4];
            values[lane] = bad;
            assert_eq!(
                NativePointerPosition::new(
                    Vec2::new(values[0], values[1]),
                    Vec2::new(values[2], values[3]),
                ),
                Err(NativeInputRuntimeError::NonFiniteValue),
            );
        }
    }
}

#[test]
fn captured_positions_outside_the_surface_are_not_clamped() {
    let point =
        NativePointerPosition::new(Vec2::new(-200.0, 500.0), Vec2::new(-2000.0, 5000.0)).unwrap();
    assert_eq!(point.scene(), Vec2::new(-200.0, 500.0));
    assert_eq!(point.surface(), Vec2::new(-2000.0, 5000.0));
}

#[test]
fn move_has_one_position_update_and_no_discrete_button_event() {
    let sample = input(1, NativePointerInputKind::Move(position(2.0, -1.0)));
    let updates: Vec<_> = sample.state_updates().collect();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].source, NativeStateSource::PointerPosition);
    assert_eq!(
        updates[0].value,
        NativeInputValue::Vec2(Vec2::new(2.0, -1.0))
    );
    assert_eq!(sample.button_event(), None);
}

#[test]
fn press_keeps_its_position_after_a_later_pointer_sample() {
    let press = input(
        10,
        NativePointerInputKind::Press {
            position: position(1.0, 2.0),
            button: 0,
        },
    );
    let later_move = input(11, NativePointerInputKind::Move(position(4.0, 5.0)));
    // A latest-value consumer may retain only the later sample. The ordered
    // press remains self-contained and does not consult that sampled state.
    let latest_position = later_move.position().unwrap();
    assert_eq!(latest_position.scene(), Vec2::new(4.0, 5.0));
    assert_eq!(press.position().unwrap().scene(), Vec2::new(1.0, 2.0));
    assert_eq!(
        press.position().unwrap().surface(),
        Vec2::new(100.0, -200.0)
    );
    let projected: Vec<_> = press.state_updates().collect();
    assert_eq!(
        projected[0].value,
        NativeInputValue::Vec2(Vec2::new(1.0, 2.0))
    );
}

#[test]
fn press_and_release_project_their_own_state_and_ordered_event() {
    let press = input(
        10,
        NativePointerInputKind::Press {
            position: position(1.0, 2.0),
            button: 2,
        },
    );
    let release = input(
        12,
        NativePointerInputKind::Release {
            position: position(4.0, 5.0),
            button: 2,
        },
    );
    for (record, pressed, point, expected_event) in [
        (
            press,
            true,
            Vec2::new(1.0, 2.0),
            NativeEventOccurrence::new(10, NativeEventSource::PointerDown { button: 2 }),
        ),
        (
            release,
            false,
            Vec2::new(4.0, 5.0),
            NativeEventOccurrence::new(12, NativeEventSource::PointerUp { button: 2 }),
        ),
    ] {
        let updates: Vec<_> = record.state_updates().collect();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].source, NativeStateSource::PointerPosition);
        assert_eq!(updates[0].value, NativeInputValue::Vec2(point));
        assert_eq!(
            updates[1].source,
            NativeStateSource::PointerButton { button: 2 }
        );
        assert_eq!(updates[1].value, NativeInputValue::Bool(pressed));
        assert_eq!(record.button_event(), Some(expected_event));
    }
}

#[test]
fn repeated_identical_button_occurrences_keep_distinct_sequences() {
    let kind = NativePointerInputKind::Press {
        position: position(0.0, 0.0),
        button: 0,
    };
    let events: Vec<_> = (100..104)
        .map(|sequence| input(sequence, kind).button_event().unwrap())
        .collect();
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![100, 101, 102, 103]
    );
    assert!(events
        .iter()
        .all(|event| event.source == NativeEventSource::PointerDown { button: 0 }));
}

#[test]
fn button_codes_are_not_interpreted_as_a_dom_buttons_mask() {
    let event = input(
        1,
        NativePointerInputKind::Press {
            position: position(0.0, 0.0),
            button: 7,
        },
    );
    assert_eq!(
        event.button_event().unwrap().source,
        NativeEventSource::PointerDown { button: 7 },
    );
    assert_eq!(
        event.state_updates().nth(1).unwrap().source,
        NativeStateSource::PointerButton { button: 7 },
    );
}

#[test]
fn cancellations_never_fabricate_a_release_or_origin_position() {
    for reason in [
        NativePointerCancellation::Cancelled,
        NativePointerCancellation::CaptureLost,
        NativePointerCancellation::CaptureFailed,
        NativePointerCancellation::FocusLost,
    ] {
        let event = input(12, NativePointerInputKind::Cancel(reason));
        assert_eq!(event.kind(), NativePointerInputKind::Cancel(reason));
        assert_eq!(event.position(), None);
        assert_eq!(event.button_event(), None);
        assert_eq!(event.state_updates().count(), 0);
    }
}

#[test]
fn pointer_ids_are_scoped_by_input_source() {
    let first = NativePointerId {
        source: 1,
        pointer: 7,
    };
    let second = NativePointerId {
        source: 2,
        pointer: 7,
    };
    assert_ne!(first, second);
    assert_eq!(
        input(1, NativePointerInputKind::Move(position(0.0, 0.0))).pointer(),
        first
    );
}

#[test]
fn context_and_modifiers_are_occurrence_local_snapshots() {
    let mut view = context(3);
    let mut modifiers = NativeInputModifiers {
        shift: true,
        control: true,
        alt: false,
        meta: true,
    };
    let event = NativePointerInput::new(
        10,
        NativePointerId {
            source: 1,
            pointer: 7,
        },
        view,
        modifiers,
        NativePointerInputKind::Move(position(1.0, 2.0)),
    );
    view.view_revision = 4;
    modifiers.shift = false;
    assert_eq!(event.context(), context(3));
    assert_ne!(event.context(), view);
    assert!(event.modifiers().shift);
    assert!(event.modifiers().control);
    assert!(!event.modifiers().alt);
    assert!(event.modifiers().meta);
    assert_ne!(event.modifiers(), modifiers);
}

#[test]
fn sequence_projection_never_wraps_or_truncates() {
    let event = input(
        u64::MAX,
        NativePointerInputKind::Release {
            position: position(0.0, 0.0),
            button: 0,
        },
    );
    assert_eq!(event.sequence(), u64::MAX);
    assert_eq!(event.button_event().unwrap().sequence, u64::MAX);
}

#[test]
fn motion_records_preserve_out_and_back_evidence_for_the_session() {
    let records = [
        input(1, NativePointerInputKind::Move(position(0.0, 0.0))),
        input(2, NativePointerInputKind::Move(position(10.0, 0.0))),
        input(3, NativePointerInputKind::Move(position(0.0, 0.0))),
    ];
    let points: Vec<_> = records
        .iter()
        .map(|event| event.position().unwrap().surface())
        .collect();
    assert_eq!(points, vec![Vec2::ZERO, Vec2::new(1000.0, 0.0), Vec2::ZERO]);
}
