use super::*;
use noon::{ImageMobjectOptions, PrefixError, Scene};
use noon_runtime::ReplayLimits;

fn pending_capacity(player: &PreparedExecutionPlayer) -> Option<usize> {
    player
        .source
        .as_ref()?
        .pending
        .as_ref()
        .map(PrefixBatch::required_object_capacity)
}

fn navigation(rows: &[(&str, f64)]) -> noon::SceneSectionNavigation {
    let mut scene = Scene::new();
    for (name, time) in rows {
        scene
            .next_section_at(*name, noon::SectionType::Normal, *time)
            .unwrap();
    }
    scene.section_navigation()
}

#[test]
fn source_execution_requirement_json_maps_every_engine_reason() {
    use noon::SourceExecutionRequirement as Requirement;
    use noon_runtime::ReplayError;

    let cases = [
        (
            Requirement::HostEffects,
            serde_json::json!({"code":"host_effects"}),
        ),
        (
            Requirement::HostCallbacks,
            serde_json::json!({"code":"host_callbacks"}),
        ),
        (
            Requirement::EffectiveDisplay,
            serde_json::json!({"code":"effective_display"}),
        ),
        (
            Requirement::NativeInput,
            serde_json::json!({"code":"native_input"}),
        ),
        (Requirement::Signals, serde_json::json!({"code":"signals"})),
        (
            Requirement::ReplayUnavailable(ReplayError::UnsupportedDomain),
            serde_json::json!({
                "code":"replay_unavailable",
                "detail": ReplayError::UnsupportedDomain.to_string()
            }),
        ),
        (
            Requirement::ExternalResources,
            serde_json::json!({"code":"external_resources"}),
        ),
    ];
    assert_eq!(source_execution_requirement_json(None), "null");
    for (requirement, expected) in cases {
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&source_execution_requirement_json(Some(
                requirement
            )))
            .unwrap(),
            expected
        );
    }
}

#[test]
fn prepared_player_exposes_its_retained_source_requirement() {
    let mut player = staged_player(93, false);
    assert_eq!(player.source_execution_requirement_json(), "null");
    player.source.as_mut().unwrap().required =
        Some(noon::SourceExecutionRequirement::HostCallbacks);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&player.source_execution_requirement_json())
            .unwrap(),
        serde_json::json!({"code":"host_callbacks"})
    );
}

#[test]
fn retention_metrics_distinguish_pending_extraction_from_adopted_charges() {
    let mut player = staged_player(934, false);
    let read = |player: &PreparedExecutionPlayer| {
        serde_json::from_str::<serde_json::Value>(&player.prepared_retention_metrics_json())
            .unwrap()
    };
    let before = read(&player);
    assert_eq!(before["pending"]["batches"], 1);
    assert!(before["pending"]["resourceBytes"].as_u64().unwrap() > 0);
    assert_eq!(
        before,
        read(&player),
        "observation cannot adopt or drain history"
    );
    player.reserve_source_prefix_capacity().unwrap();
    player.deliver_source_prefix().unwrap();
    let after = read(&player);
    assert_eq!(after["pending"]["batches"], 0);
    assert_eq!(after["pending"]["resourceBytes"], 0);
    assert_eq!(after["extracted"], before["extracted"]);
    assert_eq!(
        after["reader"]["resourceBytes"],
        before["extracted"]["resourceBytes"]
    );
    assert_eq!(
        after["reader"]["historyPayloadBytes"],
        before["extracted"]["historyPayloadBytes"]
    );
}

#[test]
fn staged_cues_follow_only_their_adopted_prefix_and_failed_adoption_keeps_old_cues() {
    let mut player = staged_player(931, false);
    player.set_section_navigation(navigation(&[("Start", 0.0)]));
    player.source.as_mut().unwrap().staged_navigation = Some(navigation(&[
        ("Start", 0.0),
        ("Interior", 1.5),
        ("Open", 2.0),
    ]));
    let mut foreign = staged_player(932, false);
    player.source.as_mut().unwrap().pending = foreign.source.as_mut().unwrap().pending.take();

    assert!(player.deliver_source_prefix().is_err());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&player.sections_json()).unwrap(),
        serde_json::json!([{"index":0,"name":"Start","time":0.0}]),
        "a rejected batch cannot publish its staged cue snapshot",
    );

    let mut player = staged_player(933, false);
    player.set_section_navigation(navigation(&[("Start", 0.0)]));
    player.source.as_mut().unwrap().staged_navigation = Some(navigation(&[
        ("Start", 0.0),
        ("Interior", 1.5),
        ("Open", 2.0),
    ]));
    player.reserve_source_prefix_capacity().unwrap();
    assert!(player.deliver_source_prefix().unwrap());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&player.sections_json()).unwrap(),
        serde_json::json!([
            {"index":0,"name":"Start","time":0.0},
            {"index":1,"name":"Interior","time":1.5}
        ]),
        "matching append adopts interior cues but keeps the exclusive frontier hidden",
    );
}

#[test]
fn zero_duration_source_metadata_waits_for_a_matching_batch() {
    let mut player = staged_player(934, false);
    player.set_section_navigation(navigation(&[("Start", 0.0)]));
    player.source.as_mut().unwrap().pending = None;
    player.source.as_mut().unwrap().staged_navigation =
        Some(navigation(&[("Start", 0.0), ("Deferred", 1.0)]));

    assert!(!player.deliver_source_prefix().unwrap());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&player.sections_json()).unwrap(),
        serde_json::json!([{"index":0,"name":"Start","time":0.0}]),
        "zero-duration metadata is not selectable before history adopts its time",
    );
}

