use super::*;
use crate::worker_audio::WorkerAudioResources;
use noon::{
    AnimationOptions, AudioClipOptions, AudioResource, ExecutionSession, PrefixBatch,
    PrefixPublisher, RateFunction, Scene,
};
use noon_runtime::ReplayLimits;
use serde_json::Value;

struct Fixture {
    source: ExecutionSession,
    publisher: PrefixPublisher,
    player: PreparedExecutionPlayer,
}

impl Fixture {
    fn new() -> Self {
        Self::with_animation(false)
    }

    fn with_animation(animated: bool) -> Self {
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
        let segment = if animated {
            let mut live = scene.live(&mut source);
            let target = live.target_editor(&circle).unwrap();
            live.set_translation(&target, 2.0, 0.0).unwrap();
            live.declare_and_activate_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap()
        } else {
            source.wait_segment(1.0).unwrap()
        };
        source.advance_to(1.0).unwrap();
        if animated {
            scene.live(&mut source).complete_segment(segment).unwrap();
        }
        let (publisher, mut reader) = source.create_prepared_prefix(segment).unwrap();
        reader.seek(0.5).unwrap();
        let player = PreparedExecutionPlayer::from_reader(reader, 123).unwrap();
        Self {
            source,
            publisher,
            player,
        }
    }

