use super::*;
use crate::worker_pointer_presentation::WorkerPointerReceipt;
use crate::{InstalledRetainedExecutionMirror, RetainedTransportApplyOutcome};
use noon::{AnimationOptions, IndicateOptions, Mobject, PrefixPublisher, Scene, SceneExecution};
use noon_runtime::ReplayLimits;

struct Fixture {
    target: Mobject,
    source: SceneExecution,
    publisher: PrefixPublisher,
    player: PreparedExecutionPlayer,
}

impl Fixture {
    fn new(session: u32) -> Self {
        let mut scene = Scene::new();
        let mut target = scene.circle(0.65).unwrap();
        target.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
        target.set_stroke_width(0.0).unwrap();
        scene.add(&target).unwrap();
        scene
            .on_click_indicate(
                &target,
                IndicateOptions::default(),
                AnimationOptions::new().run_time(0.5),
            )
            .unwrap();
        let mut source = scene.into_execution().unwrap();
        source
            .session_mut()
            .begin_replay_retention(ReplayLimits {
                revisions: 128,
                payloads: 2048,
            })
            .unwrap();
        let segment = source.session_mut().wait_segment(1.0).unwrap();
        source.session_mut().advance_to(1.0).unwrap();
        let (publisher, mut reader) = source.session().create_prepared_prefix(segment).unwrap();
        reader.seek(0.5).unwrap();
        let mut player = PreparedExecutionPlayer::from_reader(reader, session);
        player.set_prepared_playing(false);
        player
            .set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
            .unwrap();
        Self {
            target,
            source,
            publisher,
            player,
        }
    }

    fn next(&mut self, terminal: bool) -> PrefixBatch {
        self.source.live().shift(&self.target, 1.0, 0.0).unwrap();
        let segment = self.source.session_mut().wait_segment(1.0).unwrap();
        self.source
            .session_mut()
            .advance_to(segment.end_time())
            .unwrap();
        if terminal {
            self.source.session_mut().seal_replay().unwrap();
            self.source
                .session()
                .finish_prepared_prefix(segment, &mut self.publisher)
                .unwrap()
        } else {
            self.source
                .session()
                .publish_prepared_prefix(segment, &mut self.publisher)
                .unwrap()
        }
    }

    fn deliver(&mut self, batch: PrefixBatch) {
        self.player
            .reserve_object_capacity(batch.required_object_capacity())
            .unwrap();
        self.player.preflight_append(&batch).unwrap();
        self.player.append(batch).unwrap();
    }
}

fn decode(json: &str) -> RetainedFamilyExecutionDeltaEnvelope {
    serde_json::from_str(json).unwrap()
}

fn receive(mirror: &mut InstalledRetainedExecutionMirror, json: &str) {
    let (outcome, _) = mirror.apply_json(json).unwrap();
    assert_ne!(outcome, RetainedTransportApplyOutcome::DroppedStale);
}

fn playback_state(player: &PreparedExecutionPlayer) -> serde_json::Value {
    serde_json::from_str(&player.prepared_playback_state_json(0.0).unwrap()).unwrap()
}

fn static_loop_player(session: u32) -> (PreparedExecutionPlayer, PreparedPresentationIdentity) {
    let mut fixture = Fixture::new(session);
    ready(&mut fixture.player);
    let terminal = fixture.next(true);
    fixture.deliver(terminal);
    let selected = decode(&fixture.player.seek_delta_json(1.5).unwrap().unwrap());
    fixture
        .player
        .set_prepared_loop_duration(Some(2.0))
        .unwrap();
    fixture.player.set_prepared_playing(true);
    fixture.player.execution_wake(1_000.0).unwrap();
    (
        fixture.player,
        PreparedPresentationIdentity {
            session: selected.retained.session,
            sequence: selected.retained.sequence,
        },
    )
}

#[test]
fn prepared_state_proves_only_a_current_unexpired_static_loop_hold() {
    let (mut player, publication) = static_loop_player(311);

    let state: serde_json::Value =
        serde_json::from_str(&player.prepared_playback_state_json(1_250.0).unwrap()).unwrap();
    assert_eq!(state["loopCount"], 0);
    assert_eq!(
        state["heldPresentation"]["publication"]["session"],
        publication.session
    );
    assert_eq!(
        state["heldPresentation"]["publication"]["sequence"],
        publication.sequence
    );
    assert_eq!(state["heldPresentation"]["evaluatedTime"], 1.5);
    assert_eq!(state["heldPresentation"]["time"], 1.75);
    assert_eq!(state["heldPresentation"]["validUntil"], 2.0);

    let expired: serde_json::Value =
        serde_json::from_str(&player.prepared_playback_state_json(1_500.0).unwrap()).unwrap();
    assert!(
        expired.get("heldPresentation").is_none(),
        "raw clock at the loop boundary cannot reuse the pre-wrap hold"
    );

    player.set_prepared_playing(false);
    let paused: serde_json::Value =
        serde_json::from_str(&player.prepared_playback_state_json(2_100.0).unwrap()).unwrap();
    assert!(
        paused.get("heldPresentation").is_none(),
        "paused readers do not advertise a live hold"
    );
}

