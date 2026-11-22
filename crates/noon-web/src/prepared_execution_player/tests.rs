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
        Self::with_click_binding(session, true)
    }

    fn with_click_binding(session: u32, click_indicate: bool) -> Self {
        let mut scene = Scene::new();
        let mut target = scene.circle(0.65).unwrap();
        target.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
        target.set_stroke_width(0.0).unwrap();
        scene.add(&target).unwrap();
        if click_indicate {
            scene
                .on_click_indicate(
                    &target,
                    IndicateOptions::default(),
                    AnimationOptions::new().run_time(0.5),
                )
                .unwrap();
        }
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
        self.source
            .scene_mut()
            .shift_3d(&self.target, noon::SemanticVec3::new(1.0, 0.0, 0.0))
            .unwrap();
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
    let mut source = noon::integration::scene_execution::lower(&scene).unwrap();
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

    let terminal = source.wait_segment(0.25).unwrap();
    source.advance_to(terminal.end_time()).unwrap();
    source.seal_replay().unwrap();
    player.set_prepared_loop_duration(Some(1.0)).unwrap();
    let batch = source
        .finish_prepared_prefix(terminal, &mut publisher)
        .unwrap();
    player.append(batch).unwrap();
    player.set_prepared_playing(true);
    player.execution_wake(2_000.0).unwrap();
    player.tick_delta_json(3_125.0).unwrap();
    let state = playback_state(&player);
    assert_eq!(state["time"], 1.0);
    assert_eq!(state["playing"], false);
    assert_eq!(state["completed"], true);
    assert_eq!(state["loopCount"], 0);
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
    f.source.scene_mut().add(&image).unwrap();
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
    f.source.scene_mut().add(&future).unwrap();
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
    let mut f = Fixture::with_click_binding(87, false);
    f.player.enable_default_viewer_selection();
    let (mut mirror, original_receipt) = ready(&mut f.player);
    let segment = {
        let target = f.source.scene_mut().target_editor(&f.target).unwrap();
        f.source
            .scene_mut()
            .set_translation(&target, 2.0, 0.0)
            .unwrap();
        f.source.scene_mut().set_rotation(&target, 0.4).unwrap();
        let segment = f
            .source
            .live()
            .declare_and_activate_transform_to(
                &f.target,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(noon_core::RateFunction::Linear),
            )
            .unwrap();
        f.source
            .live()
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        f.source.live().complete_segment(segment).unwrap();
        segment
    };
    let batch = f
        .source
        .session()
        .publish_prepared_prefix(segment, &mut f.publisher)
        .unwrap();
    f.deliver(batch);
    let selected = f.player.seek_delta_json(1.25).unwrap().unwrap();
    receive(&mut mirror, &selected);
    let receipt = WorkerPointerReceipt {
        sequence: decode(&selected).retained.sequence,
        presentation: original_receipt.presentation + 1,
        ..original_receipt
    };
    assert!(f
        .player
        .note_pointer_presentation_json(&serde_json::to_string(&receipt).unwrap())
        .unwrap());
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

    let original = mirror.frame().unwrap().clone();
    assert!(input(&mut f.player, "press", receipt, 1));
    assert!(input(&mut f.player, "release", receipt, 1));
    assert!(f.player.tick_delta_json(1_600.0).unwrap().is_none());
    let indicated = f.player.tick_delta_json(2_100.0).unwrap().unwrap();
    receive(&mut mirror, &indicated);
    let pulsed = mirror.frame().unwrap();
    assert_eq!(pulsed.objects[0].style.fill, Some(noon_core::YELLOW));
    assert_eq!(
        pulsed.render_transform(0).translation,
        original.render_transform(0).translation
    );
    assert!(
        (pulsed.render_transform(0).scale.x / original.render_transform(0).scale.x - 1.2).abs()
            < 1e-5
    );
    assert_eq!(f.player.playback.reader.frame(), &frame);
    receive(
        &mut mirror,
        &f.player.tick_delta_json(2_600.0).unwrap().unwrap(),
    );
    assert_eq!(mirror.frame().unwrap(), &original);

    // The render receipt and DOM events travel on independent channels. Keep
    // the earlier displayed pick after a newer frame has moved the target off
    // the original cursor, and allow that physical release to finish the click.
    f.player.set_prepared_playing(true);
    f.player.execution_wake(3_000.0).unwrap();
    let moved = decode(&f.player.tick_delta_json(3_500.0).unwrap().unwrap()).retained;
    assert!(moved.time > 1.6);
    assert!(
        f.player
            .playback
            .reader
            .frame()
            .render_transform(0)
            .translation
            .x
            > 1.0
    );
    let newer = WorkerPointerReceipt {
        sequence: moved.sequence,
        presentation: receipt.presentation + 1,
        ..receipt
    };
    assert!(f
        .player
        .note_pointer_presentation_json(&serde_json::to_string(&newer).unwrap())
        .unwrap());
    assert!(input(&mut f.player, "press", receipt, 1));
    assert!(input(&mut f.player, "release", newer, 1));
    assert!(f.player.playback.reader.viewer_inspection_active());
}

