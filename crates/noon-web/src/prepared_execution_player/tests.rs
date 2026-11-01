use super::*;
use crate::worker_pointer_presentation::WorkerPointerReceipt;
use crate::{InstalledRetainedExecutionMirror, RetainedTransportApplyOutcome};
use noon::{AnimationOptions, ExecutionSession, IndicateOptions, Mobject, PrefixPublisher, Scene};
use noon_runtime::ReplayLimits;

struct Fixture {
    scene: Scene,
    target: Mobject,
    source: ExecutionSession,
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
        let mut source = scene.execution_session().unwrap();
        source
            .begin_replay_retention(ReplayLimits {
                revisions: 128,
                payloads: 2048,
            })
            .unwrap();
        let segment = source.wait_segment(1.0).unwrap();
        source.advance_to(1.0).unwrap();
        let (publisher, mut reader) = source.create_prepared_prefix(segment).unwrap();
        reader.seek(0.5).unwrap();
        let mut player = PreparedExecutionPlayer::from_reader(reader, session).unwrap();
        player.set_prepared_playing(false);
        player
            .set_browser_pointer_view_json(r#"{"revision":1,"width":800,"height":400}"#)
            .unwrap();
        Self {
            scene,
            target,
            source,
            publisher,
            player,
        }
    }

    fn next(&mut self, terminal: bool) -> PrefixBatch {
        self.scene
            .live(&mut self.source)
            .shift(&self.target, 1.0, 0.0)
            .unwrap();
        let segment = self.source.wait_segment(1.0).unwrap();
        self.source.advance_to(segment.end_time()).unwrap();
        if terminal {
            self.source.seal_replay().unwrap();
            self.source
                .finish_prepared_prefix(segment, &mut self.publisher)
                .unwrap()
        } else {
            self.source
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

#[test]
fn prepared_frame_diagnostic_reads_the_selected_history_without_publication() {
    let mut fixture = Fixture::new(17);
    let (_, _) = ready(&mut fixture.player);
    let later = fixture.next(true);
    fixture.deliver(later);
    fixture.player.seek_delta_json(1.5).unwrap();
    let later: serde_json::Value =
        serde_json::from_str(&fixture.player.debug_frame_json()).unwrap();
    assert_eq!(later["time"], 1.5);
    assert_eq!(later["objects"][0]["center"][0], 1.0);

    fixture.player.seek_delta_json(0.5).unwrap();
    let before = fixture.player.playback.reader.publication_context();
    let frontier = fixture.player.playback.reader.frontier();
    let json = fixture.player.debug_frame_json();
    let frame: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(frame["time"], 0.5);
    assert_eq!(frame["objects"][0]["center"][0], 0.0);
    assert_eq!(fixture.player.debug_frame_json(), json);
    assert_eq!(fixture.player.playback.reader.publication_context(), before);
    assert_eq!(fixture.player.playback.reader.frontier(), frontier);
    assert!(!fixture.player.playback.playing());
    assert!(fixture
        .player
        .drain_delta_transport_json()
        .unwrap()
        .is_none());
}

fn receive(mirror: &mut InstalledRetainedExecutionMirror, json: &str) {
    let (outcome, _) = mirror.apply_json(json).unwrap();
    assert_ne!(outcome, RetainedTransportApplyOutcome::DroppedStale);
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
    drop((f.scene, f.source, f.target, f.publisher));
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
    assert!(f.player.tick_delta_json(f64::INFINITY).is_err());
    assert_eq!(f.player.clock, clock);
    assert_eq!(f.player.playback.reader.publication_context(), publication);
    assert!(f.player.tick_delta_json(1_500.0).unwrap().is_none());
    assert!(f.player.playback.buffering());
    assert_eq!(f.player.playback.reader.frame(), &frame);
    assert_eq!(f.player.prepared_playback_time_at(90_000.0).unwrap(), 1.0);
    assert_eq!(f.player.execution_wake(90_000.0).unwrap().cadence(), "idle");
    assert!(f.player.needs_preparation(0.125).unwrap());
    let batch = f.next(false);
    f.deliver(batch);
    assert_eq!(
        f.player
            .execution_wake(100_000.0)
            .unwrap()
            .timer_after_milliseconds(),
        Some(0.0)
    );
    assert!(f.player.prepared_needs_frame());
    assert_eq!(
        f.player.playback.reader.frame(),
        &frame,
        "append is not a selection"
    );
    assert!(f.player.tick_delta_json(100_250.0).unwrap().is_some());
    assert_eq!(f.player.prepared_time(), 1.0);
    assert!(!f.player.prepared_needs_frame());
    assert!(
        f.player.clock.scene_time_at(200_000.0).is_none(),
        "the catch-up tick is not presentation"
    );
    // A separate picture-qualified wake starts at the held boundary, not at
    // the old 0.5-second picture and not at a time charged for the long stall.
    f.player.execution_wake(200_000.0).unwrap();
    assert_eq!(f.player.prepared_playback_time_at(200_250.0).unwrap(), 1.25);
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
fn prepared_worker_new_image_resources_arrive_only_when_history_is_selected() {
    let mut f = Fixture::new(81);
    let (mut mirror, _) = ready(&mut f.player);
    let baseline = mirror.frame().unwrap().clone();
    let image = f
        .scene
        .live(&mut f.source)
        .create_image(noon::ImageMobjectOptions::rgba8(2, 2, [230, 40, 10, 255].repeat(4)).unwrap())
        .unwrap();
    f.scene.live(&mut f.source).add(&image).unwrap();
    let segment = f.source.wait_segment(1.0).unwrap();
    f.source.advance_to(2.0).unwrap();
    let batch = f
        .source
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
    assert_eq!(f.player.prepared_playback_time_at(90_000.0).unwrap(), 1.0);
    let batch = f.next(false);
    f.deliver(batch);
    assert!(!f.player.prepared_buffering());
    assert!(!f.player.prepared_playing());
    f.player.set_prepared_playing(true);
    assert_eq!(f.player.prepared_playback_time_at(100_000.0).unwrap(), 1.0);
    assert_eq!(
        f.player
            .execution_wake(100_000.0)
            .unwrap()
            .timer_after_milliseconds(),
        Some(0.0)
    );
    assert_eq!(f.player.prepared_playback_time_at(100_250.0).unwrap(), 1.0);
    f.player.tick_delta_json(100_250.0).unwrap();
    assert_eq!(f.player.prepared_time(), 1.0);
    f.player.execution_wake(200_000.0).unwrap();
    assert_eq!(f.player.prepared_playback_time_at(200_250.0).unwrap(), 1.25);
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
        let mut live = f.scene.live(&mut f.source);
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