#[test]
fn held_presentation_rejects_stale_publication_and_dirty_interactive_state() {
    let (mut stale, _) = static_loop_player(312);
    stale.encoder = RetainedFamilyExecutionDeltaEncoder::new(313);
    let stale_state: serde_json::Value =
        serde_json::from_str(&stale.prepared_playback_state_json(1_500.0).unwrap()).unwrap();
    assert!(
        stale_state.get("heldPresentation").is_none(),
        "a publication from an older receiver session cannot certify a hold"
    );

    let mut fixture = Fixture::new(314);
    let (_mirror, receipt) = ready(&mut fixture.player);
    fixture.player.set_prepared_playing(true);
    fixture.player.execution_wake(1_000.0).unwrap();
    assert!(input(&mut fixture.player, "press", receipt, 1));
    assert!(input(&mut fixture.player, "release", receipt, 1));
    assert!(fixture.player.playback.reader.interactions_active());
    fixture
        .player
        .playback
        .reader
        .advance_interactions(1.1)
        .unwrap();
    fixture
        .player
        .playback
        .reader
        .advance_interactions(1.2)
        .unwrap();
    assert!(fixture.player.playback.reader.wake_state().frame_pending());
    let dirty: serde_json::Value = serde_json::from_str(
        &fixture
            .player
            .prepared_playback_state_json(1_100.0)
            .unwrap(),
    )
    .unwrap();
    assert!(
        dirty.get("heldPresentation").is_none(),
        "pending interaction work cannot reuse an earlier picture"
    );
}

fn ready(
    player: &mut PreparedExecutionPlayer,
) -> (InstalledRetainedExecutionMirror, WorkerPointerReceipt) {
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(
        &player.resource_bundle_bytes().unwrap(),
    )
    .unwrap();
    let json = player.initial_delta_json().unwrap();
    let delta = decode(&json);
    receive(&mut mirror, &json);
    let receipt = WorkerPointerReceipt {
        session: delta.retained.session,
        sequence: delta.retained.sequence,
        presentation: 1,
        view_revision: delta.pointer_view.unwrap().revision,
    };
    assert!(player
        .note_pointer_presentation_json(&serde_json::to_string(&receipt).unwrap())
        .unwrap());
    (mirror, receipt)
}

fn input(
    player: &mut PreparedExecutionPlayer,
    kind: &str,
    receipt: WorkerPointerReceipt,
    source: u64,
) -> bool {
    player.submit_browser_pointer_input_json(&serde_json::json!({
        "input": { "kind":kind, "source_id":source,"pointer_id":7,"view_revision":1,
            "surface_x":400.0,"surface_y":200.0,"viewport_width":800.0,"viewport_height":400.0,"button":0 },
        "presentation":receipt,
    }).to_string()).unwrap()
}

#[test]
fn prepared_worker_snapshot_uses_existing_receiver_and_clean_frames_send_nothing() {
    let mut f = Fixture::new(71);
    let (mirror, _) = ready(&mut f.player);
    assert_eq!(mirror.frame().unwrap(), f.player.playback.reader.frame());
    assert_eq!(mirror.frame().unwrap().time, 0.5);
    for _ in 0..8 {
        assert!(f.player.drain_delta_json().unwrap().is_none());
        assert_eq!(f.player.execution_wake(1_000.0).unwrap().cadence(), "idle");
        assert!(!f.player.needs_preparation(0.25).unwrap());
    }
    assert!(f.player.needs_preparation(1.0).unwrap());
    assert!(f.player.needs_preparation(f64::NAN).is_err());
}

#[test]
fn prepared_audio_uses_reader_resources_and_never_reports_frontier_as_duration() {
    let mut scene = Scene::new();
    let resource =
        noon_core::AudioResource::from_interleaved(8_000, 1, vec![0.125; 8_000], 32_000).unwrap();
    scene
        .audio(resource, noon_core::AudioClipOptions::default())
        .unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let segment = source.wait_segment(0.25).unwrap();
    source.advance_to(segment.end_time()).unwrap();
    let (mut publisher, reader) = source.create_prepared_prefix(segment).unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, 951);
    player.set_prepared_playing(false);

    assert!(player.has_audio());
    assert_eq!(player.finite_audio_duration(), None);
    let initial_anchor: serde_json::Value = serde_json::from_str(
        &player
            .audio_anchor_json_at(0.0, 10.0)
            .expect("open prefix audio anchor"),
    )
    .unwrap();
    assert_eq!(initial_anchor["time"], 0.0);
    assert_eq!(initial_anchor["playing"], false);
    assert_eq!(initial_anchor["buffering"], false);
    assert_eq!(initial_anchor["authorizedUntil"], 0.25);
    assert_eq!(initial_anchor["sampledEpochMs"], 10.0);
    player.set_prepared_playing(true);
    player.execution_wake(1_000.0).unwrap();
    player.tick_delta_json(1_250.0).unwrap();
    assert!(player.prepared_buffering());
    let buffered_anchor: serde_json::Value = serde_json::from_str(
        &player
            .audio_anchor_json_at(1_250.0, 15.0)
            .expect("buffered audio anchor"),
    )
    .unwrap();
    assert_eq!(buffered_anchor["playing"], true);
    assert_eq!(buffered_anchor["buffering"], true);
    let resources = crate::worker_audio::WorkerAudioResources::decode(
        &player.audio_resource_bundle_bytes().unwrap(),
    )
    .unwrap();
    assert_eq!(resources.session(), 951);
    assert_eq!(resources.at_time(0.125).unwrap()[0].source_offset, 0.125);

    let next = source.wait_segment(0.5).unwrap();
    source.advance_to(next.end_time()).unwrap();
    let batch = source
        .publish_prepared_prefix(next, &mut publisher)
        .unwrap();
    player.preflight_append(&batch).unwrap();
    player.append(batch).unwrap();
    let resumed_anchor: serde_json::Value = serde_json::from_str(
        &player
            .audio_anchor_json_at(1_250.0, 20.0)
            .expect("resumed audio anchor"),
    )
    .unwrap();
    assert_eq!(resumed_anchor["buffering"], false);
    player.set_prepared_playing(false);
    let extended_anchor: serde_json::Value = serde_json::from_str(
        &player
            .audio_anchor_json_at(0.0, 20.0)
            .expect("extended open prefix audio anchor"),
    )
    .unwrap();
    assert_eq!(extended_anchor["time"], 0.0);
    assert_eq!(extended_anchor["playing"], false);
    assert_eq!(extended_anchor["authorizedUntil"], 0.75);
    assert_eq!(player.finite_audio_duration(), None);

    let original = source.audio_playback_at(0.125).unwrap().collect::<Vec<_>>();
    let mirrored = resources.at_time(0.125).unwrap();
    assert_eq!(original.len(), 1);
    assert_eq!(mirrored.len(), 1);
    assert_eq!(original[0].resource, mirrored[0].resource);
    assert_eq!(original[0].pcm, mirrored[0].pcm);
    assert_eq!(original[0].source_offset, mirrored[0].source_offset);
}