#[test]
fn terminal_source_delivery_adopts_its_endpoint_cue_only_after_append() {
    let mut player = staged_player(935, true);
    player.set_section_navigation(navigation(&[("Start", 0.0)]));
    player.source.as_mut().unwrap().staged_navigation =
        Some(navigation(&[("Start", 0.0), ("Terminal", 2.0)]));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&player.sections_json()).unwrap(),
        serde_json::json!([{"index":0,"name":"Start","time":0.0}]),
    );

    player.reserve_source_prefix_capacity().unwrap();
    assert!(player.deliver_source_prefix().unwrap());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&player.sections_json()).unwrap(),
        serde_json::json!([
            {"index":0,"name":"Start","time":0.0},
            {"index":1,"name":"Terminal","time":2.0}
        ]),
    );
    assert!(
        player.source.is_none(),
        "terminal adoption closes the source delivery cursor"
    );
}

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
        staged_navigation: None,
        required: None,
        finished: false,
    });
    player
}

#[test]
fn prepared_worker_source_reservation_rejects_foreign_pending_before_growth() {
    for terminal in [false, true] {
        let mut player = staged_player(91, terminal);
        let mut foreign = staged_player(92, terminal);
        let capacity = player.reserve_object_capacity(0).unwrap();
        let required = pending_capacity(&player).unwrap();
        assert!(required > capacity);
        let frame = player.playback.reader.frame().clone();
        let publication = player.playback.reader.publication_context();
        let history = player.playback.reader.replay_stats();
        let clock = player.clock;
        // Private fault injection models stale source/reader pairing. Public
        // callers cannot forge batches; admission must still precede allocation.
        std::mem::swap(&mut player.source, &mut foreign.source);
        for _ in 0..2 {
            assert_eq!(
                player.reserve_source_prefix_capacity(),
                Err(PrefixError::UnexpectedBatch.to_string())
            );
            assert_eq!(player.reserve_object_capacity(0).unwrap(), capacity);
            assert_eq!(pending_capacity(&player), Some(required));
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
        assert!(player.reserve_source_prefix_capacity().unwrap());
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
        let required = pending_capacity(&player).unwrap();
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
        assert_eq!(player.reserve_object_capacity(capacity).unwrap(), capacity);
        assert_eq!(player.deliver_source_prefix(), Err(expected));
        assert_eq!(pending_capacity(&player), Some(required));
        assert_eq!(player.prepared_end_time(), None);
        assert_eq!(player.playback.reader.replay_stats(), history);
        assert!(player.reserve_source_prefix_capacity().unwrap());
        assert_eq!(pending_capacity(&player), Some(required));
        assert_eq!(player.playback.reader.publication_context(), publication);
        assert!(player.deliver_source_prefix().unwrap());
        assert_eq!(pending_capacity(&player), None);
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
fn prepared_worker_source_failed_reservation_and_receiver_reset_keep_pending_terminal_retryable() {
    let mut player = staged_player(94, true);
    let capacity = player.reserve_object_capacity(0).unwrap();
    let required = pending_capacity(&player).unwrap();
    let frame = player.playback.reader.frame().clone();
    let publication = player.playback.reader.publication_context();
    assert!(player.reserve_object_capacity(usize::MAX).is_err());
    assert_eq!(player.reserve_object_capacity(0).unwrap(), capacity);
    assert_eq!(pending_capacity(&player), Some(required));
    assert_eq!(player.prepared_end_time(), None);
    assert_eq!(player.playback.reader.frame(), &frame);
    assert_eq!(player.playback.reader.publication_context(), publication);
    let identity = player.playback.reader.runtime_identity();
    player.reset_receiver(95).unwrap();
    player.initial_delta_json().unwrap();
    assert_eq!(pending_capacity(&player), Some(required));
    assert_eq!(player.playback.reader.frame(), &frame);
    assert_eq!(player.playback.reader.publication_context(), publication);
    assert_eq!(player.playback.reader.runtime_identity(), identity);
    assert!(player.reserve_source_prefix_capacity().unwrap());
    assert!(player.deliver_source_prefix().unwrap());
    assert_eq!(player.prepared_end_time(), Some(2.0));
    assert!(player.source.is_none());
}

#[test]
fn prepared_worker_source_reservation_without_pending_output_does_not_allocate() {
    let mut player = staged_player(95, false);
    let capacity = player.reserve_object_capacity(0).unwrap();
    player.source.as_mut().unwrap().pending = None;
    assert!(!player.deliver_source_prefix().unwrap());
    assert!(!player.reserve_source_prefix_capacity().unwrap());
    assert_eq!(player.reserve_object_capacity(0).unwrap(), capacity);
    assert_eq!(player.prepared_frontier(), 1.0);
    assert_eq!(player.prepared_end_time(), None);
    player.stop_source_preparation();
    assert!(!player.reserve_source_prefix_capacity().unwrap());
    assert_eq!(player.reserve_object_capacity(0).unwrap(), capacity);
    assert!(player.drain_delta_json().unwrap().is_none());
}
