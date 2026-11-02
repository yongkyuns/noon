//! The canonical context is the same owner used by ordinary Python play/wait.
//! These tests qualify source admission and plans, not browser device output.
use super::*;
use crate::canonical_authoring_scene::OrdinaryCompositionChild;
use noon::{AudioClipOptions, AudioResource};
use std::sync::{Arc, Weak};

fn soundtrack(start_time: f64) -> (CanonicalAuthoringScene, Weak<[f32]>) {
    let mut context = CanonicalAuthoringScene::default();
    let samples: Arc<[f32]> = vec![0.125; 32_000].into();
    let weak = Arc::downgrade(&samples);
    let source = AudioResource::from_interleaved(8_000, 1, samples, 128_000).unwrap();
    context
        .audio(
            &source,
            AudioClipOptions {
                start_time,
                duration: Some(3.0),
                gain: 0.5,
                fade_in: 0.5,
                fade_out: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
    (context, weak)
}

#[test]
fn ordinary_small_discovery_create_wait_prepares_silently_and_exports_same_worker_reader() {
    // Exact canonical pair of audio_ordinary_play.py, using the same score
    // resource and ordinary authoring calls as the Python facade.
    let mut context = CanonicalAuthoringScene::default();
    let resource = crate::authoring_audio::small_discovery().unwrap();
    context
        .audio(
            &resource,
            AudioClipOptions {
                duration: Some(3.0),
                gain: 0.5,
                fade_in: 0.5,
                fade_out: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
    context.allow_prepared_audio_source().unwrap();
    let circle = context.scene.circle(0.5).unwrap();
    context
        .begin_ordinary_mixed_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[OrdinaryCompositionChild::Create {
                entering_id: Some(ObjectId::new(0)),
                target: circle,
                options: AnimationOptions::new().run_time(1.0),
            }],
            AnimationOptions::new(),
            AnimationOptions::new().run_time(1.0),
        )
        .unwrap();
    context.prepare_live_segment().unwrap();
    let source_identity = context.active_live_player().unwrap().ownership_identity();
    let mut player =
        crate::PreparedExecutionPlayer::from_completed_source(&mut context, 99).unwrap();
    player.initial_delta_json().unwrap();
    let options = serde_json::json!({
        "frame_rate": { "numerator": 30_000, "denominator": 1_001 },
        "start_frame": 0, "stop": { "kind": "source_end" }, "max_frames": 100,
        "max_transitions_per_sample": 64, "final_hold_seconds": 0.0,
    })
    .to_string();
    player.prepare_history_export_json(&options).unwrap();
    context.begin_ordinary_wait(2.0).unwrap();
    player.prepare_interpreter_segment(&mut context).unwrap();
    player.deliver_source_prefix(0.0).unwrap();
    assert_eq!(
        context.active_live_player().unwrap().ownership_identity(),
        source_identity
    );
    player.finish_interpreter_source(&mut context).unwrap();
    player.deliver_source_prefix(0.0).unwrap();
    drop(context);
    assert_eq!(player.prepared_time(), 0.0);
    assert_eq!(player.prepared_end_time(), Some(3.0));
    assert!(!player.prepared_playing());
    assert!(player.drain_delta_json().unwrap().is_none());
    let anchor: serde_json::Value =
        serde_json::from_str(&player.audio_anchor_json(1_000.0, 0.0).unwrap()).unwrap();
    assert_eq!(anchor["playing"], false);
    assert_eq!(anchor["time"], 0.0);
    let plan: serde_json::Value =
        serde_json::from_str(&player.begin_finite_export_json(&options).unwrap()).unwrap();
    assert_eq!(plan["sampling"]["frames"], 90);
    let runtime = plan["source"]["runtime_identity"].clone();
    let mut frames = 0;
    loop {
        let sample: serde_json::Value =
            serde_json::from_str(&player.observe_finite_export_json().unwrap()).unwrap();
        assert_eq!(sample["observation"]["runtime_identity"], runtime);
        frames += usize::from(!sample["frame"].is_null());
        player.drain_delta_json().unwrap();
        let next: serde_json::Value = serde_json::from_str(
            &player
                .acknowledge_finite_export_json(&sample["observation"].to_string())
                .unwrap(),
        )
        .unwrap();
        if next["kind"] == "complete" {
            break;
        }
    }
    assert_eq!(frames, 90);
    assert_eq!(player.prepared_time(), 3.0);
    assert!(!player.prepared_playing());
    assert!(player
        .mix_finite_export_audio_block(24_000, 512)
        .unwrap()
        .iter()
        .any(|sample| *sample != 0.0));
    assert!(player
        .mix_finite_export_audio_block(144_000, 144)
        .unwrap()
        .iter()
        .all(|sample| *sample == 0.0));
    player.cancel_finite_export();
    assert!(!player.finite_export_active());
}

#[test]
fn canonical_audio_create_wait_prepares_with_independent_bounded_reader() {
    let (mut context, pcm) = soundtrack(0.0);
    context.allow_prepared_audio_source().unwrap();
    let circle = context.scene.circle(0.5).unwrap();
    context
        .begin_ordinary_mixed_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[OrdinaryCompositionChild::Create {
                entering_id: Some(ObjectId::new(0)),
                target: circle.clone(),
                options: AnimationOptions::new().run_time(1.0),
            }],
            AnimationOptions::new(),
            AnimationOptions::new().run_time(1.0),
        )
        .unwrap();
    assert!(context.source_preparation == SourcePreparation::Prepared);
    assert_eq!(context.active_live_player().unwrap().time(), 0.0);
    assert!(context.create_prepared_prefix().is_err());
    assert_eq!(context.prepare_live_segment().unwrap(), 1.0);
    let identity = context.active_live_player().unwrap().ownership_identity();
    let (mut publisher, mut reader) = context.create_prepared_prefix().unwrap();
    reader.seek(0.25).unwrap();
    let frame = reader.frame().clone();
    let sound = reader.audio_playback().unwrap().next().unwrap();
    assert_eq!(sound.duration, 0.75);
    assert_eq!(sound.source_offset, 0.25);
    assert_eq!(sound.gain_points.points()[0].gain, 0.25);
    assert_eq!(sound.gain_points.points().last().unwrap().gain, 0.5);
    assert_eq!(
        sound.pcm.samples().as_ptr(),
        pcm.upgrade().unwrap().as_ptr()
    );

    // An ordinary next segment uses the existing source player and receipt.
    context.begin_ordinary_wait(2.0).unwrap();
    assert_eq!(context.prepare_live_segment().unwrap(), 3.0);
    assert_eq!(
        context.active_live_player().unwrap().ownership_identity(),
        identity
    );
    assert!(context.prepare_live_segment().is_err());
    assert_eq!(reader.frontier(), 1.0);
    assert_eq!(
        reader.audio_playback().unwrap().next().unwrap().duration,
        0.75
    );
    deliver(
        &mut reader,
        context.publish_prepared_prefix(&mut publisher).unwrap(),
    );
    assert_eq!(reader.frame(), &frame);
    assert_eq!(
        reader.audio_playback().unwrap().next().unwrap().duration,
        2.75
    );
    assert!(
        reader.seek(3.0).is_err(),
        "open endpoint is not yet readable"
    );
    context.seal_prepared_source().unwrap();
    deliver(
        &mut reader,
        context.finish_prepared_prefix(&mut publisher).unwrap(),
    );
    reader.seek(3.0).unwrap();
    assert!(reader.audio_playback().unwrap().next().is_none());
    reader.seek(0.25).unwrap();
    assert_eq!(reader.frame(), &frame);
    drop(publisher);
    drop(context);
    drop(circle);
    assert!(pcm.upgrade().is_some(), "the displayed reader retains PCM");
    drop(reader);
    assert!(pcm.upgrade().is_none());
}

#[test]
fn canonical_headless_soundtrack_cannot_escape_as_realtime_output() {
    let (mut context, _) = soundtrack(2.0);
    context.select_prepared_source().unwrap();
    assert!(context.take_execution_player(5.0, 20).is_err());
    assert!(context.player_ownership.is_unstarted());
    context.begin_ordinary_wait(1.0).unwrap();
    context.prepare_live_segment().unwrap();
    let before = context.active_live_player().unwrap().debug_frame_json();
    let identity = context.active_live_player().unwrap().ownership_identity();
    let player = context.active_live_player().unwrap();
    assert_eq!(player.finite_audio_duration(), None);
    assert!(player.audio_resource_bundle_bytes().is_err());
    assert!(player.audio_anchor_json(100.0).is_err());
    assert!(player.rebind_transport(5.0, 21).is_err());
    assert!(context.take_execution_player(5.0, 21).is_err());
    assert_eq!(context.live_execution_ownership(), "active");
    assert_eq!(
        context.active_live_player().unwrap().ownership_identity(),
        identity
    );
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        before
    );
    let (_, reader) = context.create_prepared_prefix().unwrap();
    assert!(reader.has_audio(), "future onset is still an audio source");
    assert!(reader.audio_playback().unwrap().next().is_none());
    context.begin_ordinary_wait(2.0).unwrap();
    context.prepare_live_segment().unwrap();
}

#[test]
fn canonical_run_refresh_does_not_implicitly_select_headless_audio() {
    let (mut context, _) = soundtrack(0.0);
    let revision = context.scene.revision();
    context.prepare_execution_run().unwrap();
    assert!(context.begin_ordinary_wait(1.0).is_err());
    assert!(context.player_ownership.is_unstarted());
    assert_eq!(context.scene.revision(), revision);
    context.select_prepared_source().unwrap();
    context.select_prepared_source().unwrap();
    context.begin_ordinary_wait(1.0).unwrap();
    let before = context.active_live_player().unwrap().debug_frame_json();
    assert!(context.select_prepared_source().is_err());
    context.prepare_execution_run().unwrap();
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        before
    );
    context.prepare_live_segment().unwrap();
}