#[test]
fn prepared_sections_share_reader_admission_and_terminal_endpoint_rules() {
    let mut f = Fixture::new(721);
    let mut scene = Scene::new();
    for (name, time) in [
        ("Start", 0.0),
        ("Interior", 0.5),
        ("Open frontier", 1.0),
        ("Later", 2.0),
        ("Terminal", 3.0),
    ] {
        scene
            .next_section_at(name, noon::SectionType::Normal, time)
            .unwrap();
    }
    f.player.set_section_navigation(scene.section_navigation());
    let _ = ready(&mut f.player);

    let sections: serde_json::Value = serde_json::from_str(&f.player.sections_json()).unwrap();
    assert_eq!(sections.as_array().unwrap().len(), 2);
    assert_eq!(sections[1]["index"], 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&f.player.section_state_json(0.5).unwrap())
            .unwrap(),
        serde_json::json!({"current":1,"previous":0,"next":null}),
        "a cue at the open frontier is not a selectable next target",
    );
    assert!(f
        .player
        .seek_section_delta_json(false, 0.5)
        .unwrap()
        .is_none());

    f.player.set_prepared_playing(false);
    let displayed_time = f.player.prepared_time();
    let interior_batch = f.next(false);
    f.deliver(interior_batch);
    assert_eq!(
        f.player.prepared_time(),
        displayed_time,
        "append does not seek a paused reader"
    );
    assert!(
        !f.player.prepared_playing(),
        "append preserves pause intent"
    );
    let sections: serde_json::Value = serde_json::from_str(&f.player.sections_json()).unwrap();
    assert_eq!(sections.as_array().unwrap().len(), 3);
    assert_eq!(sections[2]["index"], 2);
    let delta = f
        .player
        .seek_section_delta_json(false, 0.5)
        .unwrap()
        .unwrap();
    assert!(!delta.is_empty());
    assert_eq!(f.player.prepared_time(), 1.0);

    let terminal_batch = f.next(true);
    f.deliver(terminal_batch);
    let sections: serde_json::Value = serde_json::from_str(&f.player.sections_json()).unwrap();
    assert_eq!(sections.as_array().unwrap().last().unwrap()["index"], 4);
    assert_eq!(sections.as_array().unwrap().last().unwrap()["time"], 3.0);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&f.player.section_state_json(3.0).unwrap())
            .unwrap()["current"],
        4,
        "terminal endpoint cue becomes current only after terminal adoption",
    );
}

#[test]
fn no_target_section_navigation_refreshes_the_current_no_change_receipt() {
    let mut f = Fixture::new(722);
    let (_, displayed) = ready(&mut f.player);
    f.player.no_change_presentation = Some(PreparedNoChangePresentation {
        session: displayed.session,
        sequence: displayed.sequence,
        time: 99.0,
        publication_context: f.player.playback.reader.publication_context(),
    });

    assert!(f
        .player
        .seek_section_delta_json(false, f.player.prepared_time())
        .unwrap()
        .is_none());
    let proof: serde_json::Value = serde_json::from_str(
        &f.player
            .take_prepared_no_change_presentation_json()
            .expect("no-target command refreshes the Rust receipt"),
    )
    .unwrap();
    assert_eq!(proof["session"], displayed.session);
    assert_eq!(proof["sequence"], displayed.sequence);
    assert_eq!(proof["time"], f.player.prepared_time());
}

#[test]
fn prepared_worker_future_delivery_preserves_receipt_and_pending_click() {
    let mut f = Fixture::new(72);
    let (mut mirror, receipt) = ready(&mut f.player);
    let old = f.player.playback.reader.publication_context();
    assert!(input(&mut f.player, "press", receipt, 1));
    let batch = f.next(false);
    f.deliver(batch);
    assert_eq!(f.player.playback.reader.publication_context(), old);
    assert!(f.player.drain_delta_json().unwrap().is_none());
    assert_eq!(mirror.frame().unwrap(), f.player.playback.reader.frame());
    assert!(input(&mut f.player, "release", receipt, 1));
    assert!(f.player.playback.reader.interactions_active());
    let baseline = mirror.frame().unwrap().clone();
    let mut last_sequence = receipt.sequence;
    if let Some(json) = f.player.tick_delta_json(1_000.0).unwrap() {
        last_sequence = decode(&json).retained.sequence;
        receive(&mut mirror, &json);
    }
    let json = f.player.tick_delta_json(1_250.0).unwrap().unwrap();
    assert_eq!(decode(&json).retained.sequence, last_sequence + 1);
    receive(&mut mirror, &json);
    assert_ne!(mirror.frame().unwrap(), &baseline);
    assert_eq!(mirror.frame().unwrap().time, 0.5);
    let json = f.player.tick_delta_json(1_500.0).unwrap().unwrap();
    receive(&mut mirror, &json);
    assert_eq!(mirror.frame().unwrap(), &baseline);
    assert!(f.player.drain_delta_json().unwrap().is_none());
}

