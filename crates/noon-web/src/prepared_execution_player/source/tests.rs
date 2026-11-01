use super::*;
use noon::{ImageMobjectOptions, Scene};
use noon_runtime::ReplayLimits;

// Stage a real compiler-produced creation suffix, then release the source scene.
// Reservation/retry can only use this retained batch; no source can be rerun.
fn staged_player(session: u32, terminal: bool) -> PreparedExecutionPlayer {
    let mut scene = Scene::new();
    let target = scene.circle(0.65).unwrap();
    scene.add(&target).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 128,
            payloads: 2048,
        })
        .unwrap();
    let segment = source.wait_segment(1.0).unwrap();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(segment).unwrap();
    reader.seek(0.5).unwrap();
    let image = scene
        .live(&mut source)
        .create_image(ImageMobjectOptions::rgba8(2, 2, [230, 40, 10, 255].repeat(4)).unwrap())
        .unwrap();
    scene.live(&mut source).add(&image).unwrap();
    let segment = source.wait_segment(1.0).unwrap();
    source.advance_to(2.0).unwrap();
    let batch = if terminal {
        source.seal_replay().unwrap();
        source.finish_prepared_prefix(segment, &mut publisher)
    } else {
        source.publish_prepared_prefix(segment, &mut publisher)
    }
    .unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, session).unwrap();
    player.set_prepared_playing(false);
    player
        .set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
        .unwrap();
    player.initial_delta_json().unwrap();
    player.source = Some(SourceDelivery {
        publisher,
        pending: Some(batch),
    });
    player
}

#[test]
fn prepared_worker_source_reservation_rejects_foreign_pending_before_growth() {
    for terminal in [false, true] {
        let mut player = staged_player(91, terminal);
        let mut foreign = staged_player(92, terminal);
        let capacity = player.reserve_object_capacity(0).unwrap();
        let required = player.pending_source_object_capacity().unwrap();
        assert!(required > capacity);
        let frame = player.playback.reader.frame().clone();
        let publication = player.playback.reader.publication_context();
        let history = player.playback.reader.replay_stats();
        let clock = player.clock;
        // Private fault injection models stale source/reader pairing. Public
        // callers cannot forge batches; admission must still precede allocation.
        std::mem::swap(&mut player.source, &mut foreign.source);
        for total in [0, capacity, required, required + 8] {
            assert_eq!(
                player.reserve_prefix_object_capacity(total),
                Err(PrefixError::UnexpectedBatch.to_string())
            );
            assert_eq!(player.reserve_object_capacity(0).unwrap(), capacity);
            assert_eq!(player.pending_source_object_capacity(), Some(required));
            assert_eq!(player.prepared_frontier(), 1.0);
            assert_eq!(player.prepared_end_time(), None);
            assert_eq!(player.playback.reader.frame(), &frame);
            assert_eq!(player.playback.reader.publication_context(), publication);
            assert_eq!(player.playback.reader.replay_stats(), history);
            assert_eq!(player.clock, clock);
            assert!(player.drain_delta_json().unwrap().is_none());
        }
        // The rightful owner can reserve and adopt the very same retained suffix.
        std::mem::swap(&mut player.source, &mut foreign.source);
        player.reserve_prefix_object_capacity(required).unwrap();
        assert!(player.deliver_source_prefix(0.0).unwrap());
        assert_eq!(player.prepared_frontier(), 2.0);
        assert_eq!(player.prepared_end_time(), terminal.then_some(2.0));
        assert_eq!(player.playback.reader.frame(), &frame);
        assert!(player.drain_delta_json().unwrap().is_none());
    }
}

#[test]
fn prepared_worker_source_reservation_allows_capacity_retry_without_consuming_batch() {
    for terminal in [false, true] {
        let mut player = staged_player(93, terminal);
        let capacity = player.reserve_object_capacity(0).unwrap();
        let required = player.pending_source_object_capacity().unwrap();
        let frame = player.playback.reader.frame().clone();
        let publication = player.playback.reader.publication_context();
        let history = player.playback.reader.replay_stats();
        let expected = PrefixError::CapacityRequired {
            object_slots: required,
        }
        .to_string();
        assert!(required > capacity);
        assert_eq!(player.deliver_source_prefix(0.0), Err(expected.clone()));
        // Reserving too little is not permanent rejection or successful delivery.
        assert_eq!(
            player.reserve_prefix_object_capacity(capacity).unwrap(),
            capacity
        );
        assert_eq!(player.deliver_source_prefix(0.0), Err(expected));
        assert_eq!(player.pending_source_object_capacity(), Some(required));
        assert_eq!(player.prepared_end_time(), None);
        assert_eq!(player.playback.reader.replay_stats(), history);
        player.reserve_prefix_object_capacity(required).unwrap();
        assert_eq!(player.pending_source_object_capacity(), Some(required));
        assert_eq!(player.playback.reader.publication_context(), publication);
        assert!(player.deliver_source_prefix(0.0).unwrap());
        assert_eq!(player.pending_source_object_capacity(), None);
        assert_eq!(player.prepared_end_time(), terminal.then_some(2.0));
        assert_eq!(player.playback.reader.frame(), &frame);
        assert!(player.drain_delta_json().unwrap().is_none());
        player.seek_delta_json(1.5).unwrap();
        assert!(player
            .playback
            .reader
            .frame()
            .objects
            .iter()
            .any(|row| row.content.image().is_some()));
        player.seek_delta_json(0.5).unwrap();
        let rewound = player.playback.reader.frame();
        assert_eq!(rewound.time, frame.time);
        assert_eq!(
            &rewound.objects[..frame.objects.len()],
            frame.objects.as_slice()
        );
        assert_eq!(
            &rewound.presences[..frame.presences.len()],
            frame.presences.as_slice()
        );
        assert!(rewound.presences[frame.presences.len()..]
            .iter()
            .all(|&live| !live));
    }
}