#[test]
fn canonical_headless_admission_still_rejects_callback_and_causal_sources_atomically() {
    for causal in [false, true] {
        let (mut context, _) = soundtrack(0.0);
        let target = context.scene.circle(0.5).unwrap();
        context.bind_mobject(ObjectId::new(0), &target).unwrap();
        if causal {
            context
                .set_translation_drag_targets(vec![target.clone()])
                .unwrap();
        } else {
            context
                .add_updater(&target, HostCallbackId::new(7), 0.0, None)
                .unwrap();
        }
        context.select_prepared_source().unwrap();
        let revision = context.scene.revision();
        assert!(context.begin_ordinary_wait(1.0).is_err());
        assert!(context.player_ownership.is_unstarted());
        assert_eq!(context.scene.revision(), revision);
        assert_eq!(
            context.bindings.get(&ObjectId::new(0)),
            Some(&target.node_id())
        );
    }
}

#[test]
fn automatic_audio_preparation_preserves_finite_audio_and_realtime_callback_profiles() {
    let (mut finite, _) = soundtrack(0.0);
    finite.allow_prepared_audio_source().unwrap();
    finite.authored_wait(3.0).unwrap();
    let player = finite.take_execution_player(3.0, 101).unwrap();
    assert!(finite.source_preparation == SourcePreparation::AutomaticAudio);
    assert_eq!(player.finite_audio_duration(), Some(3.0));
    assert!(player.audio_resource_bundle_bytes().is_ok());
    finite.return_execution_player(player).unwrap();

    let (mut realtime, target) = fixture();
    realtime.allow_prepared_audio_source().unwrap();
    realtime
        .add_updater(&target, HostCallbackId::new(9), 0.0, None)
        .unwrap();
    realtime.begin_ordinary_wait(1.0).unwrap();
    assert!(realtime.source_preparation == SourcePreparation::Realtime);
    let player = realtime.take_execution_player(1.0, 102).unwrap();
    assert!(player.has_required_callbacks());
    realtime.return_execution_player(player).unwrap();
}