#[test]
fn prepared_worker_consuming_delta_is_not_presentation_or_input_authority() {
    let mut f = Fixture::new(73);
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(
        &f.player.resource_bundle_bytes().unwrap(),
    )
    .unwrap();
    let json = f.player.initial_delta_json().unwrap();
    receive(&mut mirror, &json);
    let receipt = WorkerPointerReceipt {
        session: 73,
        sequence: 0,
        presentation: 1,
        view_revision: 1,
    };
    assert!(!input(&mut f.player, "press", receipt, 1));
    assert_eq!(f.player.sequence, 0);
    assert!(f
        .player
        .note_pointer_presentation_json(&serde_json::to_string(&receipt).unwrap())
        .unwrap());
    assert!(!input(&mut f.player, "release", receipt, 1));
    assert!(input(&mut f.player, "press", receipt, 2));
    assert!(input(&mut f.player, "release", receipt, 2));
    assert!(f.player.playback.reader.interactions_active());
}

#[test]
fn prepared_worker_terminal_adoption_and_source_drop_do_not_replace_display() {
    let mut f = Fixture::new(74);
    let (mut mirror, receipt) = ready(&mut f.player);
    let baseline = mirror.frame().unwrap().clone();
    let batch = f.next(true);
    f.deliver(batch);
    assert_eq!(f.player.playback.reader.end_time(), Some(2.0));
    assert!(f.player.drain_delta_json().unwrap().is_none());
    let mut player = f.player;
    drop((f.source, f.target, f.publisher));
    assert!(input(&mut player, "press", receipt, 1));
    assert!(input(&mut player, "release", receipt, 1));
    assert!(player.playback.reader.interactions_active());
    let json = player.seek_delta_json(2.0).unwrap().unwrap();
    receive(&mut mirror, &json);
    assert!(!player.playback.reader.interactions_active());
    assert_eq!(mirror.frame().unwrap().time, 2.0);
    assert!(mirror
        .frame()
        .unwrap()
        .objects
        .iter()
        .any(|object| object.transform.translation.x == 1.0));
    let json = player.seek_delta_json(0.5).unwrap().unwrap();
    receive(&mut mirror, &json);
    assert_eq!(mirror.frame().unwrap(), &baseline);
    assert!(!input(&mut player, "press", receipt, 2));
}

#[test]
fn prepared_idle_no_change_proof_joins_the_last_wire_and_is_consumed_once() {
    let mut f = Fixture::new(78);
    ready(&mut f.player);

    assert!(f.player.drain_delta_json().unwrap().is_none());
    let proof: serde_json::Value = serde_json::from_str(
        &f.player
            .take_prepared_no_change_presentation_json()
            .expect("unchanged drain has a retained publication to prove against"),
    )
    .unwrap();
    assert_eq!(proof["session"], 78);
    assert_eq!(proof["sequence"], 0);
    assert_eq!(proof["time"], 0.5);
    assert_eq!(
        proof["publicationContext"]["frame_epoch"],
        f.player
            .playback
            .reader
            .publication_context()
            .frame_epoch()
            .get()
    );
    assert!(f
        .player
        .take_prepared_no_change_presentation_json()
        .is_none());
    assert!(f.player.drain_delta_json().unwrap().is_none());
    assert!(f
        .player
        .take_prepared_no_change_presentation_json()
        .is_some());
    assert!(f.player.seek_delta_json(1.0).is_err());
    assert!(f
        .player
        .take_prepared_no_change_presentation_json()
        .is_none());
    assert!(f.player.seek_delta_json(0.75).unwrap().is_some());
    assert!(f
        .player
        .take_prepared_no_change_presentation_json()
        .is_none());
}

#[test]
fn prepared_worker_unavailable_seek_is_atomic_and_buffered_wall_time_is_not_charged() {
    let mut f = Fixture::new(75);
    ready(&mut f.player);
    f.player.set_prepared_playing(true);
    assert_eq!(
        f.player
            .execution_wake(1_000.0)
            .unwrap()
            .timer_after_milliseconds(),
        Some(500.0)
    );
    let frame = f.player.playback.reader.frame().clone();
    let publication = f.player.playback.reader.publication_context();
    let clock = f.player.clock;
    assert!(f.player.seek_delta_json(1.0).is_err());
    assert!(f.player.tick_delta_transport_json(f64::INFINITY).is_err());
    assert_eq!(f.player.clock, clock);
    assert_eq!(f.player.playback.reader.publication_context(), publication);
    assert!(f.player.tick_delta_json(1_500.0).unwrap().is_none());
    assert!(f.player.playback.buffering());
    assert_eq!(f.player.playback.reader.frame(), &frame);
    f.player.reset_receiver(76).unwrap();
    let (recovered, _) = ready(&mut f.player);
    assert_eq!(recovered.frame().unwrap(), &frame);
    assert!(f.player.prepared_playing());
    assert!(f.player.prepared_buffering());
    assert_eq!(f.player.execution_wake(90_000.0).unwrap().cadence(), "idle");
    assert!(f.player.needs_preparation(0.125).unwrap());
    let batch = f.next(false);
    f.deliver(batch);
    assert_eq!(
        f.player
            .execution_wake(100_000.0)
            .unwrap()
            .timer_after_milliseconds(),
        Some(500.0)
    );
    assert!(f.player.tick_delta_json(100_250.0).unwrap().is_none());
    assert_eq!(f.player.playback.reader.frame(), &frame);
    assert_eq!(f.player.playback.reader.publication_context(), publication);
}

