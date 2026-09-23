//! Actual worker ABI and retained-delta receipt lifecycle. Acknowledgement below
//! models successful render-owner feedback explicitly; it is never inferred from
//! capturing or consuming a delta.
use super::*;
use crate::worker_pointer_presentation::WorkerPointerReceipt;
use crate::{PointerPresentationView, RetainedFamilyExecutionDeltaEnvelope};
use noon_core::{NativeInputValue, NativeStateSource};

fn player() -> SemanticExecutionPlayer {
    let mut scene = noon::Scene::new();
    let mut circle = scene.circle(1.0).unwrap();
    circle.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut p =
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 2.0, 7).unwrap();
    p.pause();
    p.set_pointer_fill_selection(Some(4.0)).unwrap();
    p
}
fn register(p: &mut SemanticExecutionPlayer, revision: u64) {
    p.set_browser_pointer_view_json(
        &serde_json::json!({
            "revision": revision, "width": 800.0, "height": 400.0
        })
        .to_string(),
    )
    .unwrap();
}
fn delta(p: &mut SemanticExecutionPlayer) -> RetainedFamilyExecutionDeltaEnvelope {
    let json = if !p.snapshot_sent {
        p.initial_delta_json().unwrap()
    } else {
        p.drain_delta_json().unwrap().expect("expected publication")
    };
    serde_json::from_str(&json).unwrap()
}
fn receipt(
    delta: &RetainedFamilyExecutionDeltaEnvelope,
    presentation: u64,
) -> WorkerPointerReceipt {
    WorkerPointerReceipt {
        session: delta.retained.session,
        sequence: delta.retained.sequence,
        presentation,
        view_revision: delta.pointer_view.unwrap().revision,
    }
}
fn acknowledge(p: &mut SemanticExecutionPlayer, r: WorkerPointerReceipt) -> bool {
    p.note_pointer_presentation_json(&serde_json::to_string(&r).unwrap())
        .unwrap()
}
fn ready() -> (SemanticExecutionPlayer, WorkerPointerReceipt) {
    let mut p = player();
    register(&mut p, 1);
    let r = receipt(&delta(&mut p), 1);
    assert!(acknowledge(&mut p, r));
    (p, r)
}
fn wire(kind: &str, source: u64, view: u64, x: f32) -> serde_json::Value {
    let positioned = matches!(kind, "press" | "move" | "release");
    serde_json::json!({"kind":kind,"source_id":source,"pointer_id":7,"view_revision":view,
        "surface_x":positioned.then_some(x),"surface_y":positioned.then_some(200.0),
        "viewport_width":positioned.then_some(800.0),"viewport_height":positioned.then_some(400.0),
        "button":matches!(kind,"press"|"release").then_some(0)})
}
fn input(
    p: &mut SemanticExecutionPlayer,
    kind: &str,
    source: u64,
    r: Option<WorkerPointerReceipt>,
    view: u64,
    x: f32,
) -> Result<bool, String> {
    p.submit_browser_pointer_input_json(
        &serde_json::json!({
        "input":wire(kind,source,view,x),"presentation":r})
        .to_string(),
    )
}
fn invalidate(p: &mut SemanticExecutionPlayer, r: WorkerPointerReceipt) -> bool {
    p.invalidate_pointer_presentation_json(&serde_json::to_string(&r).unwrap())
        .unwrap()
}

