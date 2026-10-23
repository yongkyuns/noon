use noon_compile::{lower_semantic_execution_root, SemanticExecutionIndex};
use noon_core::{
    AudioClipDefinition, AudioClipOptions, AudioResource, SemanticMutationTransaction,
    SemanticStore,
};

use crate::{AudioMixError, AudioMixFormat, SceneInstance, MAX_AUDIO_MIX_BLOCK_FRAMES};

// A power-of-two rate gives exactly representable authored sample boundaries
// for the small hand-calculated oracle fixtures below.
const RATE: u32 = 8_192;

fn pcm(rate: u32, channels: u16, samples: &[f32]) -> AudioResource {
    AudioResource::from_interleaved(rate, channels, samples.to_vec(), samples.len() * 4).unwrap()
}

fn runtime(clips: Vec<(AudioResource, AudioClipOptions)>) -> SceneInstance {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let mut transaction = SemanticMutationTransaction::new();
    for (pcm, options) in clips {
        let resource = store.insert_audio_resource(pcm).unwrap();
        let definition = AudioClipDefinition::new(
            resource,
            store.audio_resources().get(resource).unwrap(),
            options,
        )
        .unwrap();
        transaction.create_audio_clip(root, definition);
    }
    transaction.apply(&mut store).unwrap();
    let compiled = lower_semantic_execution_root(&store, root, &mut SemanticExecutionIndex::new())
        .unwrap()
        .into_parts()
        .0;
    // Dropping the authoring store also exercises compiled PCM ownership.
    SceneInstance::new(compiled)
}

fn mono_format() -> AudioMixFormat {
    AudioMixFormat::new(RATE, 1).unwrap()
}

fn render(
    runtime: &SceneInstance,
    scene_start: f64,
    first_frame: u64,
    format: AudioMixFormat,
    frames: usize,
) -> Vec<f32> {
    let mut output = vec![0.875; frames * usize::from(format.channels())];
    runtime
        .mix_audio_block(scene_start, first_frame, format, &mut output)
        .unwrap();
    output
}

fn rejected_without_writes(
    runtime: &SceneInstance,
    scene_start: f64,
    first_frame: u64,
    format: AudioMixFormat,
    samples: usize,
) -> AudioMixError {
    let mut output = vec![0.875; samples];
    let error = runtime
        .mix_audio_block(scene_start, first_frame, format, &mut output)
        .unwrap_err();
    assert!(output.iter().all(|value| *value == 0.875));
    error
}

#[test]
fn format_admits_only_supported_pcm_rates_and_channels() {
    for sample_rate in [0, 7_999, 192_001, u32::MAX] {
        assert_eq!(
            AudioMixFormat::new(sample_rate, 2),
            Err(AudioMixError::UnsupportedSampleRate { sample_rate })
        );
    }
    for channels in [0, 3, u16::MAX] {
        assert_eq!(
            AudioMixFormat::new(48_000, channels),
            Err(AudioMixError::UnsupportedChannelCount { channels })
        );
    }
    for sample_rate in [8_000, 48_000, 192_000] {
        for channels in [1, 2] {
            let format = AudioMixFormat::new(sample_rate, channels).unwrap();
            assert_eq!(format.sample_rate(), sample_rate);
            assert_eq!(format.channels(), channels);
        }
    }
    assert_eq!(
        AudioMixFormat::STEREO_48_KHZ,
        AudioMixFormat::new(48_000, 2).unwrap()
    );
}

#[test]
fn audio_free_scene_fills_a_bounded_block_with_silence() {
    let runtime = runtime(Vec::new());
    assert_eq!(
        render(&runtime, 0.25, 123, AudioMixFormat::STEREO_48_KHZ, 17),
        vec![0.0; 34]
    );
    assert_eq!(
        render(&runtime, 0.0, 0, mono_format(), MAX_AUDIO_MIX_BLOCK_FRAMES),
        vec![0.0; MAX_AUDIO_MIX_BLOCK_FRAMES]
    );
    let mut empty = [];
    runtime
        .mix_audio_block(0.0, 1 << 53, mono_format(), &mut empty)
        .unwrap();
}