#[test]
fn prepared_worker_wire_carrier_preserves_exact_envelope_metadata() {
    let mut f = Fixture::new(76);
    let bundle = f.player.resource_bundle_bytes().unwrap();
    let carrier = f.player.drain_delta_transport_json().unwrap().unwrap();
    let (header, body) = carrier.split_once('\n').unwrap();
    assert!(header.len() <= 512);
    let header: serde_json::Value = serde_json::from_str(header).unwrap();
    let delta = decode(body);
    assert_eq!(header["session"], delta.retained.session);
    assert_eq!(header["sequence"], delta.retained.sequence);
    assert_eq!(header["snapshot"], true);
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bundle).unwrap();
    receive(&mut mirror, body);
    assert_eq!(mirror.frame().unwrap(), f.player.playback.reader.frame());
    assert!(f.player.drain_delta_transport_json().unwrap().is_none());
}

#[test]
fn prepared_tick_transport_preserves_the_tick_json_and_header_metadata() {
    let mut json = Fixture::new(76);
    let mut carrier = Fixture::new(76);
    for fixture in [&mut json, &mut carrier] {
        let (_, receipt) = ready(&mut fixture.player);
        assert!(input(&mut fixture.player, "press", receipt, 1));
        let batch = fixture.next(false);
        fixture.deliver(batch);
        assert!(input(&mut fixture.player, "release", receipt, 1));
        fixture.player.set_prepared_playing(true);
        fixture.player.execution_wake(1_000.0).unwrap();
    }

    let body = |packet: String| {
        let (header, body) = packet.split_once('\n').unwrap();
        assert!(header.len() <= 512);
        let header: serde_json::Value = serde_json::from_str(header).unwrap();
        let delta = decode(body);
        assert_eq!(header["channel"], "noon.execution.retained");
        assert_eq!(header["session"], delta.retained.session);
        assert_eq!(header["sequence"], delta.retained.sequence);
        assert_eq!(header["snapshot"], delta.retained.snapshot);
        body.to_owned()
    };
    let expected = json.player.tick_delta_json(1_000.0).unwrap();
    let actual = carrier
        .player
        .tick_delta_transport_json(1_000.0)
        .unwrap()
        .map(body);
    assert_eq!(actual, expected);

    let expected = json
        .player
        .tick_delta_json(1_250.0)
        .unwrap()
        .expect("active interaction should publish the later tick");
    let actual = carrier
        .player
        .tick_delta_transport_json(1_250.0)
        .unwrap()
        .expect("carrier tick should preserve the active interaction publication");
    assert_eq!(body(actual), expected);
}

#[test]
fn prepared_worker_foreign_batch_and_receipt_rejection_do_not_consume_owner_state() {
    let mut f = Fixture::new(77);
    let (_, receipt) = ready(&mut f.player);
    let mut foreign = Fixture::new(78);
    let batch = foreign.next(false);
    let frame = f.player.playback.reader.frame().clone();
    let revision = f.player.playback.reader.publication_context();
    assert!(f.player.preflight_append(&batch).is_err());
    foreign.deliver(batch);
    assert!(!input(
        &mut f.player,
        "press",
        WorkerPointerReceipt {
            session: 78,
            ..receipt
        },
        1
    ));
    assert_eq!(f.player.sequence, 0);
    assert_eq!(f.player.playback.reader.frame(), &frame);
    assert_eq!(f.player.playback.reader.publication_context(), revision);
    assert!(f.player.drain_delta_json().unwrap().is_none());
}

#[test]
fn prepared_worker_surface_retirement_cancels_gesture_not_retained_history() {
    let mut f = Fixture::new(79);
    let (_, receipt) = ready(&mut f.player);
    assert!(input(&mut f.player, "press", receipt, 1));
    let history = f.player.playback.reader.replay_stats();
    assert!(f
        .player
        .invalidate_pointer_presentation_json(&serde_json::to_string(&receipt).unwrap())
        .unwrap());
    assert!(!input(&mut f.player, "release", receipt, 1));
    assert!(!f.player.playback.reader.interactions_active());
    assert_eq!(f.player.playback.reader.replay_stats(), history);
    let json = f.player.drain_delta_json().unwrap().unwrap();
    let d = decode(&json);
    assert_eq!(d.retained.sequence, 1);
    let new_receipt = WorkerPointerReceipt {
        sequence: 1,
        presentation: 2,
        ..receipt
    };
    assert!(f
        .player
        .note_pointer_presentation_json(&serde_json::to_string(&new_receipt).unwrap())
        .unwrap());
    assert!(input(&mut f.player, "press", new_receipt, 2));
    assert!(input(&mut f.player, "release", new_receipt, 2));
    assert!(f.player.playback.reader.interactions_active());
}

#[test]
fn prepared_worker_cannot_rebootstrap_an_already_started_resource_stream() {
    let mut f = Fixture::new(80);
    ready(&mut f.player);
    let context = f.player.playback.reader.publication_context();
    assert!(f.player.resource_bundle_bytes().is_err());
    assert!(f.player.initial_delta_json().is_err());
    assert_eq!(f.player.playback.reader.publication_context(), context);
    assert!(f.player.drain_delta_json().unwrap().is_none());
}