#[test]
fn prepared_video_wait_holds_the_shared_clock_and_mixer_at_the_same_pts_boundary() {
    let mut scene = Scene::new();
    let clip =
        crate::video_asset::BrowserVideoSource::new(17, 16, 9, vec![0.0, 0.25, 0.5, 0.75], 1.0)
            .unwrap();
    let pcm =
        noon_core::AudioResource::from_interleaved(8_000, 1, vec![0.25; 8_000], 32_000).unwrap();
    noon::integration::place_video(
        &mut scene,
        std::sync::Arc::new(clip),
        Some((&pcm, 0.0)),
        0.0,
        0.0..1.0,
        2.0,
    )
    .unwrap();
    let mut source = noon::integration::scene_execution::lower(&scene).unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let segment = source.wait_segment(1.25).unwrap();
    source.advance_to(segment.end_time()).unwrap();
    let (_, reader) = source.create_prepared_prefix(segment).unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, 952);
    let [(handle, _)] = player.playback.reader.video_rasters() else {
        panic!("video missing from history")
    };
    let clip_key = crate::video_asset::readiness::clip_key(*handle);
    player.initial_delta_json().unwrap();
    assert!(player.set_video_window(&clip_key, 0, 1).unwrap());
    player.execution_wake(1_000.0).unwrap();

    let anchor = |player: &PreparedExecutionPlayer, wall| {
        serde_json::from_str::<serde_json::Value>(&player.audio_anchor_json_at(wall, wall).unwrap())
            .unwrap()
    };
    assert_eq!(anchor(&player, 1_000.0)["authorizedUntil"], 0.25);
    assert!(player.tick_delta_json(1_250.0).unwrap().is_none());
    assert!(player.prepared_playing());
    assert!(player.video_buffering());
    assert!(player
        .execution_wake(4_000.0)
        .unwrap()
        .timer_after_milliseconds()
        .is_none());
    assert_eq!(player.prepared_time(), 0.0);
    assert_eq!(anchor(&player, 4_000.0)["buffering"], true);
    assert_eq!(anchor(&player, 4_000.0)["authorizedUntil"], 0.25);

    assert!(player.set_video_window(&clip_key, 0, 3).unwrap());
    let delta = player.tick_delta_json(4_000.0).unwrap().unwrap();
    assert_eq!(decode(&delta).retained.time, 0.25);
    assert_eq!(anchor(&player, 4_000.0)["time"], 0.25);
    assert_eq!(anchor(&player, 4_000.0)["buffering"], true);
    assert_eq!(player.prepared_playback_time_at(4_125.0).unwrap(), 0.25);
    player.acknowledge_video_presentation(4_150.0).unwrap();
    assert_eq!(anchor(&player, 4_150.0)["buffering"], false);
    assert_eq!(anchor(&player, 4_150.0)["authorizedUntil"], 0.75);
    assert_eq!(player.prepared_playback_time_at(4_275.0).unwrap(), 0.375);

    player.pause_prepared_at(4_275.0).unwrap();
    player.seek_delta_json(0.0).unwrap();
    assert_eq!(anchor(&player, 4_500.0)["time"], 0.0);
    assert_eq!(anchor(&player, 4_500.0)["playing"], false);
}