#[test]
fn capture_and_delta_consumption_do_not_authorize_a_pointer() {
    let mut p = player();
    register(&mut p, 1);
    let d = delta(&mut p);
    let r = receipt(&d, 1);
    assert!(!input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    assert_eq!(p.next_native_event_sequence, 0);
    assert!(p.browser_pointer_binding.is_none());
    assert!(p.session.selected_pointer_target().is_none());
    assert!(acknowledge(&mut p, r));
    // Even after presentation, the old rejected source is never replayed.
    assert!(!input(&mut p, "release", 1, Some(r), 1, 400.0).unwrap());
    assert!(input(&mut p, "press", 2, Some(r), 1, 400.0).unwrap());
    assert!(input(&mut p, "release", 2, Some(r), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}
#[test]
fn collected_without_a_receipt_stays_rejected_after_presentation() {
    let (mut p, _) = ready();
    assert!(!input(&mut p, "press", 1, None, 1, 400.0).unwrap());
    assert_eq!(p.next_native_event_sequence, 0);
}
#[test]
fn valid_receipt_projects_selection_without_authored_or_resource_changes() {
    let (mut p, r) = ready();
    let frame = p.session.frame().clone();
    let publication = p.session.publication_context();
    let resources = p.resource_bundle_bytes();
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    assert!(input(&mut p, "release", 1, Some(r), 1, 400.0).unwrap());
    let d = delta(&mut p);
    assert!(d.selection_overlay.is_some());
    assert!(d.retained.objects.is_empty());
    assert!(d.resource_additions.is_none());
    assert_eq!(p.session.frame(), &frame);
    assert_eq!(p.session.publication_context(), publication);
    assert_eq!(p.resource_bundle_bytes(), resources);
    assert!(p.drain_delta_json().unwrap().is_none());
}
#[test]
fn old_collected_receipt_is_not_retagged_after_newer_presentation() {
    let (mut p, a) = ready();
    assert!(input(&mut p, "press", 1, Some(a), 1, 400.0).unwrap());
    // An unpresented execution round trip changes identity without changing pixels.
    p.session.advance_to(0.25).unwrap();
    p.session.seek(0.0).unwrap();
    let b = receipt(&delta(&mut p), 2);
    assert!(acknowledge(&mut p, b));
    assert!(!input(&mut p, "release", 1, Some(a), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_none());
    let ack = p.next_native_event_sequence;
    assert!(!input(&mut p, "press", 1, Some(b), 1, 400.0).unwrap());
    assert_eq!(
        p.next_native_event_sequence, ack,
        "rejected packet tail cannot acknowledge input"
    );
    assert!(input(&mut p, "press", 2, Some(b), 1, 400.0).unwrap());
    assert!(input(&mut p, "release", 2, Some(b), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}
#[test]
fn unchanged_geometry_does_not_make_an_unpresented_revision_current() {
    let (mut p, r) = ready();
    p.session.advance_to(0.25).unwrap();
    p.session.seek(0.0).unwrap();
    assert!(!input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_none());
    assert!(p.drain_delta_json().unwrap().is_some());
}
#[test]
fn foreign_future_or_wrong_view_receipts_cannot_authorize_or_replace_current() {
    let (mut p, r) = ready();
    for bad in [
        WorkerPointerReceipt { session: 8, ..r },
        WorkerPointerReceipt { sequence: 999, ..r },
        WorkerPointerReceipt {
            view_revision: 2,
            ..r
        },
    ] {
        assert!(!acknowledge(&mut p, bad));
    }
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    assert!(input(&mut p, "release", 1, Some(r), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}
#[test]
fn issued_newer_delta_retires_previous_receipt_even_before_presentation() {
    let (mut p, a) = ready();
    p.session.advance_to(0.25).unwrap();
    let b = receipt(&delta(&mut p), 2);
    assert!(!acknowledge(&mut p, a));
    assert!(!input(&mut p, "press", 1, Some(a), 1, 400.0).unwrap());
    assert!(acknowledge(&mut p, b));
    assert!(input(&mut p, "press", 2, Some(b), 1, 400.0).unwrap());
}
#[test]
fn surface_invalidation_still_cancels_when_newer_delta_is_in_flight() {
    let (mut p, a) = ready();
    assert!(input(&mut p, "press", 1, Some(a), 1, 400.0).unwrap());
    // Issue a presentation-only delta without mutating the session gesture.
    let snapshot = p.worker_pointer_presentation.capture(&p.session).unwrap();
    p.worker_pointer_presentation
        .issue(7, a.sequence + 1, snapshot);
    assert!(invalidate(&mut p, a));
    let b = WorkerPointerReceipt {
        sequence: a.sequence + 1,
        presentation: 2,
        ..a
    };
    assert!(acknowledge(&mut p, b));
    assert!(input(&mut p, "release", 1, Some(b), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_none());
}
#[test]
fn same_delta_repaint_does_not_reuse_an_old_presentation_receipt() {
    let (mut p, a) = ready();
    assert!(input(&mut p, "press", 1, Some(a), 1, 400.0).unwrap());
    assert!(invalidate(&mut p, a));
    let b = WorkerPointerReceipt {
        presentation: 2,
        ..a
    };
    assert!(acknowledge(&mut p, b));
    assert!(!acknowledge(&mut p, a));
    assert!(!input(&mut p, "release", 1, Some(a), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_none());
}
#[test]
fn old_invalidation_cannot_cancel_a_newer_presented_gesture() {
    let (mut p, a) = ready();
    let b = WorkerPointerReceipt {
        presentation: 2,
        ..a
    };
    assert!(acknowledge(&mut p, b));
    assert!(input(&mut p, "press", 1, Some(b), 1, 400.0).unwrap());
    assert!(!invalidate(&mut p, a));
    assert!(input(&mut p, "release", 1, Some(b), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}
#[test]
fn view_registration_is_not_a_receipt_and_old_view_cannot_return() {
    let (mut p, a) = ready();
    assert!(input(&mut p, "press", 1, Some(a), 1, 400.0).unwrap());
    register(&mut p, 2);
    let d = delta(&mut p);
    assert_eq!(d.pointer_view.unwrap().revision, 2);
    assert!(!acknowledge(&mut p, a));
    assert!(!input(&mut p, "press", 2, Some(a), 2, 400.0).unwrap());
    let b = receipt(&d, 2);
    assert!(acknowledge(&mut p, b));
    assert!(input(&mut p, "press", 3, Some(b), 2, 400.0).unwrap());
    assert!(input(&mut p, "release", 3, Some(b), 2, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}
#[test]
fn invalid_view_does_not_reset_valid_receipt_or_pressed_state() {
    let (mut p, r) = ready();
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    for value in [
        serde_json::json!({"revision":0,"width":800,"height":400}),
        serde_json::json!({"revision":1,"width":801,"height":400}),
        serde_json::json!({"revision":2,"width":-1,"height":400}),
        serde_json::json!({"revision":9007199254740992_u64,"width":800,"height":400}),
    ] {
        assert!(p.set_browser_pointer_view_json(&value.to_string()).is_err());
    }
    assert!(input(&mut p, "release", 1, Some(r), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}
#[test]
fn hidden_view_emits_clear_mapping_without_minting_a_receipt() {
    let (mut p, r) = ready();
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    p.set_browser_pointer_view_json(r#"{"revision":2,"width":0,"height":0}"#)
        .unwrap();
    let d = delta(&mut p);
    assert!(d.pointer_view.is_none());
    assert!(!acknowledge(&mut p, r));
    assert!(p.drain_delta_json().unwrap().is_none());
    assert!(p.session.selected_pointer_target().is_none());
}
#[test]
fn cancellation_does_not_require_a_positional_receipt() {
    let (mut p, r) = ready();
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    p.session.advance_to(0.25).unwrap();
    assert!(input(&mut p, "cancel", 1, None, 1, 400.0).unwrap());
    assert!(input(&mut p, "release", 1, Some(r), 1, 400.0).is_err());
    assert!(p.session.selected_pointer_target().is_none());
}
#[test]
fn malformed_or_foreign_rejected_tail_is_not_silently_dropped() {
    let (mut p, r) = ready();
    assert!(!input(&mut p, "press", 1, None, 1, 400.0).unwrap());
    let mut foreign = wire("move", 1, 1, 400.0);
    foreign["pointer_id"] = 8.into();
    assert!(p
        .submit_browser_pointer_input_json(
            &serde_json::json!({"input":foreign,"presentation":r}).to_string()
        )
        .is_err());
    let mut malformed = wire("move", 1, 1, 400.0);
    malformed["button"] = 1.into();
    assert!(p
        .submit_browser_pointer_input_json(
            &serde_json::json!({"input":malformed,"presentation":r}).to_string()
        )
        .is_err());
    assert_eq!(p.next_native_event_sequence, 0);
}
#[test]
fn invalid_envelopes_fail_without_source_configuration() {
    let (mut p, r) = ready();
    for value in [
        wire("press", 1, 1, 400.0),
        serde_json::json!({"input":wire("press",1,1,400.0),"presentation":r,"unknown":1}),
        serde_json::json!({"input":wire("press",1,1,400.0),"presentation":{"session":7,"sequence":0,"presentation":0,"view_revision":1}}),
    ] {
        assert!(p
            .submit_browser_pointer_input_json(&value.to_string())
            .is_err());
        assert!(p.browser_pointer_binding.is_none());
        assert_eq!(p.next_native_event_sequence, 0);
    }
}
#[test]
fn sequence_exhaustion_cannot_mutate_rejection_cleanup() {
    let (mut p, r) = ready();
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    p.next_native_event_sequence = u64::MAX;
    let before = p.session.publication_context();
    let binding = p.browser_pointer_binding;
    assert!(input(&mut p, "release", 1, None, 1, 400.0).is_err());
    assert_eq!(p.session.publication_context(), before);
    assert_eq!(p.browser_pointer_binding, binding);
}
#[test]
fn rebind_transport_never_reuses_a_receipt_from_the_old_session() {
    let (mut p, r) = ready();
    p.rebind_transport(2.0, 8).unwrap();
    assert!(!acknowledge(&mut p, r));
    register(&mut p, 2);
    let b = receipt(&delta(&mut p), 2);
    assert_eq!(b.session, 8);
    assert!(acknowledge(&mut p, b));
    assert!(!input(&mut p, "press", 1, Some(r), 2, 400.0).unwrap());
    assert!(input(&mut p, "press", 2, Some(b), 2, 400.0).unwrap());
}
#[test]
fn signal_only_change_requires_a_new_issued_and_presented_receipt() {
    let mut f = super::tests::pointer_fixture();
    let p = &mut f.player;
    register(p, 1);
    let a = receipt(&delta(p), 1);
    assert!(acknowledge(p, a));
    // Pointer position has no visual dependents in this fixture.
    p.session
        .set_native_state_input(
            NativeStateSource::PointerPosition,
            NativeInputValue::Vec2(Vec2::new(2.0, 3.0)),
        )
        .unwrap();
    assert!(p.session.take_renderer_publication().changes().is_empty());
    let b = receipt(&delta(p), 2);
    assert!(!input(p, "press", 1, Some(a), 1, 400.0).unwrap());
    assert!(acknowledge(p, b));
    assert!(input(p, "press", 2, Some(b), 1, 400.0).unwrap());
}
#[test]
fn retained_pointer_view_validation_rejects_invalid_mapping_before_mirror_admission() {
    let (mut p, _) = ready();
    p.session.advance_to(0.25).unwrap();
    let mut d = delta(&mut p);
    d.pointer_view = Some(PointerPresentationView {
        revision: 1,
        width: 0.0,
        height: 400.0,
    });
    assert!(d.validate().is_err());
    d.pointer_view = Some(PointerPresentationView {
        revision: 1,
        width: 800.0,
        height: 400.0,
    });
    assert!(d.validate().is_ok());
}

#[test]
fn invalidated_receipt_cannot_be_resurrected_before_repaint() {
    let (mut p, a) = ready();
    assert!(input(&mut p, "press", 1, Some(a), 1, 400.0).unwrap());
    assert!(invalidate(&mut p, a));
    let acknowledged_sequence = p.next_native_event_sequence;
    assert!(
        !acknowledge(&mut p, a),
        "late duplicate is not a replacement presentation"
    );
    assert_eq!(p.next_native_event_sequence, acknowledged_sequence);
    assert!(!input(&mut p, "release", 1, Some(a), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_none());
    let b = receipt(&delta(&mut p), 2);
    assert!(acknowledge(&mut p, b));
    assert!(input(&mut p, "press", 2, Some(b), 1, 400.0).unwrap());
    assert!(input(&mut p, "release", 2, Some(b), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}

#[test]
fn registered_view_without_pointer_subscribers_preserves_quiet_presentation() {
    let mut p = player();
    p.set_pointer_fill_selection(None).unwrap();
    assert!(!p.session.has_native_pointer_subscribers());
    register(&mut p, 1);
    let displayed = delta(&mut p);
    let r = receipt(&displayed, 1);
    assert!(acknowledge(&mut p, r));
    let publication = p.session.publication_context();

    for time in [0.25, 0.5, 1.0] {
        p.session.advance_to(time).unwrap();
        assert_eq!(p.time(), time);
        assert_ne!(p.session.publication_context(), publication);
        assert!(
            p.drain_delta_json().unwrap().is_none(),
            "a dormant pointer view must not republish a visually quiet sample"
        );
    }
}

#[test]
fn enabling_selection_after_quiet_samples_requires_fresh_presentation() {
    let mut p = player();
    p.set_pointer_fill_selection(None).unwrap();
    register(&mut p, 1);
    let old = receipt(&delta(&mut p), 1);
    assert!(acknowledge(&mut p, old));
    p.session.advance_to(0.5).unwrap();
    assert!(p.drain_delta_json().unwrap().is_none());

    p.set_pointer_fill_selection(Some(4.0)).unwrap();
    assert!(p.session.has_native_pointer_subscribers());
    let fresh = receipt(&delta(&mut p), 2);
    assert_ne!(fresh.sequence, old.sequence);
    assert!(!input(&mut p, "press", 1, Some(old), 1, 400.0).unwrap());
    assert!(acknowledge(&mut p, fresh));
    assert!(!input(&mut p, "release", 1, Some(fresh), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_none());
    assert!(input(&mut p, "press", 2, Some(fresh), 1, 400.0).unwrap());
    assert!(input(&mut p, "release", 2, Some(fresh), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}

#[test]
fn disabling_last_pointer_subscriber_clears_once_then_settles() {
    let (mut p, r) = ready();
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    assert!(input(&mut p, "release", 1, Some(r), 1, 400.0).unwrap());
    let selected = delta(&mut p);
    assert!(selected.selection_overlay.is_some());
    assert!(acknowledge(&mut p, receipt(&selected, 2)));

    p.set_pointer_fill_selection(None).unwrap();
    assert!(!p.session.has_native_pointer_subscribers());
    let cleared = delta(&mut p);
    assert!(cleared.selection_overlay.is_none());
    assert!(acknowledge(&mut p, receipt(&cleared, 3)));
    p.session.advance_to(0.5).unwrap();
    assert!(p.drain_delta_json().unwrap().is_none());
}
