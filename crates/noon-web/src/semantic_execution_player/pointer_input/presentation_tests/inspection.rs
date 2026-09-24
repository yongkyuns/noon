use super::*;

fn scroll(
    p: &mut SemanticExecutionPlayer,
    r: Option<WorkerPointerReceipt>,
    pixels: f64,
) -> Result<Option<bool>, String> {
    scroll_at(p, r, pixels, 500.0)
}
fn scroll_at(
    p: &mut SemanticExecutionPlayer,
    r: Option<WorkerPointerReceipt>,
    pixels: f64,
    x: f32,
) -> Result<Option<bool>, String> {
    p.scroll_inspection_view_json(
        &serde_json::json!({
            "view_revision": 1, "viewport_width": 800.0, "viewport_height": 400.0,
            "surface_x": x, "surface_y": 200.0, "delta_pixels": pixels, "presentation": r,
        })
        .to_string(),
    )
}
fn zoom_in() -> f64 {
    -500.0 * 2.0_f64.ln()
}

#[test]
fn zoom_transports_one_composed_view_without_scene_rows_or_authored_time() {
    let (mut p, old) = ready();
    let publication = p.session.publication_context();
    let original = p.session.frame().clone();
    assert_eq!(scroll(&mut p, Some(old), zoom_in()), Ok(Some(true)));
    assert_eq!(p.session.publication_context(), publication);
    assert_eq!(p.session.frame(), &original);
    assert_eq!(
        p.session.camera().unwrap(),
        noon_core::Camera2DState::default()
    );
    let d = delta(&mut p);
    assert!(!d.retained.snapshot);
    assert!(d.retained.objects.is_empty());
    assert!(d.retained.removed_slots.is_empty());
    assert_eq!(d.retained.time, 0.0);
    assert_eq!(d.retained.camera.height, 4.0);
    assert_eq!(d.retained.camera.center, Vec2::new(1.0, 0.0));
    let new = receipt(&d, 2);
    assert!(acknowledge(&mut p, new));
    // A point outside the original unit circle but inside its enlarged image.
    assert!(input(&mut p, "press", 1, Some(new), 1, 250.0).unwrap());
    assert!(input(&mut p, "release", 1, Some(new), 1, 250.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}

#[test]
fn capture_consumption_and_delayed_acknowledgement_never_authorize_old_scroll() {
    let mut p = player();
    register(&mut p, 1);
    let d = delta(&mut p);
    let old = receipt(&d, 1);
    assert_eq!(scroll(&mut p, None, zoom_in()), Ok(None));
    assert_eq!(scroll(&mut p, Some(old), zoom_in()), Ok(None));
    assert_eq!(p.session.inspection_view_revision(), 0);
    assert!(acknowledge(&mut p, old));
    assert_eq!(scroll(&mut p, None, zoom_in()), Ok(None));
    assert_eq!(scroll(&mut p, Some(old), zoom_in()), Ok(Some(true)));
    let new = receipt(&delta(&mut p), 2);
    assert!(acknowledge(&mut p, new));
    assert_eq!(scroll(&mut p, Some(old), zoom_in()), Ok(None));
    assert!(
        p.drain_delta_json().unwrap().is_none(),
        "stale packet must not create repaint loop"
    );
}

#[test]
fn old_scroll_and_old_presentation_stay_retired_after_camera_aba() {
    let (mut p, old) = ready();
    scroll(&mut p, Some(old), zoom_in()).unwrap();
    assert!(!acknowledge(&mut p, old));
    let mid = receipt(&delta(&mut p), 2);
    assert!(acknowledge(&mut p, mid));
    scroll(&mut p, Some(mid), -zoom_in()).unwrap();
    let d = delta(&mut p);
    let new = receipt(&d, 3);
    assert!(acknowledge(&mut p, new));
    assert_eq!(d.retained.camera, noon_core::Camera2DState::default());
    assert_eq!(p.session.inspection_view_revision(), 2);
    assert!(!acknowledge(&mut p, old));
    assert_eq!(scroll(&mut p, Some(old), zoom_in()), Ok(None));
    assert_eq!(scroll(&mut p, Some(new), zoom_in()), Ok(Some(true)));
}

#[test]
fn changed_view_discards_in_flight_contact_tail_without_second_cancellation() {
    let (mut p, r) = ready();
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    let sequence = p.next_native_event_sequence;
    scroll(&mut p, Some(r), zoom_in()).unwrap();
    for kind in ["move", "release", "cancel"] {
        assert!(!input(&mut p, kind, 1, Some(r), 1, 400.0).unwrap());
    }
    assert_eq!(p.next_native_event_sequence, sequence);
    assert!(p.session.selected_pointer_target().is_none());
    let fresh = receipt(&delta(&mut p), 2);
    assert!(acknowledge(&mut p, fresh));
    assert!(!input(&mut p, "release", 1, Some(fresh), 1, 300.0).unwrap());
    assert!(input(&mut p, "press", 2, Some(fresh), 1, 300.0).unwrap());
    assert!(input(&mut p, "release", 2, Some(fresh), 1, 300.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}

#[test]
fn no_op_preserves_the_displayed_receipt_and_pending_click() {
    let (mut p, r) = ready();
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    assert_eq!(scroll(&mut p, Some(r), 0.0), Ok(Some(false)));
    assert_eq!(p.session.inspection_view_revision(), 0);
    assert!(p.drain_delta_json().unwrap().is_none());
    assert!(input(&mut p, "release", 1, Some(r), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}

#[test]
fn malformed_foreign_and_resized_scroll_do_not_mutate_or_cancel() {
    let (mut p, r) = ready();
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    let before = p.session.frame().clone();
    let sequence = p.next_native_event_sequence;
    for bad in [
        r#"{}"#,
        r#"{"delta_pixels":1e999}"#,
        r#"{"view_revision":1,"viewport_width":0,"viewport_height":400,"surface_x":0,"surface_y":0,"delta_pixels":1,"presentation":null}"#,
        r#"{"view_revision":1,"viewport_width":800,"viewport_height":400,"surface_x":0,"surface_y":0,"delta_pixels":1,"presentation":null,"extra":1}"#,
    ] {
        assert!(p.scroll_inspection_view_json(bad).is_err());
    }
    let mut foreign = r;
    foreign.session += 1;
    assert_eq!(scroll(&mut p, Some(foreign), zoom_in()), Ok(None));
    let bad_view = serde_json::json!({"view_revision":2,"viewport_width":800.0,"viewport_height":400.0,
        "surface_x":500.0,"surface_y":200.0,"delta_pixels":zoom_in(),"presentation":r});
    assert_eq!(
        p.scroll_inspection_view_json(&bad_view.to_string()),
        Ok(None)
    );
    assert_eq!(p.session.frame(), &before);
    assert_eq!(p.next_native_event_sequence, sequence);
    assert_eq!(p.session.inspection_view_revision(), 0);
    assert!(input(&mut p, "release", 1, Some(r), 1, 400.0).unwrap());
    assert!(p.session.selected_pointer_target().is_some());
}

#[test]
fn obsolete_quiet_view_refreshes_once_without_replaying_the_wheel() {
    let mut p = player();
    p.set_pointer_fill_selection(None).unwrap();
    register(&mut p, 1);
    let old = receipt(&delta(&mut p), 1);
    assert!(acknowledge(&mut p, old));
    p.session.advance_to(0.25).unwrap();
    assert_eq!(scroll(&mut p, Some(old), zoom_in()), Ok(None));
    assert_eq!(p.session.inspection_view_revision(), 0);
    let new = receipt(&delta(&mut p), 2);
    assert!(acknowledge(&mut p, new));
    assert!(p.drain_delta_json().unwrap().is_none());
    assert_eq!(scroll(&mut p, Some(new), zoom_in()), Ok(Some(true)));
    assert_eq!(p.session.frame().time, 0.25);
}

#[test]
fn replay_seal_allows_view_only_scroll_and_rebind_preserves_only_the_adjustment() {
    let (mut p, r) = ready();
    p.session
        .begin_replay_retention(noon_runtime::ReplayLimits::default())
        .unwrap();
    p.session.seal_replay().unwrap();
    let stats = p.session.replay_stats();
    assert_eq!(scroll(&mut p, Some(r), zoom_in()), Ok(Some(true)));
    assert_eq!(p.session.replay_stats(), stats);
    let expected = p.session.inspection_camera().unwrap();
    p.rebind_transport(2.0, 8).unwrap();
    register(&mut p, 1);
    let d = delta(&mut p);
    assert_eq!(d.retained.camera, expected);
    assert!(!acknowledge(&mut p, r));
    assert_eq!(scroll(&mut p, Some(r), zoom_in()), Ok(None));
}

#[test]
fn worker_inspection_during_real_indicate_preserves_restoration_and_authored_guard() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut session = scene.execution_session().unwrap();
    let original = scene.live(&mut session).effective(&circle).unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_indicate(
            &circle,
            noon::IndicateOptions::default(),
            noon::AnimationOptions::new().run_time(1.0),
        )
        .unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let midpoint = scene.live(&mut session).effective(&circle).unwrap();
    assert!(midpoint.transform.scale.x > original.transform.scale.x);
    let mut p = SemanticExecutionPlayer::from_session(session, 2.0, 7).unwrap();
    p.pause();
    register(&mut p, 1);
    let r = receipt(&delta(&mut p), 1);
    assert!(acknowledge(&mut p, r));
    assert_eq!(scroll(&mut p, Some(r), zoom_in()), Ok(Some(true)));
    assert_eq!(p.session.frame().time, 0.5);
    assert!(scene
        .live(&mut p.session)
        .set_translation(&circle, 9.0, 0.0)
        .is_err());
    let inspection = p.session.inspection_camera().unwrap();
    scene
        .live(&mut p.session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene
        .live(&mut p.session)
        .complete_segment(segment)
        .unwrap();
    let final_state = scene.live(&mut p.session).effective(&circle).unwrap();
    assert_eq!(final_state.transform, original.transform);
    assert_eq!(final_state.style, original.style);
    assert_eq!(delta(&mut p).retained.camera, inspection);
}

#[test]
fn worker_scroll_does_not_bypass_a_pending_callback_and_retries_after_completion() {
    let (mut p, r) = ready();
    assert!(input(&mut p, "press", 1, Some(r), 1, 400.0).unwrap());
    let binding = p.browser_pointer_binding;
    let phase = p.session.begin_required_callback_phase(0.0, []).unwrap();
    assert!(scroll(&mut p, Some(r), zoom_in()).is_err());
    assert_eq!(p.browser_pointer_binding, binding);
    assert_eq!(p.session.inspection_view_revision(), 0);
    p.session
        .commit_required_callback_phase(phase.finish())
        .unwrap();
    // A callback commit can publish a new epoch. Use an actual new displayed
    // frame; never rewrite the old packet's collection receipt to permit retry.
    p.worker_pointer_presentation
        .set_view(
            &mut p.session,
            &mut p.browser_pointer_binding,
            &mut p.next_native_event_sequence,
            PointerPresentationView {
                revision: 2,
                width: 800.0,
                height: 400.0,
            },
        )
        .unwrap();
    let fresh = receipt(&delta(&mut p), 2);
    assert!(acknowledge(&mut p, fresh));
    let request = serde_json::json!({"view_revision":2,"viewport_width":800,"viewport_height":400,
        "surface_x":500,"surface_y":200,"delta_pixels":zoom_in(),"presentation":fresh});
    assert_eq!(
        p.scroll_inspection_view_json(&request.to_string()),
        Ok(Some(true))
    );
}
