use noon_compile::{lower_semantic_execution_root, SemanticExecutionIndex};
use noon_core::{
    AudioClipDefinition, AudioClipOptions, AudioResource, SemanticMutationTransaction,
    SemanticStore,
};

use crate::{
    AudioGainPoint, AudioLoopRegion, AudioPlaybackError, SceneInstance, TimelineWakeState,
};

fn runtime() -> SceneInstance {
    runtime_with(AudioClipOptions {
        start_time: 2.0,
        source_start_frame: 8_000,
        source_end_frame: Some(24_000),
        duration: Some(8.0),
        looped: true,
        gain: 0.5,
        fade_in: 2.0,
        fade_out: 2.0,
    })
}

fn runtime_with(options: AudioClipOptions) -> SceneInstance {
    runtime_with_count(options, 1)
}

fn runtime_with_count(options: AudioClipOptions, count: usize) -> SceneInstance {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let pcm = AudioResource::from_interleaved(8_000, 1, vec![0.125; 32_000], 128_000).unwrap();
    let resource = store.insert_audio_resource(pcm).unwrap();
    let definition = AudioClipDefinition::new(
        resource,
        store.audio_resources().get(resource).unwrap(),
        options,
    )
    .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    for _ in 0..count {
        transaction.create_audio_clip(root, definition);
    }
    transaction.apply(&mut store).unwrap();
    let compiled = lower_semantic_execution_root(&store, root, &mut SemanticExecutionIndex::new())
        .unwrap()
        .into_parts()
        .0;
    SceneInstance::new(compiled)
}

fn points(values: &[(f64, f64)]) -> Vec<AudioGainPoint> {
    values
        .iter()
        .map(|&(offset, gain)| AudioGainPoint { offset, gain })
        .collect()
}

#[test]
fn future_clip_instructions_include_trim_loop_and_clip_wide_envelope() {
    let runtime = runtime();
    assert!(runtime.has_audio());
    assert_eq!(runtime.audio_end_time(), Some(10.0));
    assert_eq!(runtime.finite_timeline_end_time(), Some(10.0));
    let clip = runtime.audio_playback().unwrap().next().unwrap();
    assert_eq!(clip.start_delay, 2.0);
    assert_eq!(clip.duration, 8.0);
    assert_eq!(clip.source_offset, 1.0);
    assert_eq!(
        clip.loop_region,
        Some(AudioLoopRegion {
            start: 1.0,
            end: 3.0
        })
    );
    assert_eq!(clip.pcm.frames(), 32_000);
    assert_eq!(
        clip.gain_points.points(),
        points(&[(0.0, 0.0), (2.0, 0.5), (6.0, 0.5), (8.0, 0.0)])
    );
}

#[test]
fn seek_reanchors_source_phase_and_only_remaining_gain_envelope() {
    let mut runtime = runtime();
    runtime.seek(3.0).unwrap();
    let clip = runtime.audio_playback().unwrap().next().unwrap();
    assert_eq!(clip.source_offset, 2.0);
    assert_eq!(clip.start_delay, 0.0);
    assert_eq!(clip.duration, 7.0);
    assert_eq!(
        clip.gain_points.points(),
        points(&[(0.0, 0.25), (1.0, 0.5), (5.0, 0.5), (7.0, 0.0)])
    );
    runtime.seek(9.0).unwrap();
    let clip = runtime.audio_playback().unwrap().next().unwrap();
    assert_eq!(clip.source_offset, 2.0);
    assert_eq!(clip.duration, 1.0);
    assert_eq!(
        clip.gain_points.points(),
        points(&[(0.0, 0.25), (1.0, 0.0)])
    );
    runtime.seek(10.0).unwrap();
    assert_eq!(runtime.audio_playback().unwrap().count(), 0);
}

