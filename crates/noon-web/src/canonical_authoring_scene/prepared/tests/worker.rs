use super::*;
use crate::PreparedExecutionPlayer;

fn start() -> (CanonicalAuthoringScene, Mobject, PreparedExecutionPlayer) {
    let (mut context, target) = fixture();
    context.begin_ordinary_wait(1.0).unwrap();
    context.prepare_live_segment().unwrap();
    let mut player = PreparedExecutionPlayer::from_completed_source(&mut context, 901).unwrap();
    player.set_prepared_playing(false).unwrap();
    player.seek_delta_json(0.5).unwrap();
    assert!(player.drain_delta_json().unwrap().is_none());
    (context, target, player)
}

fn state(player: &PreparedExecutionPlayer) -> serde_json::Value {
    serde_json::from_str(&player.debug_prepared_playback_json()).unwrap()
}

#[test]
fn interpreter_worker_factory_requires_completion_and_does_not_lease_source() {
    let (mut context, _) = fixture();
    context.begin_ordinary_wait(1.0).unwrap();
    let before = context.active_live_player().unwrap().debug_frame_json();
    assert!(PreparedExecutionPlayer::from_completed_source(&mut context, 901).is_err());
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        before
    );
    context.prepare_live_segment().unwrap();
    let identity = context.active_live_player().unwrap().ownership_identity();
    let player = PreparedExecutionPlayer::from_completed_source(&mut context, 901).unwrap();
    assert_eq!(state(&player)["time"], 0.0);
    assert_eq!(state(&player)["frontier"], 1.0);
    assert_eq!(
        context.active_live_player().unwrap().ownership_identity(),
        identity
    );
    assert_eq!(context.live_execution_ownership(), "active");
}

#[test]
fn interpreter_worker_completion_is_separate_from_quiet_historical_adoption() {
    let (mut context, target, mut player) = start();
    context
        .active_live_player()
        .unwrap()
        .live_set_translation(&target, 2.0, 0.0)
        .unwrap();
    context.begin_ordinary_wait(1.0).unwrap();
    player.prepare_interpreter_segment(&mut context).unwrap();
    assert_eq!(context.active_live_player().unwrap().time(), 2.0);
    assert_eq!(state(&player)["frontier"], 1.0);
    assert!(player.pending_source_object_capacity().is_some());
    assert!(player.drain_delta_json().unwrap().is_none());
    assert!(player.deliver_source_prefix(0.0).unwrap());
    assert_eq!(state(&player)["frontier"], 2.0);
    assert_eq!(state(&player)["time"], 0.5);
    assert!(player.pending_source_object_capacity().is_none());
    assert!(player.drain_delta_json().unwrap().is_none());
    assert!(!player.deliver_source_prefix(0.0).unwrap());
    assert!(player.seek_delta_json(1.5).unwrap().is_some());
}

#[test]
fn interpreter_worker_foreign_source_is_rejected_before_completion_or_sealing() {
    let (mut context, _, mut player) = start();
    let (mut foreign, _) = fixture();
    foreign.begin_ordinary_wait(2.0).unwrap();
    let before = foreign.active_live_player().unwrap().debug_frame_json();
    assert!(player.prepare_interpreter_segment(&mut foreign).is_err());
    assert!(player.finish_interpreter_source(&mut foreign).is_err());
    assert_eq!(
        foreign.active_live_player().unwrap().debug_frame_json(),
        before
    );
    assert!(foreign
        .active_live_player()
        .unwrap()
        .has_pending_live_segment());
    // Neither the foreign context nor the rightful binding was retired.
    foreign.prepare_live_segment().unwrap();
    assert!(foreign.create_prepared_prefix().is_ok());
    context.begin_ordinary_wait(0.25).unwrap();
    player.prepare_interpreter_segment(&mut context).unwrap();
    assert!(player.deliver_source_prefix(0.0).unwrap());
    assert_eq!(state(&player)["frontier"], 1.25);
}

#[test]
fn interpreter_worker_source_lease_rejection_preserves_the_rightful_binding() {
    let (mut context, _, mut player) = start();
    context.begin_ordinary_wait(0.25).unwrap();
    let lease = context.take_execution_player(1.25, 902).unwrap();
    assert!(player.prepare_interpreter_segment(&mut context).is_err());
    context.return_execution_player(lease).unwrap();
    player.prepare_interpreter_segment(&mut context).unwrap();
    assert!(player.deliver_source_prefix(0.0).unwrap());
    assert_eq!(state(&player)["frontier"], 1.25);
}