#[test]
fn prepared_receiver_reset_cancels_old_contact_and_preserves_the_reader() {
    let mut f = Fixture::new(80);
    assert!(f.player.reset_receiver(81).is_err());
    let (_, old_receipt) = ready(&mut f.player);
    f.player.set_prepared_playing(true);
    f.player.execution_wake(1_000.0).unwrap();
    assert!(input(&mut f.player, "press", old_receipt, 1));
    let identity = f.player.playback.reader.runtime_identity();
    for invalid in [0, 79, 80] {
        let context = f.player.playback.reader.publication_context();
        assert!(f.player.reset_receiver(invalid).is_err());
        assert_eq!(f.player.playback.reader.publication_context(), context);
    }
    f.player.reset_receiver(81).unwrap();
    assert_eq!(f.player.playback.reader.runtime_identity(), identity);
    assert_eq!(f.player.prepared_time(), 0.5);
    assert_eq!(f.player.prepared_frontier(), 1.0);
    assert_eq!(f.player.prepared_end_time(), None);
    assert!(f.player.prepared_playing());
    assert_eq!(f.player.prepared_playback_time_at(90_000.0).unwrap(), 0.5);
    assert!(!input(&mut f.player, "release", old_receipt, 1));
    assert!(!f.player.playback.reader.interactions_active());
    let (mirror, receipt) = ready(&mut f.player);
    assert_eq!(receipt.session, 81);
    assert_eq!(receipt.sequence, 0);
    assert_eq!(receipt.presentation, 1);
    assert_eq!(mirror.frame().unwrap(), f.player.playback.reader.frame());
    assert_eq!(
        f.player
            .execution_wake(90_000.0)
            .unwrap()
            .timer_after_milliseconds(),
        Some(500.0)
    );
    assert_eq!(f.player.prepared_playback_time_at(90_250.0).unwrap(), 0.75);
    assert!(!f
        .player
        .note_pointer_presentation_json(&serde_json::to_string(&old_receipt).unwrap())
        .unwrap());
    assert!(input(&mut f.player, "press", receipt, 2));
    assert!(input(&mut f.player, "release", receipt, 2));
    assert!(f.player.playback.reader.interactions_active());
    // Delivery continues through the same publisher after receiver replacement.
    let next = f.next(true);
    f.deliver(next);
    assert_eq!(f.player.prepared_end_time(), Some(2.0));
    assert_eq!(f.player.prepared_time(), 0.5);
}

#[test]
fn prepared_worker_new_image_resources_arrive_only_when_history_is_selected() {
    let mut f = Fixture::new(81);
    let (mut mirror, _) = ready(&mut f.player);
    let baseline = mirror.frame().unwrap().clone();
    let image = f
        .source
        .scene_mut()
        .image(noon::ImageMobjectOptions::rgba8(2, 2, [230, 40, 10, 255].repeat(4)).unwrap())
        .unwrap();
    f.source.live().add(&image).unwrap();
    let segment = f.source.session_mut().wait_segment(1.0).unwrap();
    f.source.session_mut().advance_to(2.0).unwrap();
    let batch = f
        .source
        .session()
        .publish_prepared_prefix(segment, &mut f.publisher)
        .unwrap();
    f.deliver(batch);
    assert!(f.player.drain_delta_json().unwrap().is_none());
    assert_eq!(mirror.frame().unwrap(), &baseline);
    let json = f.player.seek_delta_json(1.5).unwrap().unwrap();
    // The actual strict receiver needs the image resource closure in this
    // publication, not a producer-wide bootstrap or a future-history mirror.
    receive(&mut mirror, &json);
    assert!(mirror
        .frame()
        .unwrap()
        .objects
        .iter()
        .any(|row| row.content.image().is_some()));
    let selected = mirror.frame().unwrap().clone();
    let identity = f.player.playback.reader.runtime_identity();
    let future = f
        .source
        .scene_mut()
        .image(noon::ImageMobjectOptions::rgba8(2, 2, [10, 230, 40, 255].repeat(4)).unwrap())
        .unwrap();
    f.source.live().add(&future).unwrap();
    let segment = f.source.session_mut().wait_segment(1.0).unwrap();
    f.source.session_mut().advance_to(3.0).unwrap();
    let batch = f
        .source
        .session()
        .publish_prepared_prefix(segment, &mut f.publisher)
        .unwrap();
    f.deliver(batch);
    let reader_frame = f.player.playback.reader.frame().clone();
    f.player.reset_receiver(82).unwrap();
    let (recovered, _) = ready(&mut f.player);
    // Each installed receiver remaps resource handles into its own local arena.
    // Compare resource contents and all frame fields after that one remapping.
    let old_image = selected.objects[1].content.image().unwrap();
    let new_image = recovered.frame().unwrap().objects[1]
        .content
        .image()
        .unwrap();
    assert_eq!(
        mirror.resources().images().get(old_image.resource()),
        recovered.resources().images().get(new_image.resource())
    );
    assert_eq!(
        (old_image.width(), old_image.height(), old_image.sampling()),
        (new_image.width(), new_image.height(), new_image.sampling())
    );
    let mut remapped = recovered.frame().unwrap().clone();
    remapped.objects[1].content = selected.objects[1].content.clone();
    assert_eq!(remapped, selected);
    assert_eq!(f.player.playback.reader.frame(), &reader_frame);
    assert_eq!(
        recovered.resources().inventory().counts()[0],
        1,
        "only the selected image is bootstrapped"
    );
    assert_eq!(f.player.playback.reader.runtime_identity(), identity);
    assert_eq!(f.player.prepared_frontier(), 3.0);
    mirror = recovered;
    assert!(f.player.drain_delta_json().unwrap().is_none());
    receive(
        &mut mirror,
        &f.player.seek_delta_json(0.5).unwrap().unwrap(),
    );
    assert_eq!(mirror.frame().unwrap(), &baseline);
    receive(
        &mut mirror,
        &f.player.seek_delta_json(1.5).unwrap().unwrap(),
    );
    assert!(mirror
        .frame()
        .unwrap()
        .objects
        .iter()
        .any(|row| row.content.image().is_some()));
}