    fn next(&mut self, terminal: bool) -> PrefixBatch {
        let segment = self.source.wait_segment(2.0).unwrap();
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

    fn anchor(&mut self, wall: f64) -> Value {
        serde_json::from_str(
            &self
                .player
                .audio_anchor_json(1_000_000.0 + wall, wall)
                .unwrap(),
        )
        .unwrap()
    }
}

#[test]
fn animated_audio_anchor_pairs_delayed_observation_with_the_admitted_clock() {
    let mut f = Fixture::with_animation(true);
    f.player.initial_delta_json().unwrap();
    let observe = |f: &mut Fixture, wall| {
        let frame = f.player.playback.reader.frame().clone();
        let publication = f.player.playback.reader.publication_context();
        let frontier = f.player.prepared_frontier();
        let clock = f.player.clock;
        let source_frame = f.source.frame().clone();
        let anchor = f.anchor(wall);
        assert_eq!(f.player.playback.reader.frame(), &frame);
        assert_eq!(f.player.playback.reader.publication_context(), publication);
        assert_eq!(f.player.prepared_frontier(), frontier);
        assert_eq!(f.player.clock, clock);
        assert_eq!(f.source.frame(), &source_frame);
        assert!(f.player.drain_delta_json().unwrap().is_none());
        anchor
    };

    let cold = observe(&mut f, 900.0);
    assert_eq!(cold["time"], 0.5);
    assert_eq!(cold["playing"], false);
    f.player.set_prepared_playing(true);
    assert_eq!(observe(&mut f, 1_000.0)["time"], 0.5);
    assert_eq!(f.player.clock.scene_time_at(1_000.0), None);
    f.player.execution_wake(1_000.0).unwrap();
    assert_eq!(
        crate::BrowserExecutionWakePlan::from_runtime(f.player.playback.wake_state()).cadence(),
        crate::BrowserExecutionCadence::AnimationFrame
    );

    f.player.tick_delta_json(1_100.0).unwrap();
    let delayed = observe(&mut f, 1_180.0);
    assert!((delayed["time"].as_f64().unwrap() - 0.68).abs() < 1e-12);
    assert_eq!(delayed["playing"], true);
    assert_eq!(f.player.prepared_time(), 0.6);
    assert_eq!(f.player.prepared_playback_time_at(1_180.0).unwrap(), 0.6);

    f.player.tick_delta_json(1_380.0).unwrap();
    let next = observe(&mut f, 1_380.0);
    assert!((next["time"].as_f64().unwrap() - 0.88).abs() < 1e-12);
    let authored_delta_ms =
        (next["time"].as_f64().unwrap() - delayed["time"].as_f64().unwrap()) * 1_000.0;
    let wall_delta_ms =
        next["sampledEpochMs"].as_f64().unwrap() - delayed["sampledEpochMs"].as_f64().unwrap();
    assert!((authored_delta_ms - wall_delta_ms).abs() < 1e-9);

    let bounded = observe(&mut f, 2_000.0);
    assert_eq!(bounded["time"], 1.0);
    assert_eq!(bounded["availableUntil"], 1.0);
    assert_eq!(bounded["playing"], false);
    assert!(!f.player.prepared_buffering());
    assert!((f.player.prepared_time() - 0.88).abs() < 1e-12);
    f.player.pause_prepared_at(2_000.0).unwrap();
    let paused = observe(&mut f, 5_000.0);
    assert!((paused["time"].as_f64().unwrap() - 0.88).abs() < 1e-12);
    assert_eq!(paused["playing"], false);
}

#[test]
fn static_deadline_audio_anchor_pairs_delayed_observation_with_the_admitted_clock() {
    let mut scene = Scene::new();
    scene
        .audio(
            AudioResource::from_interleaved(8_000, 1, vec![0.125; 16_000], 64_000).unwrap(),
            AudioClipOptions {
                duration: Some(8.0),
                looped: true,
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
    let wait = source.wait_segment(1.0).unwrap();
    source.advance_to(wait.end_time()).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(wait).unwrap();
    reader.seek(0.5).unwrap();
    let animation = {
        let mut live = scene.live(&mut source);
        let target = live.target_editor(&circle).unwrap();
        live.set_translation(&target, 2.0, 0.0).unwrap();
        live.declare_and_activate_transform_to(
            &circle,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap()
    };
    source.advance_to(animation.end_time()).unwrap();
    scene.live(&mut source).complete_segment(animation).unwrap();
    let animation_prefix = source
        .publish_prepared_prefix(animation, &mut publisher)
        .unwrap();
    let player = PreparedExecutionPlayer::from_reader(reader, 123).unwrap();
    let mut f = Fixture {
        source,
        publisher,
        player,
    };
    f.player.initial_delta_json().unwrap();
    f.player.append(animation_prefix).unwrap();
    assert_eq!(f.player.prepared_frontier(), 2.0);
    let observe = |f: &mut Fixture, wall| {
        let frame = f.player.playback.reader.frame().clone();
        let publication = f.player.playback.reader.publication_context();
        let frontier = f.player.prepared_frontier();
        let clock = f.player.clock;
        let wake = f.player.playback.wake_state();
        let held_frontier = f.player.held_frontier;
        let buffering = f.player.prepared_buffering();
        let source_frame = f.source.frame().clone();
        let anchor = f.anchor(wall);
        assert_eq!(f.player.playback.reader.frame(), &frame);
        assert_eq!(f.player.playback.reader.publication_context(), publication);
        assert_eq!(f.player.prepared_frontier(), frontier);
        assert_eq!(f.player.clock, clock);
        assert_eq!(f.player.playback.wake_state(), wake);
        assert_eq!(f.player.held_frontier, held_frontier);
        assert_eq!(f.player.prepared_buffering(), buffering);
        assert_eq!(f.source.frame(), &source_frame);
        assert!(f.player.drain_delta_json().unwrap().is_none());
        anchor
    };

    f.player.set_prepared_playing(true);
    f.player.execution_wake(1_000.0).unwrap();
    f.player.tick_delta_json(1_490.0).unwrap();
    assert_eq!(
        crate::BrowserExecutionWakePlan::from_runtime(f.player.playback.wake_state()).cadence(),
        crate::BrowserExecutionCadence::TimerAtSceneTime(1.0)
    );
    assert_eq!(f.player.prepared_time(), 0.5);
    assert_eq!(f.player.prepared_playback_time_at(1_580.0).unwrap(), 1.0);
    let delayed = observe(&mut f, 1_580.0);
    assert!((delayed["time"].as_f64().unwrap() - 1.08).abs() < 1e-12);
    assert_eq!(delayed["playing"], true);

    f.player.tick_delta_json(1_680.0).unwrap();
    assert_eq!(
        crate::BrowserExecutionWakePlan::from_runtime(f.player.playback.wake_state()).cadence(),
        crate::BrowserExecutionCadence::AnimationFrame
    );
    let next = observe(&mut f, 1_680.0);
    assert!((next["time"].as_f64().unwrap() - 1.18).abs() < 1e-12);
    let authored_delta_ms =
        (next["time"].as_f64().unwrap() - delayed["time"].as_f64().unwrap()) * 1_000.0;
    let wall_delta_ms =
        next["sampledEpochMs"].as_f64().unwrap() - delayed["sampledEpochMs"].as_f64().unwrap();
    assert!((authored_delta_ms - wall_delta_ms).abs() < 1e-9);

    let bounded = observe(&mut f, 3_000.0);
    assert_eq!(bounded["time"], 2.0);
    assert_eq!(bounded["availableUntil"], 2.0);
    assert_eq!(bounded["playing"], false);
    assert!(!f.player.prepared_buffering());
    assert!((f.player.prepared_time() - 1.18).abs() < 1e-12);
}

#[test]
fn audio_resource_bootstrap_and_undelivered_completion_are_silent_and_reader_local() {
    let mut f = Fixture::new();
    f.player.initial_delta_json().unwrap();
    let frame = f.player.playback.reader.frame().clone();
    let publication = f.player.playback.reader.publication_context();
    let resources =
        WorkerAudioResources::decode(&f.player.audio_resource_bundle_bytes().unwrap()).unwrap();
    let pending = f.next(true);
    let first = f.anchor(1000.0);
    assert_eq!(first["playing"], false);
    assert_eq!(first["time"], 0.5);
    assert_eq!(first["availableUntil"], 1.0);
    assert_eq!(first["audioSequence"], 1);
    assert_eq!(f.player.prepared_end_time(), None);
    assert_eq!(f.player.playback.reader.frame(), &frame);
    assert_eq!(f.player.playback.reader.publication_context(), publication);
    assert!(f.player.drain_delta_json().unwrap().is_none());
    let before = resources.at_time_before(0.5, 1.0).unwrap();
    assert_eq!(before[0].duration, 0.5);
    assert_eq!(before[0].source_offset, 1.0);
    assert_eq!(before[0].gain_points.points().last().unwrap().gain, 0.25);

    f.player.append(pending).unwrap();
    let adopted = f.anchor(50_000.0);
    assert_eq!(adopted["playing"], false);
    assert_eq!(adopted["time"], 0.5);
    assert_eq!(adopted["availableUntil"], 3.0);
    assert_eq!(adopted["audioSequence"], 2);
    assert_eq!(f.player.prepared_end_time(), Some(3.0));
    assert!(f.player.drain_delta_json().unwrap().is_none());
    let after = resources.at_time_before(0.5, 3.0).unwrap();
    assert_eq!(after[0].duration, 2.5);
    assert_eq!(after[0].source_offset, before[0].source_offset);
    assert_eq!(after[0].gain_points.points().last().unwrap().gain, 0.5);
}

#[test]
fn audio_anchor_uses_the_reader_clock_and_preserves_pause_buffer_seek_and_terminal() {
    let mut f = Fixture::new();
    f.player.initial_delta_json().unwrap();
    f.player.set_prepared_playing(true);
    f.player.execution_wake(1000.0).unwrap();
    let playing = f.anchor(1250.0);
    assert_eq!(playing["time"], 0.75);
    assert_eq!(playing["playing"], true);
    assert_eq!(playing["availableUntil"], 1.0);
    f.player.pause_prepared_at(1250.0).unwrap();
    let paused = f.anchor(50_000.0);
    assert_eq!(paused["time"], 0.75);
    assert_eq!(paused["playing"], false);
    f.player.set_prepared_playing(true);
    f.player.execution_wake(100_000.0).unwrap();
    assert_eq!(f.anchor(100_125.0)["time"], 0.875);
    f.player.tick_delta_json(100_300.0).unwrap();
    assert!(f.player.prepared_buffering());
    let buffered = f.anchor(900_000.0);
    assert_eq!(buffered["time"], 1.0);
    assert_eq!(buffered["playing"], false);

    let terminal = f.next(true);
    f.player.append(terminal).unwrap();
    assert!(f.player.prepared_needs_frame());
    f.player.execution_wake(1_000_000.0).unwrap();
    let held = f.anchor(1_000_125.0);
    assert_eq!(held["time"], 1.0);
    assert_eq!(
        held["playing"], false,
        "a delivered boundary is not its picture"
    );
    f.player.tick_delta_json(1_000_125.0).unwrap();
    assert!(!f.player.prepared_needs_frame());
    assert_eq!(f.player.prepared_time(), 1.0);
    // The endpoint separately acknowledges the catch-up picture, then admits
    // this ordinary wake. Renderer latency is excluded along with buffering.
    f.player.execution_wake(1_100_000.0).unwrap();
    let resumed = f.anchor(1_100_125.0);
    assert_eq!(resumed["time"], 1.125);
    assert_eq!(resumed["playing"], true);
    assert_eq!(resumed["availableUntil"], 3.0);
    f.player.seek_delta_json(0.0).unwrap();
    assert_eq!(f.anchor(2_000_000.0)["time"], 0.0);
    f.player.execution_wake(2_000_000.0).unwrap();
    f.player.tick_delta_json(2_003_000.0).unwrap();
    let ended = f.anchor(2_003_000.0);
    assert_eq!(ended["time"], 3.0);
    assert_eq!(ended["playing"], false);
    f.player.seek_delta_json(0.0).unwrap();
    assert_eq!(f.anchor(3_000_000.0)["playing"], true);
}

#[test]
fn rejected_audio_anchor_does_not_consume_sequence_or_change_the_reader() {
    let mut f = Fixture::new();
    let publication = f.player.playback.reader.publication_context();
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(f.player.audio_anchor_json(invalid, 1000.0).is_err());
        assert!(f.player.audio_anchor_json(1000.0, invalid).is_err());
    }
    assert_eq!(f.anchor(1000.0)["audioSequence"], 1);
    assert_eq!(f.player.playback.reader.publication_context(), publication);
    f.player.audio_sequence = u32::MAX;
    assert!(f.player.audio_anchor_json(1000.0, 1000.0).is_err());
    assert_eq!(f.player.audio_sequence, u32::MAX);
    assert_eq!(f.player.playback.reader.publication_context(), publication);
}
