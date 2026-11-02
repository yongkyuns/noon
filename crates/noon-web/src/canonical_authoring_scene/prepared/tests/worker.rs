use super::*;
use crate::PreparedExecutionPlayer;

fn start() -> (CanonicalAuthoringScene, Mobject, PreparedExecutionPlayer) {
    let (mut context, target) = fixture();
    context.begin_ordinary_wait(1.0).unwrap();
    context.prepare_live_segment().unwrap();
    let mut player = PreparedExecutionPlayer::from_completed_source(&mut context, 901).unwrap();
    player.set_prepared_playing(false);
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
    assert!(player.reserve_source_prefix_capacity().unwrap());
    assert!(player.drain_delta_json().unwrap().is_none());
    assert!(player.deliver_source_prefix().unwrap());
    assert_eq!(state(&player)["frontier"], 2.0);
    assert_eq!(state(&player)["time"], 0.5);
    assert!(!player.reserve_source_prefix_capacity().unwrap());
    assert!(player.drain_delta_json().unwrap().is_none());
    assert!(!player.deliver_source_prefix().unwrap());
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
    assert!(player.deliver_source_prefix().unwrap());
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
    assert!(player.deliver_source_prefix().unwrap());
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
    let prepared = context.active_live_player().unwrap().debug_frame_json();
    let before = state(&player);
    assert!(player
        .deliver_source_prefix()
        .unwrap_err()
        .contains("CapacityRequired"));
    assert_eq!(state(&player), before);
    assert!(player
        .prepare_interpreter_segment(&mut context)
        .unwrap_err()
        .contains("pending prefix"));
    assert!(player.finish_interpreter_source(&mut context).is_err());
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        prepared
    );
    assert!(player.reserve_source_prefix_capacity().unwrap());
    assert!(player.deliver_source_prefix().unwrap());
    assert_eq!(state(&player)["frontier"], 1.5);
    assert!(player.drain_delta_json().unwrap().is_none());
    assert!(!player.reserve_source_prefix_capacity().unwrap());
}

#[test]
fn interpreter_worker_terminal_capacity_failure_is_not_false_completion() {
    let (mut context, _, mut player) = start();
    let extra = context
        .live_create_manim_geometry(noon::ManimGeometryOptions::circle(0.25).unwrap())
        .unwrap();
    context.live_add_mobject(ObjectId::new(1), &extra).unwrap();
    player.finish_interpreter_source(&mut context).unwrap();
    drop(context);
    drop(extra);
    assert!(player.deliver_source_prefix().is_err());
    assert_eq!(state(&player)["end"], serde_json::Value::Null);
    assert!(player.seek_delta_json(1.0).is_err());
    assert!(player.reserve_source_prefix_capacity().unwrap());
    assert!(player.deliver_source_prefix().unwrap());
    assert_eq!(state(&player)["end"], 1.0);
    assert_eq!(state(&player)["time"], 0.5);
    assert!(player.drain_delta_json().unwrap().is_none());
    assert!(player.deliver_source_prefix().is_err());
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
    assert!(!player.reserve_source_prefix_capacity().unwrap());
    assert!(!player.deliver_source_prefix().unwrap());
    assert_eq!(state(&player)["frontier"], 1.0);
    assert!(player.seek_delta_json(1.0).is_err());
    player.finish_interpreter_source(&mut context).unwrap();
    assert!(player.deliver_source_prefix().unwrap());
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
    assert!(player.reserve_source_prefix_capacity().unwrap());
    player.stop_source_preparation();
    player.stop_source_preparation();
    assert!(!player.reserve_source_prefix_capacity().unwrap());
    assert!(player.deliver_source_prefix().is_err());
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
    // The actual source receipt is already complete; it cannot be driven twice.
    assert!(player.prepare_interpreter_segment(&mut context).is_err());
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
fn interpreter_worker_late_source_handoff_waits_for_demand_and_keeps_original_player() {
    let (mut context, target) = fixture();
    let path = context
        .scene
        .path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(2.0, 0.0)),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
    context.bind_mobject(ObjectId::new(1), &path).unwrap();
    context.begin_ordinary_wait(1.0).unwrap();
    context.prepare_live_segment().unwrap();
    let mut player = PreparedExecutionPlayer::from_completed_source(&mut context, 901).unwrap();
    player.seek_delta_json(0.5).unwrap();
    context
        .begin_ordinary_mixed_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[
                crate::canonical_authoring_scene::OrdinaryCompositionChild::MoveAlongPath {
                    target,
                    path,
                    options: AnimationOptions::new().run_time(1.0),
                },
            ],
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .unwrap();
    let before = context.active_live_player().unwrap().debug_frame_json();
    let identity = context.active_live_player().unwrap().ownership_identity();
    assert!(!player.prepare_interpreter_segment(&mut context).unwrap());
    assert!(!player.source_execution_ready());
    assert!(player.begin_source_execution(&mut context).is_err());
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        before
    );
    assert!(context
        .active_live_player()
        .unwrap()
        .has_pending_live_segment());
    assert_eq!(state(&player)["frontier"], 1.0);
    assert!(player.seek_delta_json(0.75).is_ok());
    player.tick_delta_json(0.0).unwrap();
    player.tick_delta_json(2000.0).unwrap();
    assert!(player.source_execution_ready());
    // A pause while buffering preserves play intent at the ownership boundary.
    player.set_prepared_playing(false);
    let (mut foreign, _, _) = start();
    assert!(player.begin_source_execution(&mut foreign).is_err());
    assert!(!player.begin_source_execution(&mut context).unwrap());
    assert!(player.begin_source_execution(&mut context).is_err());
    assert_eq!(
        context.active_live_player().unwrap().ownership_identity(),
        identity
    );
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        before
    );
    let original = context.take_execution_player(2.0, 0).unwrap();
    assert_eq!(original.ownership_identity(), identity);
    assert!(original.has_pending_live_segment());
    assert_eq!(original.time(), 1.0);
    assert!(!original.is_playing());
}