#[test]
fn interpreter_worker_capacity_failure_keeps_one_batch_without_repeating_source() {
    let (mut context, _, mut player) = start();
    let extra = context
        .live_create_manim_geometry(noon::ManimGeometryOptions::circle(0.25).unwrap())
        .unwrap();
    context.live_add_mobject(ObjectId::new(1), &extra).unwrap();
    context.begin_ordinary_wait(0.5).unwrap();
    player.prepare_interpreter_segment(&mut context).unwrap();
    let required = player.pending_source_object_capacity().unwrap();
    let prepared = context.active_live_player().unwrap().debug_frame_json();
    let before = state(&player);
    assert!(player
        .deliver_source_prefix(0.0)
        .unwrap_err()
        .contains("CapacityRequired"));
    assert_eq!(state(&player), before);
    assert_eq!(player.pending_source_object_capacity(), Some(required));
    assert!(player
        .prepare_interpreter_segment(&mut context)
        .unwrap_err()
        .contains("pending prefix"));
    assert!(player.finish_interpreter_source(&mut context).is_err());
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        prepared
    );
    player.reserve_prefix_object_capacity(required).unwrap();
    assert!(player.deliver_source_prefix(0.0).unwrap());
    assert_eq!(state(&player)["frontier"], 1.5);
    assert!(player.drain_delta_json().unwrap().is_none());
    assert!(player.pending_source_object_capacity().is_none());
}

#[test]
fn interpreter_worker_terminal_capacity_failure_is_not_false_completion() {
    let (mut context, _, mut player) = start();
    let extra = context
        .live_create_manim_geometry(noon::ManimGeometryOptions::circle(0.25).unwrap())
        .unwrap();
    context.live_add_mobject(ObjectId::new(1), &extra).unwrap();
    player.finish_interpreter_source(&mut context).unwrap();
    let required = player.pending_source_object_capacity().unwrap();
    drop(context);
    drop(extra);
    assert!(player.deliver_source_prefix(0.0).is_err());
    assert_eq!(state(&player)["end"], serde_json::Value::Null);
    assert!(player.seek_delta_json(1.0).is_err());
    player.reserve_prefix_object_capacity(required).unwrap();
    assert!(player.deliver_source_prefix(0.0).unwrap());
    assert_eq!(state(&player)["end"], 1.0);
    assert_eq!(state(&player)["time"], 0.5);
    assert!(player.drain_delta_json().unwrap().is_none());
    assert!(player.deliver_source_prefix(0.0).is_err());
    assert!(player.seek_delta_json(1.0).unwrap().is_some());
    assert!(player.seek_delta_json(0.5).unwrap().is_some());
}

#[test]
fn interpreter_worker_zero_duration_completion_defers_same_time_edits() {
    let (mut context, target, mut player) = start();
    context
        .active_live_player()
        .unwrap()
        .live_set_translation(&target, 3.0, 0.0)
        .unwrap();
    context.begin_ordinary_wait(0.0).unwrap();
    player.prepare_interpreter_segment(&mut context).unwrap();
    assert!(player.pending_source_object_capacity().is_none());
    assert!(!player.deliver_source_prefix(0.0).unwrap());
    assert_eq!(state(&player)["frontier"], 1.0);
    assert!(player.seek_delta_json(1.0).is_err());
    player.finish_interpreter_source(&mut context).unwrap();
    assert!(player.deliver_source_prefix(0.0).unwrap());
    let delta = player.seek_delta_json(1.0).unwrap().unwrap();
    let decoded: crate::RetainedFamilyExecutionDeltaEnvelope =
        serde_json::from_str(&delta).unwrap();
    assert_eq!(decoded.retained.time, 1.0);
}

#[test]
fn interpreter_worker_cancellation_releases_pending_only_and_never_seals_history() {
    let (mut context, _, mut player) = start();
    context.begin_ordinary_wait(0.25).unwrap();
    player.prepare_interpreter_segment(&mut context).unwrap();
    assert!(player.pending_source_object_capacity().is_some());
    player.stop_source_preparation();
    player.stop_source_preparation();
    assert!(player.pending_source_object_capacity().is_none());
    assert!(player.deliver_source_prefix(0.0).is_err());
    assert!(player.prepare_interpreter_segment(&mut context).is_err());
    assert_eq!(state(&player)["frontier"], 1.0);
    assert_eq!(state(&player)["end"], serde_json::Value::Null);
    assert!(player.drain_delta_json().unwrap().is_none());
    assert!(player.seek_delta_json(0.75).is_ok());
    assert_eq!(context.active_live_player().unwrap().time(), 1.25);
}