#[test]
fn clip_onsets_and_exclusive_ends_preserve_silent_gaps() {
    let runtime = runtime(vec![
        (
            pcm(RATE, 1, &[0.25, 0.25]),
            AudioClipOptions {
                start_time: 2.0 / f64::from(RATE),
                ..Default::default()
            },
        ),
        (
            pcm(RATE, 1, &[0.5, 0.5]),
            AudioClipOptions {
                start_time: 5.0 / f64::from(RATE),
                ..Default::default()
            },
        ),
    ]);
    assert_eq!(
        render(&runtime, 0.0, 0, mono_format(), 8),
        [0.0, 0.0, 0.25, 0.25, 0.0, 0.5, 0.5, 0.0]
    );
}

#[test]
fn mono_duplicates_stereo_keeps_identity_and_mono_downmix_averages() {
    let mono = runtime(vec![(
        pcm(RATE, 1, &[0.25, -0.5]),
        AudioClipOptions::default(),
    )]);
    let stereo_format = AudioMixFormat::new(RATE, 2).unwrap();
    assert_eq!(
        render(&mono, 0.0, 0, stereo_format, 2),
        [0.25, 0.25, -0.5, -0.5]
    );
    let stereo = runtime(vec![(
        pcm(RATE, 2, &[0.25, -0.5, 0.75, 0.125]),
        AudioClipOptions::default(),
    )]);
    assert_eq!(
        render(&stereo, 0.0, 0, stereo_format, 2),
        [0.25, -0.5, 0.75, 0.125]
    );
    assert_eq!(render(&stereo, 0.0, 0, mono_format(), 2), [-0.125, 0.4375]);
}

#[test]
fn overlapping_clips_sum_and_only_the_final_mix_is_range_checked() {
    let mixed = runtime(vec![
        (pcm(RATE, 1, &[0.25; 4]), AudioClipOptions::default()),
        (
            pcm(RATE, 1, &[0.5; 2]),
            AudioClipOptions {
                start_time: 1.0 / f64::from(RATE),
                ..Default::default()
            },
        ),
    ]);
    assert_eq!(
        render(&mixed, 0.0, 0, mono_format(), 5),
        [0.25, 0.75, 0.75, 0.25, 0.0]
    );
    let cancellation = runtime(vec![
        (pcm(RATE, 1, &[0.75]), AudioClipOptions::default()),
        (pcm(RATE, 1, &[0.75]), AudioClipOptions::default()),
        (pcm(RATE, 1, &[-0.75]), AudioClipOptions::default()),
    ]);
    assert_eq!(render(&cancellation, 0.0, 0, mono_format(), 1), [0.75]);
}

#[test]
fn one_shot_interpolation_holds_the_last_trimmed_frame() {
    let runtime = runtime(vec![(
        pcm(RATE, 1, &[-1.0, 0.25, 0.75, -1.0]),
        AudioClipOptions {
            source_start_frame: 1,
            source_end_frame: Some(3),
            ..Default::default()
        },
    )]);
    assert_eq!(
        render(
            &runtime,
            0.0,
            0,
            AudioMixFormat::new(RATE * 2, 1).unwrap(),
            5
        ),
        [0.25, 0.5, 0.75, 0.75, 0.0]
    );
}

#[test]
fn loop_interpolation_wraps_its_neighbor_inside_the_trim() {
    let runtime = runtime(vec![(
        pcm(RATE, 1, &[-1.0, 0.25, 0.75, -1.0]),
        AudioClipOptions {
            source_start_frame: 1,
            source_end_frame: Some(3),
            looped: true,
            duration: Some(4.0 / f64::from(RATE)),
            ..Default::default()
        },
    )]);
    assert_eq!(
        render(
            &runtime,
            0.0,
            0,
            AudioMixFormat::new(RATE * 2, 1).unwrap(),
            9
        ),
        [0.25, 0.5, 0.75, 0.5, 0.25, 0.5, 0.75, 0.5, 0.0]
    );
}

#[test]
fn a_one_frame_loop_is_constant_through_fractional_source_positions() {
    let runtime = runtime(vec![(
        pcm(RATE, 1, &[-1.0, 0.375, -1.0]),
        AudioClipOptions {
            source_start_frame: 1,
            source_end_frame: Some(2),
            looped: true,
            duration: Some(3.0 / f64::from(RATE)),
            ..Default::default()
        },
    )]);
    assert_eq!(
        render(
            &runtime,
            0.0,
            0,
            AudioMixFormat::new(RATE * 2, 1).unwrap(),
            7
        ),
        [0.375, 0.375, 0.375, 0.375, 0.375, 0.375, 0.0]
    );
}

