//! Exercise committed shared clicks through the actual worker receipt ABI.
use super::*;
use crate::worker_pointer_presentation::WorkerPointerReceipt;
use crate::RetainedFamilyExecutionDeltaEnvelope;
use noon::{PlaybackControl, Scene, SectionType};

fn player(action: PlaybackControl, audio: bool, canonical: bool) -> SemanticExecutionPlayer {
    let mut scene = Scene::new();
    let mut button = scene.rectangle(2.0, 1.0).unwrap();
    button.set_fill(0.2, 0.4, 0.5, 1.0).unwrap();
    scene.add(&button).unwrap();
    scene.on_click_playback(&button, action).unwrap();
    for name in ["First", "Second", "Third"] {
        scene.next_section(name, SectionType::Normal).unwrap();
        scene.wait(2.0).unwrap();
    }
    if audio {
        let resource =
            noon_core::AudioResource::from_interleaved(8_000, 1, vec![0.1; 8_000], 32_000).unwrap();
        scene
            .audio(
                resource,
                noon_core::AudioClipOptions {
                    looped: true,
                    duration: Some(6.0),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let navigation = scene.section_navigation();
    let session = scene.execution_session().unwrap();
    let mut p = if canonical {
        SemanticExecutionPlayer::from_live_session(
            session,
            std::rc::Rc::clone(scene.integration_store()),
            scene.root(),
            6.0,
            7,
        )
        .unwrap()
    } else {
        SemanticExecutionPlayer::from_scene_execution(
            scene.into_execution_with_session(session).unwrap(),
            6.0,
            7,
        )
        .unwrap()
    };
    p.set_section_navigation(navigation);
    p.pause();
    p.set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
        .unwrap();
    p
}

fn present(p: &mut SemanticExecutionPlayer, count: u64) -> WorkerPointerReceipt {
    let delta = if p.snapshot_sent {
        p.drain_delta_json().unwrap().unwrap()
    } else {
        p.initial_delta_json().unwrap()
    };
    let delta: RetainedFamilyExecutionDeltaEnvelope = serde_json::from_str(&delta).unwrap();
    let receipt = WorkerPointerReceipt {
        session: delta.retained.session,
        sequence: delta.retained.sequence,
        presentation: count,
        view_revision: 1,
    };
    assert!(p
        .note_pointer_presentation_json(&serde_json::to_string(&receipt).unwrap())
        .unwrap());
    receipt
}

fn input(
    p: &mut SemanticExecutionPlayer,
    receipt: Option<WorkerPointerReceipt>,
    kind: &str,
    source: u64,
    x: f32,
) -> bool {
    let positional = matches!(kind, "press" | "release" | "move");
    p.submit_browser_pointer_input_json(
        &serde_json::json!({
            "input": {
                "kind": kind, "source_id": source, "pointer_id": 7, "view_revision": 1,
                "surface_x": positional.then_some(x), "surface_y": positional.then_some(200.0),
                "viewport_width": positional.then_some(800.0),
                "viewport_height": positional.then_some(400.0),
                "button": matches!(kind, "press" | "release").then_some(0),
            },
            "presentation": receipt,
        })
        .to_string(),
    )
    .unwrap()
}

fn click(p: &mut SemanticExecutionPlayer, receipt: WorkerPointerReceipt, source: u64) -> u64 {
    assert!(input(p, Some(receipt), "press", source, 400.0));
    assert!(input(p, Some(receipt), "release", source, 400.0));
    let request: serde_json::Value =
        serde_json::from_str(&p.pending_scene_control_json().unwrap()).unwrap();
    request["sequence"].as_str().unwrap().parse().unwrap()
}

#[test]
fn every_action_is_an_exactly_once_receipt_without_changing_authored_state() {
    for canonical in [false, true] {
        for (action, name) in [
            (PlaybackControl::TogglePlayback, "toggle_playback"),
            (PlaybackControl::Restart, "restart"),
            (PlaybackControl::PreviousSection, "previous_section"),
            (PlaybackControl::NextSection, "next_section"),
            (PlaybackControl::ToggleAudioMuted, "toggle_audio_muted"),
        ] {
            let mut p = player(action, true, canonical);
            let receipt = present(&mut p, 1);
            let before = p.session.publication_context();
            let frame = p.session.frame().clone();
            let resources = p.audio_resource_bundle_bytes().unwrap();
            let sequence = click(&mut p, receipt, 1);
            assert_eq!(p.session.publication_context(), before);
            assert_eq!(p.session.frame(), &frame);
            assert_eq!(p.audio_resource_bundle_bytes().unwrap(), resources);
            assert!(!p.is_playing(), "input commits before host dispatch");
            assert_eq!(
                p.take_scene_control(sequence).unwrap().as_deref(),
                Some(name)
            );
            assert_eq!(p.take_scene_control(sequence).unwrap(), None);
            assert!(p.pending_scene_control_json().is_none());
            assert!(p.session.take_renderer_publication().changes().is_empty());
        }
    }
}

#[test]
fn newer_control_survives_old_completion_and_ordinary_hover() {
    let mut p = player(PlaybackControl::TogglePlayback, true, true);
    let receipt = present(&mut p, 1);
    let old = click(&mut p, receipt, 1);
    let pending = p.pending_scene_control_json();
    p.set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
        .unwrap();
    assert_eq!(p.pending_scene_control_json(), pending);
    assert!(input(&mut p, Some(receipt), "move", 1, 405.0));
    let current = click(&mut p, receipt, 1);
    assert!(current > old);
    assert_eq!(p.take_scene_control(old).unwrap(), None);
    assert!(input(&mut p, Some(receipt), "move", 1, 410.0));
    assert_eq!(
        p.take_scene_control(current).unwrap().as_deref(),
        Some("toggle_playback")
    );
}

#[test]
fn changed_publication_cannot_resume_from_a_delayed_unlock() {
    let mut p = player(PlaybackControl::TogglePlayback, true, false);
    let receipt = present(&mut p, 1);
    let sequence = click(&mut p, receipt, 1);
    p.session.advance_to(0.25).unwrap();
    assert_eq!(p.take_scene_control(sequence).unwrap(), None);
    assert!(!p.is_playing());
}

#[test]
fn transport_and_view_cancellation_retire_delayed_controls() {
    type Cancel = fn(&mut SemanticExecutionPlayer, WorkerPointerReceipt);
    let cases: &[Cancel] = &[
        |p, _| p.pause(),
        |p, _| p.resume(),
        |p, _| {
            p.seek_delta_json(1.0).unwrap();
        },
        |p, _| {
            p.set_loop_duration(7.0).unwrap();
        },
        |p, _| {
            p.rebind_transport(6.0, 8).unwrap();
        },
        |p, _| p.cancel_scene_control(),
        |p, _| {
            p.set_browser_pointer_view_json(r#"{"revision":2,"width":800,"height":400}"#)
                .unwrap();
        },
        |p, receipt| {
            assert!(p
                .invalidate_pointer_presentation_json(&serde_json::to_string(&receipt).unwrap())
                .unwrap());
        },
        |p, receipt| {
            assert!(input(p, Some(receipt), "cancel", 1, 400.0));
        },
    ];
    for cancel in cases {
        let mut p = player(PlaybackControl::TogglePlayback, true, false);
        let receipt = present(&mut p, 1);
        let sequence = click(&mut p, receipt, 1);
        cancel(&mut p, receipt);
        assert_eq!(p.take_scene_control(sequence).unwrap(), None);
    }
}

#[test]
fn unpresented_seek_retired_dragged_cancelled_and_background_input_has_no_command() {
    for mode in [
        "unpresented",
        "seek-retired",
        "drag",
        "cancel",
        "background",
    ] {
        let mut p = player(PlaybackControl::NextSection, false, true);
        let receipt = present(&mut p, 1);
        let supplied = (mode != "unpresented").then_some(receipt);
        let x = if mode == "background" { 5.0 } else { 400.0 };
        let pressed = input(&mut p, supplied, "press", 1, x);
        if mode != "unpresented" {
            assert!(pressed);
        }
        match mode {
            "seek-retired" => {
                p.session.seek(0.25).unwrap();
            }
            "drag" => {
                assert!(input(&mut p, supplied, "move", 1, 480.0));
            }
            "cancel" => {
                assert!(input(&mut p, supplied, "cancel", 1, x));
            }
            _ => {}
        }
        // Cancellation retires this collector source. The real DOM adapter
        // cannot send another release under that retired source identity.
        if mode != "cancel" {
            input(&mut p, supplied, "release", 1, x);
        }
        assert!(p.pending_scene_control_json().is_none(), "{mode}");
    }
}

#[test]
fn compatible_collected_click_dispatches_once_against_its_actual_commit_publication() {
    for canonical in [false, true] {
        let mut p = player(PlaybackControl::NextSection, true, canonical);
        let receipt = present(&mut p, 1);
        let collected = p.session.publication_context();
        p.session.advance_to(0.25).unwrap();
        assert!(input(&mut p, Some(receipt), "press", 1, 400.0));
        p.session.advance_to(0.5).unwrap();
        let unpresented = p.drain_delta_json().unwrap().unwrap();
        let unpresented: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&unpresented).unwrap();
        assert!(unpresented.retained.sequence > receipt.sequence);
        assert!(input(&mut p, Some(receipt), "release", 1, 400.0));
        let request = p.pending_scene_control.expect("committed shared click");
        assert_ne!(request.publication, collected);
        assert_eq!(request.publication, p.session.publication_context());
        assert_eq!(p.time(), 0.5);
        assert!(
            !p.is_playing(),
            "input does not dispatch or reanchor playback"
        );
        assert_eq!(
            p.take_scene_control(request.sequence).unwrap().as_deref(),
            Some("next_section")
        );
        assert_eq!(p.take_scene_control(request.sequence).unwrap(), None);
    }
}

#[test]
fn finite_controls_without_audio_share_sections_and_exact_endpoint() {
    let mut p = player(PlaybackControl::NextSection, false, true);
    assert_eq!(p.finite_playback_duration(), Some(6.0));
    assert!(!p.has_audio_wasm());
    assert_eq!(p.clock.loop_duration(), None);
    for time in [2.0, 4.0, 4.0] {
        p.seek_section_delta_json(false).unwrap();
        assert_eq!(p.time(), time);
        assert!(!p.is_playing());
    }
    p.resume();
    p.tick_callback_phase_json(1_000.0).unwrap();
    p.tick_callback_phase_json(4_500.0).unwrap();
    assert_eq!(p.time(), 6.0);
    assert!(!p.is_playing());
    assert!(p.seek_delta_json(6.01).is_err());
    p.seek_delta_json(0.0).unwrap();
    p.resume();
    assert!(p.is_playing());
    assert_eq!(p.time(), 0.0);
}

#[test]
fn scene_controls_reject_a_finite_plan_with_live_drag_ownership() {
    let mut scene = Scene::new();
    let mut button = scene.rectangle(2.0, 1.0).unwrap();
    button.set_fill(0.2, 0.4, 0.5, 1.0).unwrap();
    scene.add(&button).unwrap();
    scene
        .on_click_playback(&button, PlaybackControl::Restart)
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .set_translation_drag_targets([button.node_id()])
        .unwrap();
    assert!(session.finite_timeline_end_time().is_some());
    let error = SemanticExecutionPlayer::from_scene_execution(
        scene.into_execution_with_session(session).unwrap(),
        6.0,
        7,
    )
    .err()
    .expect("finite duration does not admit a live mutation owner");
    assert!(error.contains("finite immutable"));
    assert!(error.contains("#2263"));
}
