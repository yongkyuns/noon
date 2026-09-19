use super::*;
use noon::ExecutionSession;
use noon_core::{
    NativeEventSource, NativeInputValue, NativeStateSource, ReactiveValue,
    SemanticMutationTransaction, SemanticNativeInputSource, SemanticNodeCreation, SemanticNodeId,
    SemanticObjectState, SemanticSignalValue, SemanticStore, SemanticVec3, StoredGeometry,
};

struct PointerFixture {
    player: SemanticExecutionPlayer,
    position: SemanticNodeId,
    button: SemanticNodeId,
    down: SemanticNodeId,
    up: SemanticNodeId,
}

fn pointer_fixture() -> PointerFixture {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 0.5,
    }));
    store.add_semantic_family_member(root, target).unwrap();
    let mut add_signal = |source, initial| {
        let mut tx = SemanticMutationTransaction::new();
        let pending =
            tx.create_node(SemanticNodeCreation::native_input_signal(initial, source).unwrap());
        tx.scope_signal(root, pending);
        tx.apply(&mut store).unwrap().resolve(pending).unwrap()
    };
    let position = add_signal(
        SemanticNativeInputSource::State(NativeStateSource::PointerPosition),
        SemanticSignalValue::Vec3(SemanticVec3::ZERO),
    );
    let button = add_signal(
        SemanticNativeInputSource::State(NativeStateSource::PointerButton { button: 0 }),
        SemanticSignalValue::Bool(false),
    );
    let down = add_signal(
        SemanticNativeInputSource::Event(NativeEventSource::PointerDown { button: 0 }),
        SemanticSignalValue::Scalar(0.0),
    );
    let up = add_signal(
        SemanticNativeInputSource::Event(NativeEventSource::PointerUp { button: 0 }),
        SemanticSignalValue::Scalar(0.0),
    );
    let session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    PointerFixture {
        player: SemanticExecutionPlayer::from_session(session, 1.0, 1).unwrap(),
        position,
        button,
        down,
        up,
    }
}

fn browser_pointer_json(
    kind: &str,
    x: Option<f32>,
    y: Option<f32>,
    button: Option<u8>,
    view: u64,
) -> String {
    serde_json::json!({
        "kind": kind,
        "source_id": view + 1,
        "pointer_id": 7,
        "surface_x": x,
        "surface_y": y,
        "viewport_width": x.map(|_| 800.0),
        "viewport_height": y.map(|_| 400.0),
        "button": button,
        "view_revision": view,
        "shift": true,
        "control": false,
        "alt": true,
        "meta": false
    })
    .to_string()
}

#[test]
fn browser_pointer_trace_matches_native_scene_mapping_and_edge_semantics() {
    let mut f = pointer_fixture();
    for json in [
        browser_pointer_json("move", Some(200.0), Some(100.0), None, 0),
        browser_pointer_json("press", Some(200.0), Some(100.0), Some(0), 0),
        browser_pointer_json("move", Some(600.0), Some(300.0), None, 0),
        browser_pointer_json("release", Some(600.0), Some(300.0), Some(0), 0),
    ] {
        f.player.submit_browser_pointer_input_json(&json).unwrap();
    }
    assert_eq!(
        f.player.session.effective_signal_value(f.position),
        Some(&ReactiveValue::Vec2(Vec2::new(4.0, -2.0)))
    );
    assert_eq!(
        f.player.session.effective_signal_value(f.button),
        Some(&ReactiveValue::Bool(false))
    );
    assert_eq!(
        f.player.session.effective_signal_value(f.down),
        Some(&ReactiveValue::Scalar(1.0))
    );
    assert_eq!(
        f.player.session.effective_signal_value(f.up),
        Some(&ReactiveValue::Scalar(1.0))
    );
    assert_eq!(f.player.next_native_event_sequence, 4);
    assert_eq!(f.player.session.frame().time, 0.0);
}

