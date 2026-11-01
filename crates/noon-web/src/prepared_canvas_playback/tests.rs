use super::*;
use crate::{BrowserExecutionWakeClock, BrowserExecutionWakePlan, BrowserHostWake};
use noon::{AnimationOptions, ExecutionSession, IndicateOptions, PrefixPublisher, Scene};
use noon_core::ObjectId;
use noon_runtime::{ReplayLimits, TimelineWakeState};

fn fixture() -> (
    ExecutionSession,
    PrefixPublisher,
    PreparedCanvasPlayback,
    ObjectId,
) {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.65).unwrap();
    circle.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    circle.set_stroke_width(0.0).unwrap();
    scene.add(&circle).unwrap();
    scene
        .on_click_indicate(
            &circle,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(0.5),
        )
        .unwrap();
    let mut source = scene.execution_session().unwrap();
    let object = source.execution_object_id(circle.node_id()).unwrap();
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
    (
        source,
        publisher,
        PreparedCanvasPlayback::new(reader).unwrap(),
        object,
    )
}

fn next(
    source: &mut ExecutionSession,
    publisher: &mut PrefixPublisher,
    terminal: bool,
) -> PrefixBatch {
    let segment = source.wait_segment(1.0).unwrap();
    source.advance_to(segment.end_time()).unwrap();
    if terminal {
        source.seal_replay().unwrap();
        source.finish_prepared_prefix(segment, publisher).unwrap()
    } else {
        source.publish_prepared_prefix(segment, publisher).unwrap()
    }
}

#[test]
fn prepared_canvas_frontier_buffers_without_speculative_frame_or_busy_wake() {
    let (_, _, mut playback, _) = fixture();
    playback.reader.take_renderer_publication();
    assert_eq!(
        playback.wake_state().timeline(),
        TimelineWakeState::Deadline(1.0)
    );
    let before = playback.reader.frame().clone();
    let publication = playback.reader.publication_context();
    playback.advance(1.0).unwrap();
    assert!(playback.buffering());
    assert_eq!(playback.reader.frame(), &before);
    assert_eq!(playback.reader.publication_context(), publication);
    assert!(playback.wake_state().is_quiescent());
    playback.advance(9999.0).unwrap();
    assert_eq!(playback.reader.frame(), &before);
}

#[test]
fn prepared_clock_rejects_backwards_explicit_advance_but_ignores_stale_raf_sample() {
    let (mut source, mut publisher, mut playback, _) = fixture();
    playback
        .append(next(&mut source, &mut publisher, false))
        .unwrap();
    // The timer crossed an authored deadline slightly after the next RAF's
    // timestamp was sampled. The latter must not rewind history or fail.
    playback.advance_clock_sample(1.0005).unwrap();
    assert_eq!(playback.reader.frame().time, 1.0005);
    playback.reader.take_renderer_publication();
    let before_frame = playback.reader.frame().clone();
    let before_publication = playback.reader.publication_context();
    assert!(playback.advance(1.0004349999999904).is_err());
    assert_eq!(playback.reader.frame(), &before_frame);
    playback.advance_clock_sample(1.0004349999999904).unwrap();
    assert_eq!(playback.reader.frame(), &before_frame);
    assert_eq!(playback.reader.publication_context(), before_publication);
    assert!(!playback.wake_state().frame_pending());

    // Invalid wall-derived values still fail closed without becoming a no-op
    // success. Newer samples continue normally after a stale callback.
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(playback.advance_clock_sample(invalid).is_err());
        assert_eq!(playback.reader.publication_context(), before_publication);
    }
    playback.advance_clock_sample(1.25).unwrap();
    assert_eq!(playback.reader.frame().time, 1.25);
    assert!(!playback.buffering());
}

#[test]
fn prepared_canvas_delivery_reanchors_without_charging_buffered_wall_time() {
    let (mut source, mut publisher, mut playback, _) = fixture();
    playback.reader.take_renderer_publication();
    let mut clock = BrowserExecutionWakeClock::default();
    let plan = BrowserExecutionWakePlan::from_runtime(playback.wake_state());
    assert_eq!(
        clock.directive(plan, 1000.0, 0.5).unwrap().wake(),
        BrowserHostWake::TimerAfterMilliseconds(500.0)
    );
    playback.advance(1.0).unwrap();
    let idle = BrowserExecutionWakePlan::from_runtime(playback.wake_state());
    assert_eq!(
        clock.directive(idle, 1500.0, 0.5).unwrap().wake(),
        BrowserHostWake::Idle
    );
    assert!(clock.scene_time_at(1500.0).is_none());
    let before = playback.reader.publication_context();
    playback
        .append(next(&mut source, &mut publisher, false))
        .unwrap();
    assert!(!playback.buffering());
    assert_eq!(playback.reader.publication_context(), before);
    let plan = BrowserExecutionWakePlan::from_runtime(playback.wake_state());
    clock
        .directive(plan, 999_000.0, playback.reader.frame().time)
        .unwrap();
    assert_eq!(clock.scene_time_at(999_000.0), Some(0.5));
    playback.advance(0.75).unwrap();
    assert_eq!(playback.reader.frame().time, 0.75);
}

