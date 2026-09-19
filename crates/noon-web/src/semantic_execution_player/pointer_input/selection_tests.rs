//! Real shared-session click admission and retained transport, without a DOM mock.
use super::*;
use crate::{
    InstalledRetainedExecutionMirror, RetainedFamilyExecutionDeltaEnvelope,
    RetainedTransportApplyOutcome,
};

fn player() -> SemanticExecutionPlayer {
    let mut scene = noon::Scene::new();
    let mut circle = scene.circle(1.0).unwrap();
    circle.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut player =
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 2.0, 7).unwrap();
    player.pause();
    player.set_pointer_fill_selection(Some(4.0)).unwrap();
    player.initial_delta_json().unwrap();
    player
}

fn input(player: &mut SemanticExecutionPlayer, kind: &str, x: f32) {
    player
        .submit_browser_pointer_input_json(
            &serde_json::json!({
                "kind": kind, "source_id": 1, "pointer_id": 7, "view_revision": 0,
                "surface_x": x, "surface_y": 200.0,
                "viewport_width": 800.0, "viewport_height": 400.0,
                "button": if kind == "move" { None } else { Some(0_u8) },
            })
            .to_string(),
        )
        .unwrap();
}

fn click(player: &mut SemanticExecutionPlayer, x: f32) {
    input(player, "press", x);
    input(player, "release", x);
}

fn drain(player: &mut SemanticExecutionPlayer) -> RetainedFamilyExecutionDeltaEnvelope {
    serde_json::from_str(&player.drain_delta_json().unwrap().unwrap()).unwrap()
}

#[test]
fn paused_click_and_clear_use_transport_sequence_without_authored_dirtiness() {
    let mut p = player();
    let frame = p.session.frame().clone();
    let context = p.session.publication_context();
    let bundle = p.resource_bundle_bytes();
    input(&mut p, "press", 400.0);
    assert!(p.drain_delta_json().unwrap().is_none());
    input(&mut p, "release", 400.0);
    let selected = drain(&mut p);
    assert!(!selected.retained.snapshot);
    assert_eq!(selected.retained.sequence, 1);
    assert!(selected.retained.objects.is_empty());
    assert!(selected.family_states.is_empty() && selected.family_plans.is_empty());
    assert!(selected.resource_additions.is_none());
    assert!(selected.transient_presentations.is_empty());
    assert!(selected.selection_overlay.is_some());
    assert_eq!(p.session.frame(), &frame);
    assert_eq!(p.session.publication_context(), context);
    assert_eq!(p.resource_bundle_bytes(), bundle);
    assert!(p.session.take_renderer_publication().changes().is_empty());
    assert!(p.drain_delta_json().unwrap().is_none());
    click(&mut p, 5.0);
    let clear = drain(&mut p);
    assert_eq!(clear.retained.sequence, 2);
    assert!(clear.retained.objects.is_empty());
    assert!(clear.selection_overlay.is_none());
    assert!(!serde_json::to_value(&clear)
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("selection_overlay"));
    assert_eq!(p.session.frame(), &frame);
    assert_eq!(p.session.publication_context(), context);
    assert_eq!(p.execution_wake(50_000.0).unwrap().cadence(), "idle");
}

#[test]
fn same_selection_out_and_back_and_rejected_configuration_emit_no_delta() {
    let mut p = player();
    click(&mut p, 400.0);
    let selected = drain(&mut p).selection_overlay;
    click(&mut p, 400.0);
    assert!(p.drain_delta_json().unwrap().is_none());
    input(&mut p, "press", 5.0);
    input(&mut p, "move", 45.0);
    input(&mut p, "move", 5.0);
    input(&mut p, "release", 5.0);
    assert!(p.drain_delta_json().unwrap().is_none());
    for bad in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(p.set_pointer_fill_selection(Some(bad)).is_err());
        assert!(p.drain_delta_json().unwrap().is_none());
    }
    assert_eq!(p.last_sent_selection_overlay, selected);
    p.set_pointer_fill_selection(None).unwrap();
    assert!(drain(&mut p).selection_overlay.is_none());
    assert!(p.drain_delta_json().unwrap().is_none());
}

#[test]
fn rebind_snapshot_projects_current_session_but_seek_clears_selection() {
    let mut p = player();
    click(&mut p, 400.0);
    let selected = drain(&mut p).selection_overlay;
    p.rebind_transport(2.0, 8).unwrap();
    let snapshot: RetainedFamilyExecutionDeltaEnvelope =
        serde_json::from_str(&p.initial_delta_json().unwrap()).unwrap();
    assert!(snapshot.retained.snapshot);
    assert_eq!(snapshot.retained.session, 8);
    assert_eq!(snapshot.retained.sequence, 0);
    assert_eq!(snapshot.selection_overlay, selected);
    let clear: RetainedFamilyExecutionDeltaEnvelope =
        serde_json::from_str(&p.seek_delta_json(0.0).unwrap().unwrap()).unwrap();
    assert!(clear.selection_overlay.is_none());
    assert!(p.session.selected_pointer_target().is_none());
}

#[test]
fn invalid_overlay_does_not_acknowledge_or_mutate_the_retained_mirror() {
    let mut p = player();
    // Begin a new transport so the mirror's first snapshot is sequence zero.
    p.rebind_transport(2.0, 8).unwrap();
    let mut mirror =
        InstalledRetainedExecutionMirror::from_bundle_bytes(&p.resource_bundle_bytes()).unwrap();
    let snapshot: RetainedFamilyExecutionDeltaEnvelope =
        serde_json::from_str(&p.initial_delta_json().unwrap()).unwrap();
    mirror.apply_family(snapshot).unwrap();
    let before = mirror.frame().unwrap().clone();
    click(&mut p, 400.0);
    let valid = drain(&mut p);
    let mut invalid = valid.clone();
    invalid
        .selection_overlay
        .as_mut()
        .unwrap()
        .transform
        .scale
        .x = 0.0;
    assert!(mirror.apply_family(invalid).is_err());
    assert_eq!(mirror.frame().unwrap(), &before);
    // The same sequence must still be admissible, not dropped as acknowledged.
    let (outcome, _) = mirror.apply_family(valid).unwrap();
    assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
    assert_eq!(mirror.frame().unwrap(), &before);
}