#[test]
fn browser_cancel_and_view_rebind_clear_buttons_without_release() {
    let mut f = pointer_fixture();
    f.player
        .submit_browser_pointer_input_json(&browser_pointer_json(
            "press",
            Some(200.0),
            Some(100.0),
            Some(0),
            3,
        ))
        .unwrap();
    f.player
        .submit_browser_pointer_input_json(&browser_pointer_json("focus_lost", None, None, None, 3))
        .unwrap();
    assert_eq!(
        f.player.session.effective_signal_value(f.button),
        Some(&ReactiveValue::Bool(false))
    );
    assert_eq!(
        f.player.session.effective_signal_value(f.up),
        Some(&ReactiveValue::Scalar(0.0))
    );
    f.player
        .submit_browser_pointer_input_json(&browser_pointer_json(
            "press",
            Some(600.0),
            Some(300.0),
            Some(0),
            4,
        ))
        .unwrap();
    assert_eq!(
        f.player.session.effective_signal_value(f.down),
        Some(&ReactiveValue::Scalar(2.0))
    );
    assert_eq!(
        f.player
            .session
            .native_pointer_input_token()
            .unwrap()
            .context()
            .view_revision,
        4
    );
}

#[test]
fn rejected_browser_pointer_input_does_not_acknowledge_sequence() {
    let mut f = pointer_fixture();
    let before = f.player.session.publication_context();
    let invalid = serde_json::json!({
        "kind": "move",
        "source_id": 1,
        "pointer_id": 7,
        "surface_x": 10.0,
        "surface_y": 20.0,
        "viewport_width": 0.0,
        "viewport_height": 400.0,
        "button": null,
        "view_revision": 0
    })
    .to_string();
    assert!(f
        .player
        .submit_browser_pointer_input_json(&invalid)
        .is_err());
    assert_eq!(f.player.next_native_event_sequence, 0);
    assert!(f.player.browser_pointer_binding.is_none());
    // Malformed wire data cannot configure a new source or reset buttons.
    assert_eq!(f.player.session.publication_context(), before);
    f.player
        .submit_browser_pointer_input_json(&browser_pointer_json(
            "move",
            Some(200.0),
            Some(100.0),
            None,
            0,
        ))
        .unwrap();
    assert_eq!(f.player.next_native_event_sequence, 1);
}

fn wire(kind: &str, source: u64, pointer: i32) -> BrowserPointerInputWire {
    let positioned = matches!(kind, "move" | "press" | "release");
    serde_json::from_value(serde_json::json!({
        "kind": kind, "source_id": source, "pointer_id": pointer, "view_revision": 2,
        "surface_x": positioned.then_some(200.0), "surface_y": positioned.then_some(100.0),
        "viewport_width": positioned.then_some(800.0), "viewport_height": positioned.then_some(400.0),
        "button": matches!(kind, "press" | "release").then_some(0),
    })).unwrap()
}

#[test]
fn browser_source_and_signed_pointer_identity_reach_the_shared_session() {
    let mut f = pointer_fixture();
    f.player
        .submit_browser_pointer_input(wire("press", 19, -2))
        .unwrap();
    let token = f.player.session.native_pointer_input_token().unwrap();
    assert_eq!(
        token.pointer(),
        NativePointerId {
            source: 19,
            pointer: u64::from(u32::MAX - 1)
        }
    );
    assert_eq!(token.context().view_revision, 2);
    assert_eq!(f.player.next_native_event_sequence, 1);
}

#[test]
fn foreign_or_retired_source_cannot_release_or_cancel_the_current_pointer() {
    let mut f = pointer_fixture();
    f.player
        .submit_browser_pointer_input(wire("press", 4, 7))
        .unwrap();
    f.player
        .submit_browser_pointer_input(wire("cancel", 4, 7))
        .unwrap();
    f.player
        .submit_browser_pointer_input(wire("press", 5, 8))
        .unwrap();
    let before = f.player.session.publication_context();
    let token = f.player.session.native_pointer_input_token().unwrap();
    for kind in ["release", "cancel", "capture_lost", "focus_lost"] {
        for (source, pointer) in [(4, 7), (5, 7), (6, 99)] {
            assert!(f
                .player
                .submit_browser_pointer_input(wire(kind, source, pointer))
                .is_err());
        }
    }
    assert_eq!(f.player.session.publication_context(), before);
    assert_eq!(
        f.player.session.native_pointer_input_token().unwrap(),
        token
    );
    assert_eq!(f.player.next_native_event_sequence, 3);
    assert_eq!(
        f.player.session.effective_signal_value(f.button),
        Some(&ReactiveValue::Bool(true))
    );
    assert_eq!(
        f.player.session.effective_signal_value(f.up),
        Some(&ReactiveValue::Scalar(0.0))
    );
}