#[test]
fn prepared_worker_status_observations_never_start_the_presentation_clock() {
    let mut f = Fixture::new(82);
    f.player.set_prepared_playing(true);
    assert_eq!(f.player.prepared_time(), 0.5);
    assert_eq!(f.player.prepared_frontier(), 1.0);
    assert_eq!(f.player.prepared_end_time(), None);
    assert!(f.player.prepared_playing());
    assert!(!f.player.prepared_buffering());
    let clock = f.player.clock;
    let publication = f.player.playback.reader.publication_context();
    for wall in [0.0, 1_000.0, 50_000.0] {
        assert_eq!(f.player.prepared_playback_time_at(wall).unwrap(), 0.5);
    }
    assert_eq!(f.player.clock, clock);
    assert_eq!(f.player.playback.reader.publication_context(), publication);
    let batch = f.next(true);
    assert_eq!(f.player.prepared_frontier(), 1.0);
    assert_eq!(f.player.prepared_end_time(), None);
    f.deliver(batch);
    assert_eq!(f.player.prepared_time(), 0.5);
    assert_eq!(f.player.prepared_frontier(), 2.0);
    assert_eq!(f.player.prepared_end_time(), Some(2.0));
    assert_eq!(f.player.clock, clock);
}

#[test]
fn prepared_worker_static_pause_preserves_elapsed_time_and_resume_excludes_paused_wall_time() {
    let mut f = Fixture::new(83);
    ready(&mut f.player);
    f.player.set_prepared_playing(true);
    assert_eq!(
        f.player
            .execution_wake(1_000.0)
            .unwrap()
            .timer_after_milliseconds(),
        Some(500.0)
    );
    let frame = f.player.playback.reader.frame().clone();
    let clock = f.player.clock;
    assert_eq!(f.player.prepared_playback_time_at(900.0).unwrap(), 0.5);
    assert_eq!(f.player.prepared_playback_time_at(1_250.0).unwrap(), 0.75);
    assert_eq!(f.player.playback.reader.frame(), &frame);
    assert_eq!(f.player.clock, clock);
    assert!(f.player.drain_delta_json().unwrap().is_none());
    f.player.pause_prepared_at(1_250.0).unwrap();
    assert_eq!(f.player.prepared_time(), 0.75);
    assert!(!f.player.prepared_playing());
    assert!(!f.player.prepared_buffering());
    assert_eq!(f.player.prepared_playback_time_at(90_000.0).unwrap(), 0.75);
    f.player.pause_prepared_at(90_000.0).unwrap();
    assert_eq!(f.player.prepared_time(), 0.75);
    f.player.set_prepared_playing(true);
    assert_eq!(f.player.prepared_playback_time_at(100_000.0).unwrap(), 0.75);
    assert_eq!(
        f.player
            .execution_wake(100_000.0)
            .unwrap()
            .timer_after_milliseconds(),
        Some(250.0)
    );
    assert_eq!(
        f.player.prepared_playback_time_at(100_125.0).unwrap(),
        0.875
    );
    f.player.pause_prepared_at(100_125.0).unwrap();
    assert_eq!(f.player.prepared_time(), 0.875);
}

#[test]
fn prepared_worker_pause_at_open_frontier_buffers_without_selecting_an_epsilon_frame() {
    let mut f = Fixture::new(84);
    ready(&mut f.player);
    f.player.set_prepared_playing(true);
    f.player.execution_wake(1_000.0).unwrap();
    let frame = f.player.playback.reader.frame().clone();
    let publication = f.player.playback.reader.publication_context();
    assert_eq!(f.player.prepared_playback_time_at(50_000.0).unwrap(), 1.0);
    assert!(
        !f.player.prepared_buffering(),
        "observation does not mutate readiness"
    );
    f.player.pause_prepared_at(50_000.0).unwrap();
    assert!(f.player.prepared_buffering());
    assert!(!f.player.prepared_playing());
    assert_eq!(f.player.prepared_end_time(), None);
    assert_eq!(f.player.playback.reader.frame(), &frame);
    assert_eq!(f.player.playback.reader.publication_context(), publication);
    assert!(f.player.drain_delta_json().unwrap().is_none());
    assert_eq!(f.player.prepared_playback_time_at(90_000.0).unwrap(), 0.5);
    let batch = f.next(false);
    f.deliver(batch);
    assert!(!f.player.prepared_buffering());
    assert!(!f.player.prepared_playing());
    f.player.set_prepared_playing(true);
    assert_eq!(f.player.prepared_playback_time_at(100_000.0).unwrap(), 0.5);
    assert_eq!(
        f.player
            .execution_wake(100_000.0)
            .unwrap()
            .timer_after_milliseconds(),
        Some(500.0)
    );
    assert_eq!(f.player.prepared_playback_time_at(100_250.0).unwrap(), 0.75);
}

#[test]
fn prepared_worker_static_observation_stops_at_next_revision_before_terminal_time() {
    let mut f = Fixture::new(85);
    let batch = f.next(true);
    f.deliver(batch);
    ready(&mut f.player);
    f.player.set_prepared_playing(true);
    f.player.execution_wake(1_000.0).unwrap();
    assert_eq!(f.player.prepared_end_time(), Some(2.0));
    assert_eq!(f.player.prepared_playback_time_at(9_000.0).unwrap(), 1.0);
    f.player.pause_prepared_at(9_000.0).unwrap();
    assert_eq!(f.player.prepared_time(), 1.0);
    assert!(!f.player.prepared_buffering());
    assert!(f
        .player
        .playback
        .reader
        .frame()
        .objects
        .iter()
        .any(|row| row.transform.translation.x == 1.0));
    f.player.set_prepared_playing(true);
    f.player.execution_wake(10_000.0).unwrap();
    assert_eq!(f.player.prepared_playback_time_at(20_000.0).unwrap(), 2.0);
    f.player.pause_prepared_at(20_000.0).unwrap();
    assert_eq!(f.player.prepared_time(), 2.0);
    assert_eq!(f.player.prepared_end_time(), Some(2.0));
    assert!(!f.player.prepared_buffering());
}