#[test]
fn spatial_inspection_republishes_only_the_camera_and_survives_seek() {
    use noon_core::{SemanticCamera3D, SemanticProjection3D, SemanticRotation3D, SemanticVec3};

    let mut scene = Scene::new();
    let camera = SemanticCamera3D::new(
        SemanticVec3::new(0.0, 0.0, 10.0),
        SemanticRotation3D::IDENTITY,
        SemanticProjection3D::Perspective {
            vertical_fov_radians: 0.6,
            near: 0.1,
            far: 100.0,
        },
    )
    .unwrap();
    scene.camera_3d(camera).unwrap();
    let mut source = scene.into_execution().unwrap();
    source
        .session_mut()
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let segment = source.session_mut().wait_segment(1.0).unwrap();
    source.session_mut().advance_to(1.0).unwrap();
    let (_, reader) = source.session().create_prepared_prefix(segment).unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, 312);
    player.set_prepared_playing(false);
    player
        .set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
        .unwrap();
    assert!(player.spatial_navigation());
    let (mut mirror, _) = ready(&mut player);
    let original = mirror.camera_3d_object().unwrap().clone();
    let input =
        r#"{"view_revision":1,"orbit":[0.2,0.1],"pan":[0.03,-0.05],"zoom":-0.3,"reset":false}"#;
    assert!(!player
        .move_spatial_inspection_json(
            &input.replace(r#""view_revision":1"#, r#""view_revision":0"#)
        )
        .unwrap());
    assert!(player.move_spatial_inspection_json(input).unwrap());

    let json = player.drain_delta_json().unwrap().unwrap();
    let delta = decode(&json).retained;
    assert_eq!(delta.objects.len(), 1);
    assert_eq!(delta.objects[0].object, original.id);
    assert_eq!(delta.time, 0.0);
    receive(&mut mirror, &json);
    let inspected = mirror.camera_3d_object().unwrap().clone();
    assert_ne!(inspected.world_transform(), original.world_transform());
    assert_ne!(inspected.camera_projection(), original.camera_projection());
    assert_eq!(source.session().camera_3d().unwrap(), Some(camera));

    if let Some(json) = player.seek_delta_json(0.5).unwrap() {
        receive(&mut mirror, &json);
    }
    assert_eq!(
        mirror.camera_3d_object().unwrap().world_transform(),
        inspected.world_transform()
    );
    assert!(player
        .move_spatial_inspection_json(
            r#"{"view_revision":1,"orbit":[0,0],"pan":[0,0],"zoom":0,"reset":true}"#,
        )
        .unwrap());
    assert!(player.playback.reader.viewer_inspection_active());
    assert!(player.tick_delta_json(1_000.0).unwrap().is_none());
    let halfway = player.tick_delta_json(1_175.0).unwrap().unwrap();
    receive(&mut mirror, &halfway);
    let midway = mirror.camera_3d_object().unwrap();
    assert_ne!(midway.world_transform(), inspected.world_transform());
    assert_ne!(midway.world_transform(), original.world_transform());
    receive(
        &mut mirror,
        &player.tick_delta_json(1_400.0).unwrap().unwrap(),
    );
    assert!(!player.playback.reader.viewer_inspection_active());
    assert_eq!(
        mirror.camera_3d_object().unwrap().world_transform(),
        original.world_transform()
    );
    assert_eq!(
        mirror.camera_3d_object().unwrap().camera_projection(),
        original.camera_projection()
    );
}