#[test]
fn cancelled_sources_cannot_be_resurrected_by_late_positional_records() {
    let mut f = pointer_fixture();
    f.player
        .submit_browser_pointer_input(wire("press", 1, 7))
        .unwrap();
    let cancel = wire("capture_lost", 1, 7);
    assert_eq!(
        cancel.cancellation(),
        Some(NativePointerCancellation::CaptureLost)
    );
    f.player.submit_browser_pointer_input(cancel).unwrap();
    for kind in ["move", "press", "release"] {
        assert!(f
            .player
            .submit_browser_pointer_input(wire(kind, 1, 7))
            .is_err());
    }
    assert_eq!(f.player.next_native_event_sequence, 2);
    assert_eq!(
        f.player.session.effective_signal_value(f.button),
        Some(&ReactiveValue::Bool(false))
    );
    assert_eq!(
        f.player.session.effective_signal_value(f.up),
        Some(&ReactiveValue::Scalar(0.0))
    );
    f.player
        .submit_browser_pointer_input(wire("press", 2, 7))
        .unwrap();
    assert_eq!(
        f.player.session.effective_signal_value(f.down),
        Some(&ReactiveValue::Scalar(2.0))
    );
}

#[test]
fn stale_view_or_changed_viewport_requires_a_new_source_without_mutation() {
    let mut f = pointer_fixture();
    f.player
        .submit_browser_pointer_input(wire("press", 1, 7))
        .unwrap();
    let before = f.player.session.publication_context();
    let binding = f.player.browser_pointer_binding;
    for revision in [1, 3] {
        let mut input = wire("move", 1, 7);
        input.view_revision = revision;
        assert!(f.player.submit_browser_pointer_input(input).is_err());
    }
    let mut input = wire("move", 1, 7);
    input.viewport_width = Some(1600.0);
    assert!(f.player.submit_browser_pointer_input(input).is_err());
    assert_eq!(f.player.browser_pointer_binding, binding);
    assert_eq!(f.player.session.publication_context(), before);
    assert_eq!(f.player.next_native_event_sequence, 1);
    assert_eq!(
        f.player.session.effective_signal_value(f.button),
        Some(&ReactiveValue::Bool(true))
    );
}

#[test]
fn invalid_rebinding_record_does_not_clear_pressed_state_or_acknowledge_sequence() {
    let mut f = pointer_fixture();
    f.player
        .submit_browser_pointer_input(wire("press", 1, 7))
        .unwrap();
    let mut cases = Vec::new();
    let mut invalid = wire("press", 2, 8);
    invalid.button = None;
    cases.push(invalid);
    for dimension in [0.0, -1.0, f32::INFINITY, f32::NAN] {
        let mut invalid = wire("press", 2, 8);
        invalid.viewport_width = Some(dimension);
        cases.push(invalid);
    }
    let mut invalid = wire("press", 2, 8);
    invalid.surface_x = Some(f32::INFINITY);
    cases.push(invalid);
    let mut invalid = wire("press", 2, 8);
    invalid.surface_x = Some(f32::MAX);
    invalid.viewport_height = Some(f32::MIN_POSITIVE);
    cases.push(invalid);
    let mut invalid = wire("press", 2, 8);
    invalid.source_id = MAX_JS_INTEGER + 1;
    cases.push(invalid);
    let mut invalid = wire("press", 2, 8);
    invalid.view_revision = MAX_JS_INTEGER + 1;
    cases.push(invalid);
    let mut invalid = wire("move", 2, 8);
    invalid.button = Some(0);
    cases.push(invalid);
    let mut invalid = wire("cancel", 1, 7);
    invalid.surface_x = Some(0.0);
    cases.push(invalid);
    cases.push(wire("move", 0, 8));
    cases.push(wire("move", 2, -1));
    let before = f.player.session.publication_context();
    let token = f.player.session.native_pointer_input_token().unwrap();
    let binding = f.player.browser_pointer_binding;
    for input in cases {
        assert!(
            f.player.submit_browser_pointer_input(input).is_err(),
            "{input:?}"
        );
        assert_eq!(f.player.session.publication_context(), before);
        assert_eq!(
            f.player.session.native_pointer_input_token().unwrap(),
            token
        );
        assert_eq!(f.player.browser_pointer_binding, binding);
        assert_eq!(f.player.next_native_event_sequence, 1);
        assert_eq!(
            f.player.session.effective_signal_value(f.button),
            Some(&ReactiveValue::Bool(true))
        );
    }
}