#[test]
fn interpreter_worker_failed_completion_cannot_retry_but_retains_playback() {
    let (mut context, _, mut player) = start();
    player.seek_delta_json(0.0).unwrap();
    player
        .prepare_history_export_json(&history_export_options().to_string())
        .unwrap();
    assert!(player.finite_export_active());
    // The actual source receipt is already complete; it cannot be driven twice.
    assert!(player.prepare_interpreter_segment(&mut context).is_err());
    assert!(
        !player.finite_export_active(),
        "failed source completion releases its export preparation lock"
    );
    context.begin_ordinary_wait(0.25).unwrap();
    assert!(player.prepare_interpreter_segment(&mut context).is_err());
    assert!(context
        .active_live_player()
        .unwrap()
        .has_pending_live_segment());
    assert_eq!(state(&player)["frontier"], 1.0);
    assert_eq!(state(&player)["end"], serde_json::Value::Null);
    assert!(player.seek_delta_json(0.75).is_ok());
}

#[test]
fn interpreter_worker_raw_reader_has_no_implicit_source_authority() {
    let (mut context, _, _) = start();
    let (_, reader) = context.create_prepared_prefix().unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, 904).unwrap();
    context.begin_ordinary_wait(0.25).unwrap();
    assert!(player.prepare_interpreter_segment(&mut context).is_err());
    assert!(context
        .active_live_player()
        .unwrap()
        .has_pending_live_segment());
    assert!(player.finish_interpreter_source(&mut context).is_err());
}

// Native execution of the exact WASM adapter methods used by the worker.
fn preparing_player() -> (CanonicalAuthoringScene, PreparedExecutionPlayer) {
    let mut source = CanonicalAuthoringScene::default();
    source.select_prepared_source().unwrap();
    source.begin_ordinary_wait(1.0).unwrap();
    source.prepare_live_segment().unwrap();
    let mut player = PreparedExecutionPlayer::from_completed_source(&mut source, 98).unwrap();
    player.set_prepared_playing(false).unwrap();
    player.initial_delta_json().unwrap();
    (source, player)
}

fn history_export_options() -> serde_json::Value {
    serde_json::json!({
        "frame_rate": { "numerator": 16, "denominator": 1 },
        "start_frame": 0, "stop": { "kind": "source_end" },
        "max_frames": 64, "max_transitions_per_sample": 64, "final_hold_seconds": 0.125,
    })
}

fn parse_export(json: String) -> serde_json::Value {
    serde_json::from_str(&json).unwrap()
}

#[test]
fn history_export_preparation_waits_for_terminal_adoption_at_the_exact_shared_cap() {
    let (mut source, mut player) = preparing_player();
    let mut options = history_export_options();
    options["max_frames"] = 16.into();
    options["final_hold_seconds"] = 0.0.into();
    let before = player.debug_frame_json();
    let requested = parse_export(
        player
            .prepare_history_export_json(&options.to_string())
            .unwrap(),
    );
    assert_eq!(requested["source_preparation_limit"], 1.0);
    assert!(
        player.needs_preparation(0.001).unwrap(),
        "exact cap still permits terminal continuation"
    );
    assert!(player.finite_export_active());
    assert!(player.set_prepared_playing(true).is_err());
    assert!(player
        .begin_finite_export_json(&options.to_string())
        .is_err());
    player.finish_interpreter_source(&mut source).unwrap();
    drop(source);
    assert!(
        player
            .begin_finite_export_json(&options.to_string())
            .is_err(),
        "undelivered completion is not terminal history"
    );
    player.deliver_source_prefix(0.0).unwrap();
    assert!(!player.needs_preparation(0.001).unwrap());
    assert_eq!(player.prepared_time(), 0.0);
    assert_eq!(player.debug_frame_json(), before);
    assert!(player.drain_delta_json().unwrap().is_none());
    let mut changed_options = options.clone();
    changed_options["start_frame"] = 1.into();
    assert!(player
        .begin_finite_export_json(&changed_options.to_string())
        .is_err());
    assert!(player.finite_export_active());
    player
        .begin_finite_export_json(&options.to_string())
        .unwrap();
    player.cancel_finite_export();
    assert!(!player.finite_export_active());
}

#[test]
fn history_export_preparation_rejects_overshoot_and_source_stop_releases_control_lock() {
    let (mut source, mut player) = preparing_player();
    let mut options = history_export_options();
    options["max_frames"] = 16.into();
    options["final_hold_seconds"] = 0.0.into();
    player
        .prepare_history_export_json(&options.to_string())
        .unwrap();
    source.begin_ordinary_wait(0.25).unwrap();
    player.prepare_interpreter_segment(&mut source).unwrap();
    player.deliver_source_prefix(0.0).unwrap();
    assert!(player
        .needs_preparation(0.001)
        .unwrap_err()
        .contains("max_frames"));
    assert_eq!(player.prepared_time(), 0.0);
    player.stop_source_preparation();
    assert!(!player.finite_export_active());
    player.seek_delta_json(0.5).unwrap();
    player.set_prepared_playing(true).unwrap();
    assert!(player
        .begin_finite_export_json(&options.to_string())
        .is_err());
}