#[test]
fn spatial_click_indicates_the_front_mesh_and_restores_while_paused() {
    use noon_core::{
        MeshResource, SemanticCamera3D, SemanticProjection3D, SemanticRotation3D, SemanticVec3,
        SemanticWorldTransform3D,
    };

    let mut scene = Scene::new();
    scene
        .camera_3d(
            SemanticCamera3D::new(
                SemanticVec3::new(0.0, 0.0, 10.0),
                SemanticRotation3D::IDENTITY,
                SemanticProjection3D::Orthographic {
                    height: 8.0,
                    near: 0.1,
                    far: 100.0,
                },
            )
            .unwrap(),
        )
        .unwrap();
    let triangle = MeshResource::new(
        vec![
            SemanticVec3::new(-2.0, -2.0, 0.0),
            SemanticVec3::new(2.0, -2.0, 0.0),
            SemanticVec3::new(0.0, 2.0, 0.0),
        ],
        None,
        vec![0, 1, 2],
    )
    .unwrap();
    let near = scene
        .mesh(
            noon::MeshOptions::new(triangle.clone()).with_transform(SemanticWorldTransform3D {
                translation: SemanticVec3::new(0.0, 0.0, 2.0),
                ..SemanticWorldTransform3D::IDENTITY
            }),
        )
        .unwrap();
    let far = scene.mesh(noon::MeshOptions::new(triangle)).unwrap();
    scene.add(&near).unwrap();
    scene.add(&far).unwrap();
    let target = noon_compile::semantic_execution_object_id(near.node_id());
    let mut source = scene.into_execution().unwrap();
    source
        .session_mut()
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let segment = source.session_mut().wait_segment(1.0).unwrap();
    source.session_mut().advance_to(1.0).unwrap();
    let (_, reader) = source.session().create_prepared_prefix(segment).unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, 313);
    player.set_prepared_playing(false);
    player
        .set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
        .unwrap();
    let (mut mirror, first_receipt) = ready(&mut player);
    let scroll = r#"{"view_revision":1,"orbit":[0,0],"pan":[0,0],"zoom":0.1,"reset":false}"#;
    assert!(player.move_spatial_inspection_json(scroll).unwrap());
    let presented = player.drain_delta_json().unwrap().unwrap();
    receive(&mut mirror, &presented);
    assert!(player.move_spatial_inspection_json(scroll).unwrap());
    receive(&mut mirror, &player.drain_delta_json().unwrap().unwrap());
    // Presentation can arrive after the engine has issued another camera frame.
    let receipt = WorkerPointerReceipt {
        sequence: decode(&presented).retained.sequence,
        presentation: first_receipt.presentation + 1,
        ..first_receipt
    };
    assert!(player
        .note_pointer_presentation_json(&serde_json::to_string(&receipt).unwrap())
        .unwrap());
    let sample = |phase, time, x| {
        serde_json::json!({
        "view_revision":1, "presentation":receipt,
        "samples":[{"id":2,"phase":phase,"x":x,"y":0.5,"timestamp_ms":time,"touch":false,"pan":false}],
    }).to_string()
    };
    let original = mirror
        .frame()
        .unwrap()
        .objects
        .iter()
        .find(|row| row.id == target)
        .unwrap()
        .clone();
    assert!(!player
        .move_spatial_inspection_json(&sample("press", 0, 1.0))
        .unwrap());
    assert!(player
        .move_spatial_inspection_json(&sample("release", 10, 1.0))
        .unwrap());
    assert_eq!(
        player.execution_wake(1_000.0).unwrap().cadence(),
        "animation_frame"
    );
    assert!(player.tick_delta_json(1_000.0).unwrap().is_none());
    let json = player.tick_delta_json(1_500.0).unwrap().unwrap();
    let delta = decode(&json).retained;
    assert_eq!(delta.objects.len(), 1);
    assert_eq!(delta.objects[0].object, target);
    assert_eq!(
        delta.objects[0].style.fill.unwrap().red,
        noon_core::YELLOW.red
    );
    assert_eq!(delta.objects[0].spatial.as_ref().unwrap().scale, [1.2; 3]);
    assert_eq!(delta.time, 0.0);
    receive(&mut mirror, &json);
    receive(
        &mut mirror,
        &player.tick_delta_json(2_000.0).unwrap().unwrap(),
    );
    let restored = mirror
        .frame()
        .unwrap()
        .objects
        .iter()
        .find(|row| row.id == target)
        .unwrap();
    assert_eq!(restored.style, original.style);
    assert_eq!(restored.world_transform(), original.world_transform());
    assert_eq!(player.execution_wake(2_000.0).unwrap().cadence(), "idle");
}

