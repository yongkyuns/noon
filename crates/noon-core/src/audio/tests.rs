use super::*;
use crate::AudioResourceArena;

fn source(sample_rate: u32, channels: u16, frames: usize) -> (AudioResourceHandle, AudioResource) {
    let resource = AudioResource::from_interleaved(
        sample_rate,
        channels,
        vec![0.0; frames * usize::from(channels)],
        usize::MAX,
    )
    .unwrap();
    let mut arena = AudioResourceArena::new();
    let handle = arena.insert(resource.clone()).unwrap();
    (handle, resource)
}

fn clip(options: AudioClipOptions) -> AudioClipDefinition {
    let (handle, resource) = source(8_000, 2, 16_000);
    AudioClipDefinition::new(handle, &resource, options).unwrap()
}

fn position(clip: &AudioClipDefinition, time: f64) -> AudioClipPosition {
    clip.state_at(time).unwrap().unwrap()
}

#[test]
fn one_shot_uses_trimmed_frame_duration_and_exclusive_end() {
    let clip = clip(AudioClipOptions {
        start_time: 2.0,
        source_start_frame: 2_000,
        source_end_frame: Some(10_000),
        ..Default::default()
    });
    assert_eq!(clip.duration(), 1.0);
    assert_eq!(clip.end_time(), 3.0);
    assert_eq!(clip.state_at(2.0_f64.next_down()), Ok(None));
    assert_eq!(position(&clip, 2.0).source_frame, 2_000.0);
    assert_eq!(position(&clip, 2.25).source_frame, 4_000.0);
    let last = position(&clip, 3.0_f64.next_down());
    assert!(last.source_frame >= 2_000.0 && last.source_frame < 10_000.0);
    assert_eq!(clip.state_at(3.0), Ok(None));
    assert_eq!(clip.state_at(f64::MAX), Ok(None));
}

#[test]
fn explicit_one_shot_duration_can_cut_source_short() {
    let clip = clip(AudioClipOptions {
        duration: Some(0.5),
        ..Default::default()
    });
    assert_eq!(position(&clip, 0.25).source_frame, 2_000.0);
    assert_eq!(clip.state_at(0.5), Ok(None));
}

#[test]
fn fractional_positions_count_frames_not_interleaved_samples() {
    let (handle, resource) = source(48_000, 2, 48_000);
    let clip = AudioClipDefinition::new(handle, &resource, AudioClipOptions::default()).unwrap();
    assert_eq!(position(&clip, 0.5).source_frame, 24_000.0);
    for frame in [0.5, 1.0, 127.25, 12_345.5, 47_999.0] {
        assert!((position(&clip, frame / 48_000.0).source_frame - frame).abs() < 1e-10);
    }
}

#[test]
fn loop_repeats_the_whole_trimmed_range_without_resetting_fades() {
    let clip = clip(AudioClipOptions {
        start_time: 1.0,
        source_start_frame: 2_000,
        source_end_frame: Some(6_000),
        looped: true,
        duration: Some(3.0),
        gain: 0.2,
        fade_in: 1.0,
        fade_out: 1.0,
    });
    assert_eq!(position(&clip, 1.0).source_frame, 2_000.0);
    assert_eq!(position(&clip, 1.25).source_frame, 4_000.0);
    assert_eq!(position(&clip, 1.5).source_frame, 2_000.0);
    assert_eq!(position(&clip, 1.5).gain, 0.1);
    assert_eq!(position(&clip, 3.0).gain, 0.2);
    assert_eq!(position(&clip, 3.5).gain, 0.1);
    assert_eq!(clip.state_at(4.0), Ok(None));
}

#[test]
fn repeated_paused_queries_and_forward_backward_jumps_have_no_history() {
    let clip = clip(AudioClipOptions {
        source_start_frame: 7,
        source_end_frame: Some(4_007),
        looped: true,
        duration: Some(1_000_000.0),
        ..Default::default()
    });
    let paused = position(&clip, 0.125);
    assert_eq!(
        position(&clip, 999_999.125).source_frame,
        paused.source_frame
    );
    assert_eq!(position(&clip, 0.125), paused);
    assert_eq!(position(&clip, 0.125), paused);
    assert_eq!(position(&clip, 0.0).source_frame, 7.0);
}

#[test]
fn enormous_finite_loop_query_preserves_phase_without_overflow() {
    let (handle, resource) = source(8_000, 1, 7);
    let clip = AudioClipDefinition::new(
        handle,
        &resource,
        AudioClipOptions {
            looped: true,
            duration: Some(f64::MAX),
            ..Default::default()
        },
    )
    .unwrap();
    // MAX/2 = (2^53 - 1) * 2^970. Modulo 7 it is 6, and 8000
    // is also 6 modulo 7, so its elapsed PCM-frame phase is 1.
    assert_eq!(position(&clip, f64::MAX / 2.0).source_frame, 1.0);
    assert_eq!(clip.state_at(f64::MAX), Ok(None));
}

#[test]
fn large_elapsed_time_keeps_fractional_frames_after_range_reduction() {
    let (handle, resource) = source(8_000, 1, 7);
    let clip = AudioClipDefinition::new(
        handle,
        &resource,
        AudioClipOptions {
            looped: true,
            duration: Some(2_000_000_000_000.0),
            ..Default::default()
        },
    )
    .unwrap();
    // This exact f64 fraction is 1/4096 second. Multiplying the full
    // elapsed time by 8000 first would discard part of the frame fraction.
    let seconds = 1_500_000_000_000_u128;
    let fraction = 1.0 / 4096.0;
    let expected = (((seconds * 8_000) % 7) as f64 + fraction * 8_000.0) % 7.0;
    assert_eq!(
        position(&clip, seconds as f64 + fraction).source_frame,
        expected
    );
}