#[test]
fn prepared_worker_source_failed_reservation_keeps_pending_terminal_retryable() {
    let mut player = staged_player(94, true);
    let capacity = player.reserve_object_capacity(0).unwrap();
    let required = player.pending_source_object_capacity().unwrap();
    let frame = player.playback.reader.frame().clone();
    let publication = player.playback.reader.publication_context();
    assert!(player.reserve_prefix_object_capacity(usize::MAX).is_err());
    assert_eq!(player.reserve_object_capacity(0).unwrap(), capacity);
    assert_eq!(player.pending_source_object_capacity(), Some(required));
    assert_eq!(player.prepared_end_time(), None);
    assert_eq!(player.playback.reader.frame(), &frame);
    assert_eq!(player.playback.reader.publication_context(), publication);
    player.reserve_prefix_object_capacity(required).unwrap();
    assert!(player.deliver_source_prefix(0.0).unwrap());
    assert_eq!(player.prepared_end_time(), Some(2.0));
    assert!(player.source.is_none());
}

#[test]
fn prepared_worker_source_reservation_without_pending_output_keeps_explicit_maintenance() {
    let mut player = staged_player(95, false);
    let required = player.pending_source_object_capacity().unwrap();
    player.source.as_mut().unwrap().pending = None;
    assert!(!player.deliver_source_prefix(0.0).unwrap());
    assert_eq!(
        player.reserve_prefix_object_capacity(required).unwrap(),
        required
    );
    assert_eq!(player.prepared_frontier(), 1.0);
    assert_eq!(player.prepared_end_time(), None);
    player.stop_source_preparation();
    assert_eq!(
        player.reserve_prefix_object_capacity(required).unwrap(),
        required
    );
    assert!(player.drain_delta_json().unwrap().is_none());
}

#[test]
fn source_delivery_does_not_start_cold_clock_resume_pause_or_reanchor_unexpired_playback() {
    for mode in ["cold", "paused", "unexpired"] {
        let mut player = staged_player(96, false);
        let required = player.pending_source_object_capacity().unwrap();
        player.reserve_prefix_object_capacity(required).unwrap();
        player.set_prepared_playing(true);
        if mode != "cold" {
            player.execution_wake(1000.0).unwrap();
        }
        if mode == "paused" {
            player.pause_prepared_at(1250.0).unwrap();
            player.drain_delta_json().unwrap();
        }
        let frame = player.playback.reader.frame().clone();
        let publication = player.playback.reader.publication_context();
        let clock = player.clock;
        let playing = player.prepared_playing();
        let wall = if mode == "unexpired" {
            1250.0
        } else {
            56_000.0
        };
        assert!(player.deliver_source_prefix(wall).unwrap());
        assert_eq!(
            player.clock, clock,
            "{mode}: delivery does not create a clock epoch"
        );
        assert_eq!(player.held_frontier, None);
        assert!(!player.prepared_needs_frame());
        assert_eq!(player.prepared_playing(), playing);
        assert_eq!(player.playback.reader.frame(), &frame);
        assert_eq!(player.playback.reader.publication_context(), publication);
        assert!(player.drain_delta_json().unwrap().is_none());
        let expected = if mode == "cold" { 0.5 } else { 0.75 };
        assert_eq!(player.prepared_playback_time_at(wall).unwrap(), expected);
    }
}