#[test]
fn forward_backward_and_repeated_queries_agree_without_advancing_time() {
    let mut runtime = runtime();
    runtime.advance_to(5.0).unwrap();
    let expected = {
        let clip = runtime.audio_playback().unwrap().next().unwrap();
        (clip.source_offset, clip.duration, clip.gain_points)
    };
    for time in [8.5, 0.0, 3.0, 0.0] {
        runtime.seek(time).unwrap();
        runtime.advance_to(5.0).unwrap();
        let clip = runtime.audio_playback().unwrap().next().unwrap();
        assert_eq!(
            (clip.source_offset, clip.duration, clip.gain_points),
            expected
        );
        assert_eq!(runtime.frame().time, 5.0);
    }
}

#[test]
fn audio_uses_existing_event_scheduler_without_visual_dirty_work() {
    let mut runtime = runtime();
    runtime.take_frame_changes();
    assert!(runtime.has_timeline_channels());
    assert_eq!(
        runtime.wake_state().timeline(),
        TimelineWakeState::Deadline(2.0)
    );
    runtime.advance_to(1.0).unwrap();
    assert!(runtime.take_frame_changes().is_empty());
    runtime.advance_to(2.0).unwrap();
    assert_eq!(
        runtime.wake_state().timeline(),
        TimelineWakeState::Deadline(10.0)
    );
    runtime.advance_to(3.0).unwrap();
    assert_eq!(runtime.last_stats().groups_evaluated, 0);
    assert_eq!(runtime.last_timeline_scheduler_stats().active_groups, 0);
    assert_eq!(runtime.last_timeline_scheduler_stats().groups_requested, 0);
    assert_eq!(runtime.last_timeline_scheduler_stats().events_crossed, 0);
    assert!(runtime.take_frame_changes().is_empty());
    runtime.advance_to(10.0).unwrap();
    assert_eq!(
        runtime.wake_state().timeline(),
        TimelineWakeState::Quiescent
    );
    assert!(runtime.take_frame_changes().is_empty());
}

#[test]
fn audio_output_rejects_negative_scene_time_without_changing_it() {
    let mut runtime = runtime();
    runtime.seek(-1.0).unwrap();
    assert!(matches!(
        runtime.audio_playback(),
        Err(AudioPlaybackError::InvalidTime(-1.0))
    ));
    assert_eq!(runtime.frame().time, -1.0);
}

#[test]
fn visual_only_runtime_has_no_audio_allocation_or_cadence() {
    let compiled = noon_compile::CompiledScene::compile_objects(Vec::new(), &[]).unwrap();
    let mut runtime = SceneInstance::new(compiled);
    runtime.take_frame_changes();
    assert!(!runtime.has_audio());
    assert_eq!(runtime.audio_end_time(), None);
    assert_eq!(runtime.audio_playback().unwrap().count(), 0);
    assert!(!runtime.has_timeline_channels());
    assert!(runtime.wake_state().is_quiescent());
}

#[test]
fn once_clip_uses_trimmed_region_and_stops_at_its_half_open_end() {
    let mut runtime = runtime_with(AudioClipOptions {
        start_time: 1.0,
        source_start_frame: 4_000,
        source_end_frame: Some(20_000),
        gain: 0.25,
        ..AudioClipOptions::default()
    });
    let clip = runtime.audio_playback().unwrap().next().unwrap();
    assert_eq!(clip.source_offset, 0.5);
    assert_eq!(clip.duration, 2.0);
    assert_eq!(clip.loop_region, None);
    assert_eq!(
        clip.gain_points.points(),
        points(&[(0.0, 0.25), (2.0, 0.25)])
    );
    runtime.advance_to(2.0).unwrap();
    let clip = runtime.audio_playback().unwrap().next().unwrap();
    assert_eq!(clip.source_offset, 1.5);
    assert_eq!(clip.duration, 1.0);
    runtime.advance_to(3.0).unwrap();
    assert_eq!(runtime.audio_playback().unwrap().count(), 0);
}

#[test]
fn prepared_time_does_not_change_audio_until_the_coherent_frame_commits() {
    let mut runtime = runtime();
    let prepared = runtime.prepare_advance_to(3.0).unwrap();
    assert_eq!(
        runtime
            .audio_playback()
            .unwrap()
            .next()
            .unwrap()
            .start_delay,
        2.0
    );
    assert_eq!(runtime.frame().time, 0.0);
    let effective = runtime.prepare_effective_property_batch(&[]).unwrap();
    runtime.commit_prepared_frame(prepared, effective).unwrap();
    let clip = runtime.audio_playback().unwrap().next().unwrap();
    assert_eq!(clip.start_delay, 0.0);
    assert_eq!(clip.source_offset, 2.0);
    assert_eq!(runtime.frame().time, 3.0);
    assert_eq!(
        runtime.wake_state().timeline(),
        TimelineWakeState::Deadline(10.0)
    );
}