#[test]
fn viewer_indicates_the_selected_historical_outer_family_about_one_center() {
    let mut scene = Scene::new();
    let mut tick = scene.circle(0.5).unwrap();
    let mut other_axis = scene.circle(0.5).unwrap();
    tick.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    other_axis.set_fill(0.0, 1.0, 0.0, 1.0).unwrap();
    other_axis.set_translation(2.0, 0.0).unwrap();
    scene.add(&tick).unwrap();
    scene.add(&other_axis).unwrap();
    let mut source = scene.into_execution().unwrap();
    source
        .session_mut()
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let first = source.session_mut().wait_segment(1.0).unwrap();
    source.session_mut().advance_to(1.0).unwrap();
    let (mut publisher, reader) = source.session().create_prepared_prefix(first).unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, 320);
    player.set_prepared_playing(false);
    player.enable_default_viewer_selection();
    player
        .set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
        .unwrap();
    let (mut mirror, mut receipt) = ready(&mut player);

    let ticks = source.scene_mut().family(&[(&tick).into()]).unwrap();
    let axes = source
        .scene_mut()
        .family(&[(&ticks).into(), (&other_axis).into()])
        .unwrap();
    source.scene_mut().add_many(&[(&axes).into()]).unwrap();
    let second = source.session_mut().wait_segment(1.0).unwrap();
    source.session_mut().advance_to(2.0).unwrap();
    let batch = source
        .session()
        .publish_prepared_prefix(second, &mut publisher)
        .unwrap();
    player
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    player.preflight_append(&batch).unwrap();
    player.append(batch).unwrap();

    for (time, expected_count) in [(1.25, 2), (0.5, 1)] {
        let selected = player.seek_delta_json(time).unwrap().unwrap();
        receive(&mut mirror, &selected);
        receipt.sequence = decode(&selected).retained.sequence;
        receipt.presentation += 1;
        assert!(player
            .note_pointer_presentation_json(&serde_json::to_string(&receipt).unwrap())
            .unwrap());
        let original = mirror.frame().unwrap().clone();
        let canonical = player.playback.reader.frame().clone();
        assert!(input(&mut player, "press", receipt, 1));
        assert!(input(&mut player, "release", receipt, 1));
        let wall = 10_000.0 + 4_000.0 * f64::from(receipt.presentation as u32);
        assert!(player.tick_delta_json(wall).unwrap().is_none());
        let pulsed = player.tick_delta_json(wall + 500.0).unwrap().unwrap();
        let rows = decode(&pulsed).retained.objects;
        assert_eq!(rows.len(), expected_count);
        assert!(rows
            .iter()
            .all(|row| row.style.fill == Some(noon_core::YELLOW)));
        if expected_count == 2 {
            let mut positions = rows
                .iter()
                .map(|row| row.transform.translation.x)
                .collect::<Vec<_>>();
            positions.sort_by(f32::total_cmp);
            assert!((positions[0] + 0.2).abs() < 1e-5);
            assert!((positions[1] - 2.2).abs() < 1e-5);
        } else {
            assert_eq!(rows[0].transform.translation.x, 0.0);
        }
        receive(&mut mirror, &pulsed);
        receive(
            &mut mirror,
            &player.tick_delta_json(wall + 1000.0).unwrap().unwrap(),
        );
        assert_eq!(mirror.frame().unwrap(), &original);
        assert_eq!(player.playback.reader.frame(), &canonical);
    }
}

#[test]
fn planar_viewer_indicate_scales_an_image_without_changing_its_paint() {
    let mut scene = Scene::new();
    let picture = scene.image_rgba8(1, 1, vec![22, 100, 180, 255]).unwrap();
    scene.add(&picture).unwrap();
    let mut source = scene.into_execution().unwrap();
    source
        .session_mut()
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let segment = source.session_mut().wait_segment(1.0).unwrap();
    source.session_mut().advance_to(1.0).unwrap();
    let (_, reader) = source.session().create_prepared_prefix(segment).unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, 316);
    player.set_prepared_playing(false);
    player.enable_default_viewer_selection();
    player
        .set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
        .unwrap();
    let (mut mirror, receipt) = ready(&mut player);
    let original = mirror.frame().unwrap().clone();
    assert!(input(&mut player, "press", receipt, 1));
    assert!(input(&mut player, "release", receipt, 1));
    assert!(player.tick_delta_json(1_200.0).unwrap().is_none());
    receive(
        &mut mirror,
        &player.tick_delta_json(1_700.0).unwrap().unwrap(),
    );
    let pulsed = mirror.frame().unwrap();
    assert_eq!(pulsed.objects[0].style, original.objects[0].style);
    assert_eq!(
        pulsed.render_transform(0).scale,
        original.render_transform(0).scale * 1.2
    );
    receive(
        &mut mirror,
        &player.tick_delta_json(2_200.0).unwrap().unwrap(),
    );
    assert_eq!(mirror.frame().unwrap(), &original);
}

