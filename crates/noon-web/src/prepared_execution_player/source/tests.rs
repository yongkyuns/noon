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
    let mut player = PreparedExecutionPlayer::from_reader(reader, session);
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
        assert!(player.deliver_source_prefix().unwrap());
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
        assert_eq!(player.deliver_source_prefix(), Err(expected.clone()));
        // Reserving too little is not permanent rejection or successful delivery.
        assert_eq!(
            player.reserve_prefix_object_capacity(capacity).unwrap(),
            capacity
        );
        assert_eq!(player.deliver_source_prefix(), Err(expected));
        assert_eq!(player.pending_source_object_capacity(), Some(required));
        assert_eq!(player.prepared_end_time(), None);
        assert_eq!(player.playback.reader.replay_stats(), history);
        player.reserve_prefix_object_capacity(required).unwrap();
        assert_eq!(player.pending_source_object_capacity(), Some(required));
        assert_eq!(player.playback.reader.publication_context(), publication);
        assert!(player.deliver_source_prefix().unwrap());
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
    assert!(player.deliver_source_prefix().unwrap());
    assert_eq!(player.prepared_end_time(), Some(2.0));
    assert!(player.source.is_none());
}

#[test]
fn prepared_worker_source_reservation_without_pending_output_keeps_explicit_maintenance() {
    let mut player = staged_player(95, false);
    let required = player.pending_source_object_capacity().unwrap();
    player.source.as_mut().unwrap().pending = None;
    assert!(!player.deliver_source_prefix().unwrap());
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