#[test]
fn missing_wire_identity_cannot_bind_or_reset_an_existing_raw_pointer_button() {
    let mut f = pointer_fixture();
    f.player
        .session
        .set_native_state_input(
            NativeStateSource::PointerButton { button: 0 },
            NativeInputValue::Bool(true),
        )
        .unwrap();
    let before = f.player.session.publication_context();
    for field in ["source_id", "pointer_id", "view_revision"] {
        let mut value: serde_json::Value = serde_json::from_str(&browser_pointer_json(
            "press",
            Some(20.0),
            Some(40.0),
            Some(0),
            2,
        ))
        .unwrap();
        value.as_object_mut().unwrap().remove(field);
        assert!(f
            .player
            .submit_browser_pointer_input_json(&value.to_string())
            .is_err());
        assert!(f.player.browser_pointer_binding.is_none());
        assert_eq!(f.player.session.publication_context(), before);
        assert_eq!(
            f.player.session.effective_signal_value(f.button),
            Some(&ReactiveValue::Bool(true))
        );
    }
}

#[test]
fn sequence_exhaustion_precedes_source_configuration() {
    let mut f = pointer_fixture();
    f.player
        .submit_browser_pointer_input(wire("press", 1, 7))
        .unwrap();
    f.player.next_native_event_sequence = u64::MAX;
    let token = f.player.session.native_pointer_input_token().unwrap();
    let before = f.player.session.publication_context();
    assert!(f
        .player
        .submit_browser_pointer_input(wire("move", 2, 8))
        .is_err());
    assert_eq!(
        f.player.session.native_pointer_input_token().unwrap(),
        token
    );
    assert_eq!(f.player.session.publication_context(), before);
    assert_eq!(
        f.player.session.effective_signal_value(f.button),
        Some(&ReactiveValue::Bool(true))
    );
}

#[test]
fn browser_input_classifies_unrecorded_replay_without_blocking_first_execution() {
    let mut f = pointer_fixture();
    f.player
        .session
        .begin_replay_retention(noon_runtime::ReplayLimits::default())
        .unwrap();
    f.player
        .submit_browser_pointer_input_json(&browser_pointer_json(
            "press",
            Some(200.0),
            Some(100.0),
            Some(0),
            0,
        ))
        .unwrap();
    assert_eq!(
        f.player.seal_replay(),
        Err("replay unavailable: UnrecordedInput".to_owned())
    );
    f.player.session.advance_to(1.0).unwrap();
    assert_eq!(f.player.session.frame().time, 1.0);
    assert_eq!(
        f.player.session.effective_signal_value(f.down),
        Some(&ReactiveValue::Scalar(1.0))
    );
}

#[test]
fn browser_no_op_source_setup_does_not_poison_replay() {
    let mut f = pointer_fixture();
    f.player
        .session
        .begin_replay_retention(noon_runtime::ReplayLimits::default())
        .unwrap();
    // Center maps to the existing zero position; all reset buttons are already false.
    f.player
        .submit_browser_pointer_input_json(&browser_pointer_json(
            "move",
            Some(400.0),
            Some(200.0),
            None,
            0,
        ))
        .unwrap();
    assert_eq!(f.player.next_native_event_sequence, 1);
    f.player.seal_replay().unwrap();
}

#[test]
fn sealed_browser_input_does_not_acknowledge_and_can_retry_after_explicit_discard() {
    let mut f = pointer_fixture();
    f.player
        .submit_browser_pointer_input_json(&browser_pointer_json(
            "move",
            Some(400.0),
            Some(200.0),
            None,
            0,
        ))
        .unwrap();
    f.player
        .session
        .begin_replay_retention(noon_runtime::ReplayLimits::default())
        .unwrap();
    f.player.seal_replay().unwrap();
    let input = browser_pointer_json("press", Some(200.0), Some(100.0), Some(0), 0);
    let before = f.player.session.publication_context();
    let binding = f.player.browser_pointer_binding;
    assert!(f.player.submit_browser_pointer_input_json(&input).is_err());
    assert_eq!(f.player.next_native_event_sequence, 1);
    assert_eq!(f.player.browser_pointer_binding, binding);
    assert_eq!(f.player.session.publication_context(), before);
    assert_eq!(
        f.player.session.effective_signal_value(f.button),
        Some(&ReactiveValue::Bool(false))
    );
    f.player.session.discard_replay_retention();
    f.player.submit_browser_pointer_input_json(&input).unwrap();
    assert_eq!(f.player.next_native_event_sequence, 2);
    assert_eq!(
        f.player.session.effective_signal_value(f.down),
        Some(&ReactiveValue::Scalar(1.0))
    );
}