#[test]
fn prepared_canvas_terminal_clamps_and_then_settles_at_exact_endpoint() {
    let (mut source, mut publisher, mut playback, _) = fixture();
    playback
        .append(next(&mut source, &mut publisher, true))
        .unwrap();
    drop(source);
    drop(publisher);
    playback.advance(200.0).unwrap();
    assert_eq!(playback.reader.frame().time, 2.0);
    assert_eq!(playback.reader.end_time(), Some(2.0));
    playback.reader.take_renderer_publication();
    assert!(playback.wake_state().is_quiescent());
    playback.seek(0.5).unwrap();
    playback.reader.take_renderer_publication();
    assert!(!matches!(
        playback.wake_state().timeline(),
        TimelineWakeState::Quiescent
    ));
}

#[test]
fn prepared_canvas_invalid_time_and_delivery_preserve_buffered_state() {
    let (_, _, mut playback, _) = fixture();
    playback.reader.take_renderer_publication();
    playback.advance(1.0).unwrap();
    let before = playback.reader.publication_context();
    for time in [f64::NAN, f64::INFINITY, -1.0, 0.25] {
        assert!(playback.advance(time).is_err());
        assert_eq!(playback.reader.publication_context(), before);
        assert!(playback.buffering());
    }
    assert!(playback.seek(1.0).is_err());
    assert!(playback.buffering());
    let (mut foreign, mut publisher, _, _) = fixture();
    assert!(playback
        .append(next(&mut foreign, &mut publisher, true))
        .is_err());
    assert_eq!(playback.reader.publication_context(), before);
    assert!(playback.buffering());
    playback.seek(0.5).unwrap();
    assert!(!playback.buffering());
}

#[test]
fn prepared_canvas_paused_and_buffered_effects_keep_independent_time() {
    let (_, _, mut playback, object) = fixture();
    assert!(playback.set_playing(false));
    assert!(!playback.set_playing(false));
    assert!(!playback.playing());
    let baseline = playback.reader.frame().clone();
    let prepared = playback
        .reader
        .prepare_bound_click_indicate(object)
        .unwrap()
        .unwrap();
    playback.reader.start_transient_animation(prepared).unwrap();
    playback.reader.advance_interactions(10.0).unwrap();
    playback.advance(50.0).unwrap();
    playback.reader.advance_interactions(10.25).unwrap();
    assert_ne!(playback.reader.frame(), &baseline);
    assert_eq!(playback.reader.frame().time, 0.5);
    playback.reader.advance_interactions(10.5).unwrap();
    assert_eq!(playback.reader.frame(), &baseline);
    playback.set_playing(true);
    playback.advance(1.0).unwrap();
    let prepared = playback
        .reader
        .prepare_bound_click_indicate(object)
        .unwrap()
        .unwrap();
    playback.reader.start_transient_animation(prepared).unwrap();
    playback.reader.advance_interactions(20.0).unwrap();
    playback.reader.advance_interactions(20.25).unwrap();
    assert_eq!(playback.reader.frame().time, 0.5);
    assert!(playback.buffering());
    playback.reader.advance_interactions(20.5).unwrap();
    assert_eq!(playback.reader.frame(), &baseline);
}

#[test]
fn prepared_canvas_append_does_not_resume_user_pause_or_dirty_the_frame() {
    let (mut source, mut publisher, mut playback, _) = fixture();
    playback.set_playing(false);
    playback.reader.take_renderer_publication();
    let before = playback.reader.publication_context();
    playback
        .append(next(&mut source, &mut publisher, true))
        .unwrap();
    assert!(!playback.playing());
    assert!(playback.wake_state().is_quiescent());
    assert_eq!(playback.reader.publication_context(), before);
    playback.advance(2.0).unwrap();
    assert_eq!(playback.reader.frame().time, 0.5);
}

#[test]
fn prepared_canvas_renderer_query_uses_its_own_frame_without_consuming_dirtiness() {
    let (_, _, mut playback, _) = fixture();
    let before = playback.reader.publication_context();
    let dirty = playback.reader.wake_state().frame_pending();
    let query = playback
        .reader
        .query_renderer_viewports(&[noon_core::Rect::new(
            noon_core::Vec2::new(-2.0, -2.0),
            noon_core::Vec2::new(2.0, 2.0),
        )]);
    assert!(playback.reader.viewport_query_is_current(&query));
    assert!(!query.object_indices().is_empty());
    assert_eq!(playback.reader.publication_context(), before);
    assert_eq!(playback.reader.wake_state().frame_pending(), dirty);
}