#[test]
fn gain_and_fades_span_the_whole_clip_without_restarting_on_loops() {
    let runtime = runtime(vec![(
        pcm(RATE, 1, &[0.5, 0.5]),
        AudioClipOptions {
            start_time: 1.0 / f64::from(RATE),
            duration: Some(8.0 / f64::from(RATE)),
            looped: true,
            gain: 0.5,
            fade_in: 2.0 / f64::from(RATE),
            fade_out: 2.0 / f64::from(RATE),
            ..Default::default()
        },
    )]);
    assert_eq!(
        render(&runtime, 0.0, 0, mono_format(), 10),
        [0.0, 0.0, 0.125, 0.25, 0.25, 0.25, 0.25, 0.25, 0.125, 0.0]
    );
}

#[test]
fn a_fractional_crop_keeps_source_phase_and_an_absolute_output_index() {
    let runtime = runtime(vec![(
        pcm(RATE, 1, &[0.0, 0.25, 0.5, 0.75]),
        AudioClipOptions {
            start_time: 2.0 / f64::from(RATE),
            ..Default::default()
        },
    )]);
    let expected = [0.125, 0.375, 0.625, 0.75, 0.0];
    assert_eq!(
        render(&runtime, 2.5 / f64::from(RATE), 0, mono_format(), 5),
        expected
    );
    assert_eq!(
        render(&runtime, 0.5 / f64::from(RATE), 2, mono_format(), 5),
        expected
    );
}

#[test]
fn resampling_24khz_to_48khz_linearly_interpolates_and_duplicates_mono() {
    let runtime = runtime(vec![(
        pcm(24_000, 1, &[0.0, 0.5, 1.0]),
        AudioClipOptions::default(),
    )]);
    assert_eq!(
        render(&runtime, 0.0, 0, AudioMixFormat::STEREO_48_KHZ, 7),
        [0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0]
    );
}

#[test]
fn early_authored_duration_ends_before_the_source_trim_is_exhausted() {
    let runtime = runtime(vec![(
        pcm(RATE, 1, &[0.0, 0.5, 1.0]),
        AudioClipOptions {
            duration: Some(1.5 / f64::from(RATE)),
            ..Default::default()
        },
    )]);
    assert_eq!(
        render(
            &runtime,
            0.0,
            0,
            AudioMixFormat::new(RATE * 2, 1).unwrap(),
            5
        ),
        [0.0, 0.25, 0.5, 0.0, 0.0]
    );
}

#[test]
fn absolute_sample_indices_make_output_identical_across_block_sizes() {
    let runtime = runtime(vec![
        (
            pcm(24_000, 1, &[0.0, 0.25, 0.5, 0.0, -0.5]),
            AudioClipOptions {
                start_time: 0.001,
                duration: Some(0.1),
                looped: true,
                gain: 0.25,
                fade_in: 0.0125,
                fade_out: 0.0125,
                ..Default::default()
            },
        ),
        (
            pcm(48_000, 2, &[0.125, -0.25, 0.25, -0.125, 0.0, 0.125]),
            AudioClipOptions {
                start_time: 0.01,
                duration: Some(0.07),
                looped: true,
                gain: 0.25,
                ..Default::default()
            },
        ),
    ]);
    let scene_start = 1.0 / 96_000.0;
    let first_frame = 73;
    let frame_count = 4_099;
    let format = AudioMixFormat::STEREO_48_KHZ;
    let whole = render(&runtime, scene_start, first_frame, format, frame_count);
    for block_frames in [1, 3, 31, 257, 1_024] {
        let mut split = Vec::new();
        let mut offset = 0;
        while offset < frame_count {
            let frames = block_frames.min(frame_count - offset);
            split.extend(render(
                &runtime,
                scene_start,
                first_frame + offset as u64,
                format,
                frames,
            ));
            offset += frames;
        }
        assert_eq!(split, whole, "block size {block_frames}");
    }
}

#[test]
fn borrowed_audio_queries_preserve_runtime_time_publication_and_scheduling() {
    let mut runtime = runtime(vec![(
        pcm(RATE, 1, &[0.25; 8]),
        AudioClipOptions::default(),
    )]);
    runtime.seek(10.0).unwrap();
    runtime.take_frame_changes();
    let publication = runtime.publication_context();
    let evaluation = runtime.last_stats();
    let wake_state = runtime.wake_state();
    assert_eq!(runtime.audio_playback().unwrap().count(), 0);
    for _ in 0..3 {
        assert_eq!(render(&runtime, 0.0, 0, mono_format(), 8), [0.25; 8]);
    }
    assert_eq!(runtime.frame().time, 10.0);
    assert_eq!(runtime.publication_context(), publication);
    assert_eq!(runtime.last_stats(), evaluation);
    assert_eq!(runtime.wake_state(), wake_state);
    assert!(runtime.take_frame_changes().is_empty());
}