#[test]
fn immutable_audio_history_reuses_pcm_and_reconstructs_clip_phase() {
    let mut runtime = runtime();
    let times = [0.0, 2.0, 3.0, 5.0, 9.0, 10.0];
    let observe = |runtime: &SceneInstance| {
        runtime
            .audio_playback()
            .unwrap()
            .map(|clip| {
                (
                    clip.clip,
                    *clip.definition,
                    clip.resource,
                    clip.pcm.samples().as_ptr(),
                    clip.start_delay,
                    clip.source_offset,
                    clip.duration,
                    clip.gain_points,
                )
            })
            .collect::<Vec<_>>()
    };
    let expected = times
        .iter()
        .map(|&time| {
            runtime.seek(time).unwrap();
            observe(&runtime)
        })
        .collect::<Vec<_>>();
    runtime.seek(0.0).unwrap();
    let publication = runtime.publication_context();
    runtime
        .begin_replay_retention(crate::ReplayLimits {
            revisions: 0,
            payloads: 0,
        })
        .unwrap();
    assert_eq!(runtime.publication_context(), publication);
    runtime.advance_to(10.0).unwrap();
    runtime.seal_replay().unwrap();
    for (&time, expected) in times.iter().zip(&expected).rev() {
        runtime.seek(time).unwrap();
        assert_eq!(&observe(&runtime), expected, "audio at {time}");
    }
    assert_eq!(runtime.replay_stats().payloads_retained, 0);
    assert_eq!(runtime.replay_stats().revisions_retained, 0);
    runtime.discard_replay_retention();
    assert_eq!(runtime.frame().time, 10.0);
    assert!(runtime.audio_playback().unwrap().next().is_none());
}

#[test]
fn many_audio_clips_do_not_enter_per_frame_active_group_traversal() {
    let mut runtime = runtime_with_count(
        AudioClipOptions {
            start_time: 2.0,
            duration: Some(8.0),
            looped: true,
            ..AudioClipOptions::default()
        },
        128,
    );
    runtime.take_frame_changes();
    runtime.advance_to(3.0).unwrap();
    assert_eq!(runtime.last_timeline_scheduler_stats().events_crossed, 128);
    assert_eq!(
        runtime.wake_state().timeline(),
        TimelineWakeState::Deadline(10.0)
    );
    runtime.advance_to(4.0).unwrap();
    let stats = runtime.last_timeline_scheduler_stats();
    assert_eq!(stats.active_groups, 0);
    assert_eq!(stats.groups_requested, 0);
    assert_eq!(stats.events_crossed, 0);
    assert_eq!(runtime.last_stats().groups_evaluated, 0);
    assert!(runtime.take_frame_changes().is_empty());
    assert_eq!(runtime.audio_playback().unwrap().count(), 128);
}

#[cfg(feature = "prepared-prefix")]
#[test]
fn audio_history_does_not_bypass_prefix_resource_admission() {
    let mut runtime = runtime();
    runtime
        .begin_replay_retention(crate::ReplayLimits::default())
        .unwrap();
    runtime.advance_to(3.0).unwrap();
    let publication = runtime.publication_context();
    assert!(matches!(
        runtime.create_prefix_projection_with_resource_budget(0),
        Err(crate::PrefixError::ResourceBudgetExceeded {
            retained_bytes: 0,
            additional_bytes,
            limit_bytes: 0,
        }) if additional_bytes > 128_000
    ));
    assert_eq!(runtime.publication_context(), publication);
    assert_eq!(runtime.frame().time, 3.0);
    assert_eq!(
        runtime
            .audio_playback()
            .unwrap()
            .next()
            .unwrap()
            .source_offset,
        2.0
    );
}
