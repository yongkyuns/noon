use super::*;
use crate::worker_audio::WorkerAudioResources;
use noon::{
    AudioClipOptions, AudioResource, ExecutionSession, PrefixBatch, PrefixPublisher, Scene,
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