#[test]
fn prepared_canvas_demand_is_reader_local_validated_and_read_only() {
    let (_, _, mut playback, _) = fixture();
    playback.set_playing(false);
    playback.reader.take_renderer_publication();
    let before = playback.reader.frame().clone();
    let publication = playback.reader.publication_context();
    assert_eq!(playback.preparation_target(1.0).unwrap(), 1.5);
    for ahead in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(playback.preparation_target(ahead).is_err());
    }
    assert_eq!(playback.reader.frame(), &before);
    assert_eq!(playback.reader.publication_context(), publication);
    assert!(!playback.playing());
    playback.seek(0.25).unwrap();
    assert_eq!(playback.preparation_target(1.0).unwrap(), 1.25);
}

#[test]
fn prepared_canvas_interpreter_demand_sleeps_refills_and_ignores_rewind_history() {
    let (mut source, mut publisher, mut playback, _) = fixture();
    playback.set_playing(false);
    playback.reader.take_renderer_publication();
    let before = playback.reader.frame().clone();
    let publication = playback.reader.publication_context();
    assert!(!playback.needs_preparation(0.5).unwrap());
    assert!(playback.needs_preparation(1.0).unwrap());
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(playback.needs_preparation(invalid).is_err());
    }
    assert_eq!(playback.reader.frame(), &before);
    assert_eq!(playback.reader.publication_context(), publication);
    assert!(!playback.reader.wake_state().frame_pending());
    playback
        .append(next(&mut source, &mut publisher, false))
        .unwrap();
    assert!(!playback.needs_preparation(1.0).unwrap());
    playback.seek(0.25).unwrap();
    assert!(!playback.needs_preparation(1.0).unwrap());
    playback.seek(1.5).unwrap();
    assert!(playback.needs_preparation(1.0).unwrap());
}

#[test]
fn prepared_canvas_interpreter_demand_refills_open_wait_but_not_a_terminal_reader() {
    let (mut source, mut publisher, mut playback, _) = fixture();
    assert!(!playback.needs_preparation(0.125).unwrap());
    playback.advance(1.0).unwrap();
    assert!(playback.buffering());
    assert!(playback.needs_preparation(0.125).unwrap());
    playback
        .append(next(&mut source, &mut publisher, true))
        .unwrap();
    assert!(!playback.needs_preparation(100.0).unwrap());
    playback.seek(0.25).unwrap();
    assert!(!playback.needs_preparation(100.0).unwrap());
    assert!(playback.needs_preparation(f64::NAN).is_err());
}

// Direct canvases still lack output; the worker has an explicit bounded output
// resource/anchor boundary and must bootstrap in silence even for delayed clips.
fn audio_reader(start_time: f64) -> (PrefixReader, std::sync::Weak<[f32]>) {
    let mut scene = Scene::new();
    let samples: std::sync::Arc<[f32]> = vec![0.125; 8_000].into();
    let weak = std::sync::Arc::downgrade(&samples);
    scene
        .audio(
            noon::AudioResource::from_interleaved(8_000, 1, samples, 32_000).unwrap(),
            noon::AudioClipOptions {
                start_time,
                duration: Some(4.0),
                looped: true,
                ..Default::default()
            },
        )
        .unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let segment = source.wait_segment(1.0).unwrap();
    source.advance_to(segment.end_time()).unwrap();
    let (_, mut reader) = source.create_prepared_prefix(segment).unwrap();
    reader.seek(0.5).unwrap();
    assert!(reader.has_audio());
    (reader, weak)
}

#[test]
fn prepared_canvas_rejects_audio_before_installation_even_before_clip_onset() {
    for start in [0.0, 2.0] {
        let (reader, pcm) = audio_reader(start);
        assert_eq!(
            reader.audio_playback().unwrap().count(),
            usize::from(start < 1.0)
        );
        assert!(matches!(
            PreparedCanvasPlayback::new(reader),
            Err("prepared audio output requires displayed-time synchronization (#2358)")
        ));
        assert!(
            pcm.upgrade().is_none(),
            "rejected ownership releases reader PCM"
        );
    }
}

#[test]
fn prepared_worker_audio_bootstraps_paused_and_retains_delayed_resources() {
    for start in [0.0, 2.0] {
        let (reader, pcm) = audio_reader(start);
        let mut player = crate::PreparedExecutionPlayer::from_reader(reader, 123).unwrap();
        assert!(player.has_audio());
        assert!(!player.prepared_playing());
        let before = player.prepared_time();
        let resources = crate::worker_audio::WorkerAudioResources::decode(
            &player.audio_resource_bundle_bytes().unwrap(),
        )
        .unwrap();
        // Delayed resources exist at bootstrap, but no output beyond the
        // delivered first-second interval can be constructed from its anchor.
        assert_eq!(resources.at_time(0.0).unwrap().len(), 1);
        assert_eq!(
            resources.at_time_before(before, 1.0).unwrap().len(),
            usize::from(start < 1.0)
        );
        let anchor: serde_json::Value =
            serde_json::from_str(&player.audio_anchor_json(1000.0, 1000.0).unwrap()).unwrap();
        assert_eq!(anchor["playing"], false);
        assert_eq!(anchor["availableUntil"], 1.0);
        assert_eq!(player.prepared_time(), before);
        assert!(pcm.upgrade().is_some());
        drop(player);
        assert!(pcm.upgrade().is_none());
    }
}
