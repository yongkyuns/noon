use super::*;
use noon_core::{AnimationOptions, RateFunction};

#[test]
fn observed_source_pause_preserves_elapsed_wait_and_intent_across_segments() {
    let mut scene = noon::Scene::new();
    let object = scene.circle(0.4).unwrap();
    scene.add(&object).unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        2.0,
        199,
    )
    .unwrap();
    player.live_wait(2.0).unwrap();
    player.live_segment_wake(1_000.0).unwrap();
    assert_eq!(player.playback_time_at(1_500.0).unwrap(), 0.5);
    // The host commits elapsed static time before freezing the coherent frame.
    assert!(player
        .advance_forward_to_callback_phase_json(0.5)
        .unwrap()
        .is_none());
    player.pause();
    assert_eq!(
        player.live_segment_wake(50_000.0).unwrap().cadence(),
        "idle"
    );
    assert!(!player
        .live_drive_segment_from_wall_time(50_000.0)
        .unwrap()
        .reached_endpoint());
    assert_eq!(player.time(), 0.5);
    assert!(player.has_pending_live_segment());
    assert!(player.live_segment_wake(f64::NAN).is_err());

    player.resume();
    let wake = player.live_segment_wake(100_000.0).unwrap();
    assert_eq!(wake.timer_after_milliseconds(), Some(1_500.0));
    assert_eq!(player.playback_time_at(100_500.0).unwrap(), 1.0);
    assert!(!player
        .live_drive_segment_from_wall_time(100_500.0)
        .unwrap()
        .reached_endpoint());
    assert!(
        player.is_playing(),
        "a realtime sample must preserve play intent"
    );
    assert!(player
        .live_drive_segment_from_wall_time(101_500.0)
        .unwrap()
        .reached_endpoint());
    player.live_complete_segment().unwrap();
    assert!(
        player.is_playing(),
        "returning the lease does not pause the source"
    );
    player.pause();
    player.live_wait(1.0).unwrap();
    assert!(
        !player.is_playing(),
        "next source statement cannot override viewer pause"
    );
    assert_eq!(
        player.live_segment_wake(200_000.0).unwrap().cadence(),
        "idle"
    );
    player.resume();
    assert_eq!(
        player
            .live_segment_wake(300_000.0)
            .unwrap()
            .timer_after_milliseconds(),
        Some(1_000.0)
    );
}

#[test]
fn late_first_sample_of_a_new_finite_play_is_evaluated_before_endpoint() {
    let mut scene = noon::Scene::new();
    let object = scene.circle(0.4).unwrap();
    scene.add(&object).unwrap();
    let mut first_target = object.target_editor().unwrap();
    first_target.set_translation(1.0, 0.0).unwrap();
    let mut second_target = object.target_editor().unwrap();
    second_target.set_translation(2.0, 0.0).unwrap();
    let session = scene.execution_session().unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        2.0,
        198,
    )
    .unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let transform = |target| {
        noon::AnimationCompositionRequest::TransformTo(noon::TransformToRequest::new(
            &object, target, options,
        ))
    };
    player
        .live_declare_and_activate_composition(&transform(&first_target), options)
        .unwrap();
    player.live_segment_wake(1_000.0).unwrap();
    // A logical wait can observe the same continuous background work. It must
    // still catch up immediately, without acquiring an extra presentation.
    let wait = player.session.wait_segment(0.5).unwrap();
    assert!(matches!(
        player.session.segment_state(wait).timeline(),
        TimelineWakeState::Continuous
    ));
    assert_eq!(
        player.session.initial_continuous_segment_sample(wait, 9.0),
        9.0
    );
    let segment = player.live_segment().unwrap();
    assert_eq!(
        player
            .session
            .initial_continuous_segment_sample(segment, f64::INFINITY),
        f64::INFINITY
    );
    let first = player.live_drive_segment_from_wall_time(1_044.0).unwrap();
    assert!(!first.reached_endpoint());
    assert!(player.time() > 0.0 && player.time() < 1.0);
    assert!(player
        .live_drive_segment_from_wall_time(3_300.0)
        .unwrap()
        .reached_endpoint());
    player.live_complete_segment().unwrap();

    // The source still owns its original epoch. Its next finite play must
    // expose an interior state even though the wall clock is already overdue.
    player
        .live_declare_and_activate_composition(&transform(&second_target), options)
        .unwrap();
    player.live_segment_wake(3_400.0).unwrap();
    let first = player.live_drive_segment_from_wall_time(3_400.0).unwrap();
    assert!(
        !first.reached_endpoint(),
        "the first sample of a new finite play skipped its entire interval"
    );
    assert!(player.time() > 1.0 && player.time() < 2.0);
    let current = player.live_effective(&object).unwrap();
    assert!(current.transform.translation.x > 1.0 && current.transform.translation.x < 2.0);
    assert!((player.live_wake_clock.scene_time_at(3_400.0).unwrap() - 2.4).abs() < 1e-12);

    assert!(player
        .live_drive_segment_from_wall_time(3_400.0)
        .unwrap()
        .reached_endpoint());
    player.live_complete_segment().unwrap();
    assert_eq!(player.time(), 2.0);
    player.live_wait(0.5).unwrap();
    let wake = player.live_segment_wake(3_400.0).unwrap();
    assert!((wake.timer_after_milliseconds().unwrap() - 100.0).abs() < 1e-9);
}