#[test]
fn gain_and_nonoverlapping_linear_fades_are_bounded() {
    let clip = clip(AudioClipOptions {
        gain: 0.4,
        fade_in: 0.5,
        fade_out: 1.0,
        ..Default::default()
    });
    assert_eq!(position(&clip, 0.0).gain, 0.0);
    assert_eq!(position(&clip, 0.25).gain, 0.2);
    assert_eq!(position(&clip, 0.5).gain, 0.4);
    assert_eq!(position(&clip, 1.0).gain, 0.4);
    assert_eq!(position(&clip, 1.5).gain, 0.2);
    let muted = clip_with_gain(0.0);
    assert_eq!(position(&muted, 1.0).gain, 0.0);
    let maximal = clip_with_gain(f64::MAX);
    assert_eq!(position(&maximal, 1.0).gain, f64::MAX);
}

fn clip_with_gain(gain: f64) -> AudioClipDefinition {
    clip(AudioClipOptions {
        gain,
        ..Default::default()
    })
}

#[test]
fn invalid_scalar_inputs_are_rejected_before_a_definition_exists() {
    let (handle, resource) = source(8_000, 1, 8_000);
    let make = |options| AudioClipDefinition::new(handle, &resource, options);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert_eq!(
            make(AudioClipOptions {
                start_time: value,
                ..Default::default()
            }),
            Err(AudioClipError::InvalidStartTime)
        );
        assert_eq!(
            make(AudioClipOptions {
                duration: Some(value),
                ..Default::default()
            }),
            Err(AudioClipError::InvalidDuration)
        );
        assert_eq!(
            make(AudioClipOptions {
                gain: value,
                ..Default::default()
            }),
            Err(AudioClipError::InvalidGain)
        );
        assert_eq!(
            make(AudioClipOptions {
                fade_in: value,
                ..Default::default()
            }),
            Err(AudioClipError::InvalidFadeIn)
        );
        assert_eq!(
            make(AudioClipOptions {
                fade_out: value,
                ..Default::default()
            }),
            Err(AudioClipError::InvalidFadeOut)
        );
    }
    assert_eq!(
        make(AudioClipOptions {
            duration: Some(0.0),
            ..Default::default()
        }),
        Err(AudioClipError::InvalidDuration)
    );
}

#[test]
fn invalid_ranges_durations_and_fades_are_rejected() {
    let (handle, resource) = source(8_000, 1, 8_000);
    let make = |options| AudioClipDefinition::new(handle, &resource, options);
    for (start, end) in [(0, 0), (2, 1), (0, 8_001), (8_000, 8_000)] {
        assert!(matches!(
            make(AudioClipOptions {
                source_start_frame: start,
                source_end_frame: Some(end),
                ..Default::default()
            }),
            Err(AudioClipError::InvalidSourceRange { .. })
        ));
    }
    assert_eq!(
        make(AudioClipOptions {
            looped: true,
            ..Default::default()
        }),
        Err(AudioClipError::LoopRequiresDuration)
    );
    assert_eq!(
        make(AudioClipOptions {
            duration: Some(1.0_f64.next_up()),
            ..Default::default()
        }),
        Err(AudioClipError::DurationExceedsSource)
    );
    for (fade_in, fade_out) in [(1.0_f64.next_up(), 0.0), (0.0, 2.0), (0.75, 0.5)] {
        assert_eq!(
            make(AudioClipOptions {
                fade_in,
                fade_out,
                ..Default::default()
            }),
            Err(AudioClipError::FadesExceedDuration)
        );
    }
    assert!(make(AudioClipOptions {
        fade_in: 0.25,
        fade_out: 0.75,
        ..Default::default()
    })
    .is_ok());
}

#[test]
fn overflowing_and_swallowed_end_times_are_rejected() {
    let (handle, resource) = source(8_000, 1, 8_000);
    for (start_time, duration) in [(f64::MAX, f64::MAX), (1e20, 1.0)] {
        assert_eq!(
            AudioClipDefinition::new(
                handle,
                &resource,
                AudioClipOptions {
                    start_time,
                    duration: Some(duration),
                    looped: true,
                    ..Default::default()
                }
            ),
            Err(AudioClipError::UnrepresentableTimeRange)
        );
    }
}

#[test]
fn invalid_query_time_is_an_error_even_outside_the_clip() {
    let clip = clip(AudioClipOptions::default());
    for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert_eq!(clip.state_at(time), Err(AudioClipError::InvalidSceneTime));
    }
    assert_eq!(clip.state_at(-0.0), clip.state_at(0.0));
}

#[test]
fn admission_metadata_check_rejects_changed_rate_channels_or_frame_count() {
    let (handle, resource) = source(8_000, 1, 8_000);
    let clip = AudioClipDefinition::new(handle, &resource, AudioClipOptions::default()).unwrap();
    assert_eq!(clip.validate_resource(&resource), Ok(()));
    for (rate, channels, frames) in [(16_000, 1, 8_000), (8_000, 2, 8_000), (8_000, 1, 8_001)] {
        let (_, different) = source(rate, channels, frames);
        assert_eq!(
            clip.validate_resource(&different),
            Err(AudioClipError::ResourceMismatch)
        );
    }
}