#[test]
fn default_planar_viewer_indicates_on_demand_and_pans_without_scene_rows() {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.75).unwrap();
    circle.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut source = scene.into_execution().unwrap();
    source
        .session_mut()
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let segment = source.session_mut().wait_segment(1.0).unwrap();
    source.session_mut().advance_to(1.0).unwrap();
    let (_, reader) = source.session().create_prepared_prefix(segment).unwrap();
    let mut player = PreparedExecutionPlayer::from_reader(reader, 314);
    player.set_prepared_playing(true);
    player
        .set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
        .unwrap();
    let (mut mirror, mut original_receipt) = ready(&mut player);
    player.enable_default_viewer_selection();
    assert_eq!(player.viewer_pointer_motion(), Some(false));
    let enabled = player.drain_delta_json().unwrap().unwrap();
    receive(&mut mirror, &enabled);
    let enabled = decode(&enabled).retained;
    assert!(enabled.objects.is_empty());
    original_receipt.sequence = enabled.sequence;
    original_receipt.presentation += 1;
    assert!(player
        .note_pointer_presentation_json(&serde_json::to_string(&original_receipt).unwrap())
        .unwrap());
    assert!(player.tick_delta_json(1_000.0).unwrap().is_none());
    assert!(player.tick_delta_json(1_100.0).unwrap().is_none());

    // Pausing advances a sleeping timeline. The pixels stay the same, but a
    // fresh displayed receipt must authorize the first click at the new time.
    player.pause_prepared_at(1_125.0).unwrap();
    let paused = player.drain_delta_json().unwrap().unwrap();
    let delta = decode(&paused).retained;
    let paused_time = 0.125;
    assert_eq!(delta.time, paused_time);
    assert!(delta.objects.is_empty());
    receive(&mut mirror, &paused);
    let receipt = WorkerPointerReceipt {
        sequence: delta.sequence,
        presentation: original_receipt.presentation + 1,
        ..original_receipt
    };
    assert!(player
        .note_pointer_presentation_json(&serde_json::to_string(&receipt).unwrap())
        .unwrap());
    assert!(input(&mut player, "press", receipt, 1));
    assert!(input(&mut player, "release", receipt, 1));
    assert!(player.tick_delta_json(1_200.0).unwrap().is_none());
    let indicated = player.tick_delta_json(1_700.0).unwrap().unwrap();
    let delta = decode(&indicated);
    assert!(delta.selection_overlay.is_none());
    assert_eq!(delta.retained.time, paused_time);
    assert_eq!(
        delta.retained.objects[0].style.fill,
        Some(noon_core::YELLOW)
    );
    assert!((delta.retained.objects[0].transform.scale.x - 1.2).abs() < 1e-5);
    receive(&mut mirror, &indicated);
    let restored = player.tick_delta_json(2_200.0).unwrap().unwrap();
    let delta = decode(&restored);
    assert_eq!(
        delta.retained.objects[0].style.fill,
        Some(noon_core::Color::rgb(0.0, 0.0, 1.0))
    );
    assert_eq!(
        delta.retained.objects[0].transform.scale,
        noon_core::Vec2::ONE
    );
    receive(&mut mirror, &restored);

    assert!(player
        .move_spatial_inspection_json(
            r#"{"view_revision":1,"orbit":[0,0],"pan":[0.25,0.125],"zoom":0,"reset":false}"#,
        )
        .unwrap());
    let panned = player.drain_delta_json().unwrap().unwrap();
    let delta = decode(&panned).retained;
    assert_eq!(delta.camera.center, noon_core::Vec2::new(-2.0, 1.0));
    assert_eq!(delta.time, paused_time);
    assert!(delta.objects.is_empty());
    receive(&mut mirror, &panned);
    assert_eq!(mirror.camera().center, delta.camera.center);
    assert!(player
        .move_spatial_inspection_json(
            r#"{"view_revision":1,"orbit":[0,0],"pan":[0,0],"zoom":0,"reset":true}"#,
        )
        .unwrap());
    assert_eq!(
        player.execution_wake(2_210.0).unwrap().cadence(),
        "animation_frame"
    );
    assert!(player.tick_delta_json(2_220.0).unwrap().is_none());
    let middle = decode(&player.tick_delta_json(2_395.0).unwrap().unwrap()).retained;
    assert_eq!(middle.time, paused_time);
    assert_eq!(middle.camera.center, noon_core::Vec2::new(-1.0, 0.5));
    let restored = decode(&player.tick_delta_json(2_600.0).unwrap().unwrap()).retained;
    assert_eq!(restored.camera, noon_core::Camera2DState::default());
    assert_eq!(player.execution_wake(2_600.0).unwrap().cadence(), "idle");
}