#[test]
fn prepared_worker_loop_reanchors_the_existing_wall_clock_at_each_wrap() {
    let mut f = Fixture::new(851);
    let terminal = f.next(true);
    f.deliver(terminal);
    ready(&mut f.player);
    assert_eq!(playback_state(&f.player)["completed"], false);
    f.player.set_prepared_loop_duration(Some(2.0)).unwrap();
    f.player.set_prepared_playing(true);
    f.player.tick_delta_json(1_000.0).unwrap();
    f.player.tick_delta_json(3_500.0).unwrap();
    assert_eq!(f.player.prepared_time(), 1.0);
    assert_eq!(playback_state(&f.player)["loopCount"], 1);
    assert_eq!(playback_state(&f.player)["completed"], false);
    assert!(f.player.prepared_playing());
    assert_eq!(f.player.prepared_playback_time_at(4_000.0).unwrap(), 1.5);
    f.player.tick_delta_json(5_000.0).unwrap();
    assert_eq!(f.player.prepared_time(), 0.5);
    assert_eq!(playback_state(&f.player)["loopCount"], 2);
    assert_eq!(f.player.prepared_playback_time_at(5_250.0).unwrap(), 0.75);
    f.player.set_prepared_playing(false);
    f.player.set_prepared_playing(true);
    f.player.tick_delta_json(6_000.0).unwrap();
    f.player.tick_delta_json(6_500.0).unwrap();
    assert_eq!(f.player.prepared_time(), 1.0);
    f.player.tick_delta_json(11_000.0).unwrap();
    assert_eq!(f.player.prepared_time(), 1.5);
    assert_eq!(playback_state(&f.player)["loopCount"], 4);
    f.player.set_prepared_playing(false);
    f.player.set_prepared_playing(true);
    f.player.tick_delta_json(12_000.0).unwrap();
    f.player.tick_delta_json(13_000.0).unwrap();
    assert_eq!(f.player.prepared_time(), 0.5);
    assert_eq!(playback_state(&f.player)["loopCount"], 5);
}

#[test]
fn prepared_worker_pause_before_tick_commits_elapsed_loop_wrap() {
    let mut f = Fixture::new(852);
    let terminal = f.next(true);
    f.deliver(terminal);
    f.player.set_prepared_loop_duration(Some(2.0)).unwrap();
    let expected_frame = f.player.playback.reader.frame().clone();
    f.player.seek_delta_json(2.0).unwrap();
    f.player.set_prepared_playing(true);
    f.player.execution_wake(1_000.0).unwrap();

    assert_eq!(f.player.prepared_playback_time_at(1_500.0).unwrap(), 0.5);
    f.player.pause_prepared_at(1_500.0).unwrap();
    assert_eq!(f.player.prepared_time(), 0.5);
    assert_eq!(f.player.playback.reader.frame(), &expected_frame);
    assert_eq!(playback_state(&f.player)["loopCount"], 1);
    assert!(!f.player.prepared_playing());
}

#[test]
fn prepared_worker_static_time_controls_reject_invalid_wall_time_atomically() {
    let mut f = Fixture::new(86);
    ready(&mut f.player);
    f.player.set_prepared_playing(true);
    f.player.execution_wake(1_000.0).unwrap();
    let frame = f.player.playback.reader.frame().clone();
    let publication = f.player.playback.reader.publication_context();
    let clock = f.player.clock;
    for wall in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert!(f.player.prepared_playback_time_at(wall).is_err());
        assert!(f.player.pause_prepared_at(wall).is_err());
        assert_eq!(f.player.clock, clock);
        assert_eq!(f.player.playback.reader.frame(), &frame);
        assert_eq!(f.player.playback.reader.publication_context(), publication);
        assert!(f.player.prepared_playing());
        assert!(!f.player.prepared_buffering());
    }
    assert!(f.player.drain_delta_json().unwrap().is_none());
}

#[test]
fn prepared_worker_active_animation_observation_keeps_the_coherent_sample() {
    let mut f = Fixture::new(87);
    let segment = {
        let mut live = f.source.live();
        let target = live.target_editor(&f.target).unwrap();
        live.set_translation(&target, 2.0, 0.0).unwrap();
        let segment = live
            .declare_and_activate_transform_to(
                &f.target,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(noon_core::RateFunction::Linear),
            )
            .unwrap();
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        segment
    };
    let batch = f
        .source
        .session()
        .publish_prepared_prefix(segment, &mut f.publisher)
        .unwrap();
    f.deliver(batch);
    f.player.seek_delta_json(1.25).unwrap();
    f.player.set_prepared_playing(true);
    assert_eq!(
        f.player.execution_wake(1_000.0).unwrap().cadence(),
        "animation_frame"
    );
    let frame = f.player.playback.reader.frame().clone();
    assert_eq!(f.player.prepared_playback_time_at(1_500.0).unwrap(), 1.25);
    f.player.pause_prepared_at(1_500.0).unwrap();
    assert_eq!(f.player.prepared_time(), 1.25);
    assert_eq!(f.player.playback.reader.frame(), &frame);
    assert!(!f.player.prepared_playing());
}