#[test]
fn rejected_source_delivery_does_not_mutate_an_expired_playback_clock() {
    let mut player = staged_player(97, false);
    let mut foreign = staged_player(98, false);
    player.set_prepared_playing(true);
    player.execution_wake(1000.0).unwrap();
    let required = player.pending_source_object_capacity().unwrap();
    let frame = player.playback.reader.frame().clone();
    let publication = player.playback.reader.publication_context();
    let clock = player.clock;
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(player.deliver_source_prefix(invalid).is_err());
    }
    assert_eq!(
        player.deliver_source_prefix(56_000.0),
        Err(PrefixError::CapacityRequired {
            object_slots: required,
        }
        .to_string())
    );
    assert_eq!(player.pending_source_object_capacity(), Some(required));
    player.reserve_prefix_object_capacity(required).unwrap();
    std::mem::swap(&mut player.source, &mut foreign.source);
    assert_eq!(
        player.deliver_source_prefix(56_000.0),
        Err(PrefixError::UnexpectedBatch.to_string())
    );
    assert_eq!(player.clock, clock);
    assert_eq!(player.held_frontier, None);
    assert!(!player.prepared_buffering());
    assert_eq!(player.prepared_frontier(), 1.0);
    assert_eq!(player.playback.reader.frame(), &frame);
    assert_eq!(player.playback.reader.publication_context(), publication);
    assert!(player.drain_delta_json().unwrap().is_none());
    std::mem::swap(&mut player.source, &mut foreign.source);
    assert!(player.deliver_source_prefix(56_000.0).unwrap());
    assert_eq!(player.held_frontier, Some(1.0));
    assert!(player.prepared_needs_frame());
    assert_eq!(player.prepared_frontier(), 2.0);
    assert_eq!(player.playback.reader.frame(), &frame);
}

#[test]
fn synchronous_source_stall_cannot_charge_old_wall_time_to_new_audio_availability() {
    use noon::{AudioClipOptions, AudioResource};
    use serde_json::Value;

    let mut scene = Scene::new();
    scene
        .audio(
            AudioResource::from_interleaved(8_000, 1, vec![0.125; 16_000], 64_000).unwrap(),
            AudioClipOptions {
                source_start_frame: 4_000,
                source_end_frame: Some(12_000),
                duration: Some(8.0),
                looped: true,
                gain: 0.5,
                fade_in: 2.0,
                fade_out: 2.0,
                ..Default::default()
            },
        )
        .unwrap();
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let segment = source.wait_segment(1.0).unwrap();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(segment).unwrap();
    reader.seek(0.5).unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, 99).unwrap();
    player.initial_delta_json().unwrap();
    player.set_prepared_playing(true);
    player.execution_wake(1000.0).unwrap();
    let before: Value =
        serde_json::from_str(&player.audio_anchor_json(1_001_250.0, 1250.0).unwrap()).unwrap();
    assert_eq!(before["time"], 0.75);
    let segment = source.wait_segment(2.0).unwrap();
    source.advance_to(3.0).unwrap();
    let pending = source
        .publish_prepared_prefix(segment, &mut publisher)
        .unwrap();
    player.source = Some(SourceDelivery {
        publisher,
        pending: Some(pending),
    });
    let frame = player.playback.reader.frame().clone();
    let publication = player.playback.reader.publication_context();

    // No tick or timer can run during synchronous preparation. The actual host
    // timestamp at delivery observes the OLD one-second availability before
    // adopting a three-second prefix. It must not make that prefix already past.
    assert!(!player.prepared_buffering());
    assert!(player.deliver_source_prefix(56_000.0).unwrap());
    assert_eq!(player.prepared_frontier(), 3.0);
    assert_eq!(player.playback.reader.frame(), &frame);
    assert_eq!(player.playback.reader.publication_context(), publication);
    assert!(player.drain_delta_json().unwrap().is_none());
    assert!(player.prepared_needs_frame());
    let held: Value =
        serde_json::from_str(&player.audio_anchor_json(1_056_000.0, 56_000.0).unwrap()).unwrap();
    assert_eq!(held["time"], 1.0);
    assert_eq!(held["playing"], false);

    // Another adopted suffix while the held picture is pending cannot replace
    // that frozen observation with its newer frontier or accrue further time.
    let segment = source.wait_segment(1.0).unwrap();
    source.advance_to(4.0).unwrap();
    let delivery = player.source.as_mut().unwrap();
    delivery.pending = Some(
        source
            .publish_prepared_prefix(segment, &mut delivery.publisher)
            .unwrap(),
    );
    assert!(player.deliver_source_prefix(90_000.0).unwrap());
    assert_eq!(player.prepared_frontier(), 4.0);
    assert_eq!(player.prepared_playback_time_at(90_000.0).unwrap(), 1.0);
    assert_eq!(player.playback.reader.frame(), &frame);
    player.tick_delta_json(90_000.0).unwrap();
    assert_eq!(player.prepared_time(), 1.0);
    assert!(!player.prepared_needs_frame());
    assert!(player.clock.scene_time_at(91_000.0).is_none());
    // Only the subsequent picture-qualified wake admits a new real-time epoch.
    player.execution_wake(91_000.0).unwrap();
    let resumed: Value =
        serde_json::from_str(&player.audio_anchor_json(1_091_250.0, 91_250.0).unwrap()).unwrap();
    assert_eq!(resumed["time"], 1.25);
    assert_eq!(resumed["playing"], true);
    assert_eq!(resumed["availableUntil"], 4.0);
    assert_eq!(
        player.prepared_time(),
        1.0,
        "static observation does not evaluate another frame"
    );
}