#[test]
fn interpreter_worker_raw_reader_has_no_implicit_source_authority() {
    let (mut context, _, _) = start();
    let (_, reader) = context.create_prepared_prefix().unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, 904);
    context.begin_ordinary_wait(0.25).unwrap();
    assert!(player.prepare_interpreter_segment(&mut context).is_err());
    assert!(context
        .active_live_player()
        .unwrap()
        .has_pending_live_segment());
    assert!(player.finish_interpreter_source(&mut context).is_err());
}

#[test]
fn interpreter_worker_retains_single_glow_edits_between_prefixes() {
    use noon::effects::{EffectUpdate, Pixels};
    use noon::Color;
    let (mut context, target, mut player) = start();
    for color in [Color::RED, Color::BLUE] {
        context
            .active_live_player()
            .unwrap()
            .live_set_glow(
                &target,
                EffectUpdate::default()
                    .color(color)
                    .radius(Pixels(3.0))
                    .intensity(0.6),
            )
            .unwrap();
        context.begin_ordinary_wait(0.5).unwrap();
        player.prepare_interpreter_segment(&mut context).unwrap();
        assert!(player.deliver_source_prefix().unwrap());
    }
    assert_eq!(state(&player)["frontier"], 2.0);
}

#[test]
fn interpreter_worker_retains_geometry_morph_after_prefix_bootstrap() {
    let (mut context, target) = fixture();
    let endpoint = context.scene.rectangle(1.4, 0.3).unwrap();
    let animation = context
        .declare_live_transform_to(
            &target,
            &endpoint,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    context.begin_ordinary_wait(1.0).unwrap();
    context.prepare_live_segment().unwrap();
    let mut player = PreparedExecutionPlayer::from_completed_source(&mut context, 905).unwrap();
    player.set_prepared_playing(false);
    player.seek_delta_json(0.5).unwrap();
    context
        .active_live_player()
        .unwrap()
        .live_play_animation(&animation)
        .unwrap();
    player.prepare_interpreter_segment(&mut context).unwrap();
    assert!(player.deliver_source_prefix().unwrap());
    assert_eq!(state(&player)["frontier"], 2.0);
}