#[test]
fn overload_after_an_in_range_sample_rejects_the_whole_output() {
    let runtime = runtime(vec![(
        pcm(RATE, 2, &[0.25, 0.25, 0.5, -1.25]),
        AudioClipOptions::default(),
    )]);
    assert_eq!(
        rejected_without_writes(&runtime, 0.0, 0, AudioMixFormat::new(RATE, 2).unwrap(), 4),
        AudioMixError::OutputOutOfRange {
            frame: 1,
            channel: 1,
            value: -1.25
        }
    );
}

#[test]
fn extreme_finite_pcm_interpolates_in_f64_before_applying_gain() {
    let runtime = runtime(vec![(
        pcm(RATE, 1, &[f32::MAX, -f32::MAX]),
        AudioClipOptions {
            gain: 0.5 / f64::from(f32::MAX),
            ..Default::default()
        },
    )]);
    assert_eq!(
        render(
            &runtime,
            0.0,
            0,
            AudioMixFormat::new(RATE * 2, 1).unwrap(),
            5
        ),
        [0.5, 0.0, -0.5, -0.5, 0.0]
    );
}

#[test]
fn a_nonfinite_contribution_from_finite_inputs_rejects_without_writes() {
    let runtime = runtime(vec![(
        pcm(RATE, 1, &[0.0, 2.0]),
        AudioClipOptions {
            gain: f64::MAX,
            ..Default::default()
        },
    )]);
    assert_eq!(
        rejected_without_writes(&runtime, 0.0, 0, mono_format(), 2),
        AudioMixError::NonFiniteMix {
            frame: 1,
            channel: 0
        }
    );
}

#[test]
fn extreme_finite_clip_duration_keeps_a_small_query_bounded() {
    let runtime = runtime(vec![(
        pcm(RATE, 1, &[0.25]),
        AudioClipOptions {
            looped: true,
            duration: Some(f64::MAX),
            ..Default::default()
        },
    )]);
    assert_eq!(render(&runtime, 0.0, 0, mono_format(), 4), [0.25; 4]);
}

#[test]
fn invalid_origin_shape_and_block_size_are_rejected_before_output_writes() {
    let runtime = runtime(Vec::new());
    for scene_start in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            rejected_without_writes(&runtime, scene_start, 0, mono_format(), 1),
            AudioMixError::InvalidSceneStart(_)
        ));
    }
    assert_eq!(
        rejected_without_writes(&runtime, 0.0, 0, AudioMixFormat::STEREO_48_KHZ, 3),
        AudioMixError::IncompleteOutputFrame {
            samples: 3,
            channels: 2
        }
    );
    assert_eq!(
        rejected_without_writes(
            &runtime,
            0.0,
            0,
            mono_format(),
            MAX_AUDIO_MIX_BLOCK_FRAMES + 1
        ),
        AudioMixError::BlockFrameLimitExceeded {
            frames: MAX_AUDIO_MIX_BLOCK_FRAMES + 1,
            limit: MAX_AUDIO_MIX_BLOCK_FRAMES
        }
    );
}

#[test]
fn overflowing_frame_ranges_and_collapsed_sample_times_preserve_output() {
    let runtime = runtime(Vec::new());
    assert_eq!(
        rejected_without_writes(&runtime, 0.0, u64::MAX, mono_format(), 1),
        AudioMixError::OutputFrameRangeOverflow
    );
    assert_eq!(
        rejected_without_writes(&runtime, 0.0, 1 << 53, mono_format(), 1),
        AudioMixError::OutputFramePrecisionLoss {
            frame: (1 << 53) + 1
        }
    );
    assert_eq!(
        rejected_without_writes(&runtime, 0.0, (1 << 53) + 1, mono_format(), 0),
        AudioMixError::OutputFramePrecisionLoss {
            frame: (1 << 53) + 1
        }
    );
    for samples in [1, 2, 17] {
        assert_eq!(
            rejected_without_writes(&runtime, 2.0_f64.powi(40), 0, mono_format(), samples),
            AudioMixError::SceneTimePrecisionLoss { frame: 0 }
        );
    }
}
