//! Device-independent callback qualification and a real native output fixture.

mod controls;

use super::*;
use crate::execution_source::{
    LiveProgramExecutionSource, NativeExecutionSource, StaticExecutionSource,
};
use crate::NativeViewportConfig;
#[cfg(target_os = "linux")]
use crate::NativeApp;
use noon::{AudioClipOptions, AudioResource, RustHostCallbackTable, Scene};

fn pcm(rate: u32, channels: u16, samples: &[f32]) -> AudioResource {
    AudioResource::from_interleaved(rate, channels, samples.to_vec(), samples.len() * 4).unwrap()
}

fn session(clips: Vec<(AudioResource, AudioClipOptions)>) -> ExecutionSession {
    let mut scene = Scene::new();
    for (resource, options) in clips {
        scene.audio(resource, options).unwrap();
    }
    scene.execution_session().unwrap()
}

fn constant_session() -> ExecutionSession {
    session(vec![(
        pcm(48_000, 1, &[0.25]),
        AudioClipOptions {
            looped: true,
            duration: Some(2.0),
            ..Default::default()
        },
    )])
}

fn callback(
    session: &ExecutionSession,
    epoch: Instant,
    scene_origin: f64,
    format: AudioMixFormat,
    capacity: usize,
) -> OutputCallback {
    OutputCallback::new(
        Arc::new(OutputState::new(epoch, scene_origin, 1.0)),
        session.prepare_audio_mix(format, capacity).unwrap(),
    )
}

fn timestamp(callback: Duration, playback: Duration) -> cpal::OutputStreamTimestamp {
    cpal::OutputStreamTimestamp {
        callback: StreamInstant::new(callback.as_secs(), callback.subsec_nanos()),
        playback: StreamInstant::new(playback.as_secs(), playback.subsec_nanos()),
    }
}

fn steady_timestamp(elapsed: Duration, latency: Duration) -> cpal::OutputStreamTimestamp {
    let callback = Duration::from_secs(10) + elapsed;
    timestamp(callback, callback + latency)
}

fn apply_start_attack(samples: &mut [f32], format: AudioMixFormat, first_frame: u64) {
    let attack_frames = (f64::from(format.sample_rate()) * 0.040).ceil();
    for (offset, frame) in samples
        .chunks_exact_mut(usize::from(format.channels()))
        .enumerate()
    {
        let gain = ((first_frame + offset as u64) as f64 / attack_frames).min(1.0);
        for sample in frame {
            *sample = (f64::from(*sample) * gain) as f32;
        }
    }
}

#[test]
fn preparation_and_empty_callbacks_are_silent_without_a_clock_anchor() {
    let epoch = Instant::now();
    let source = constant_session();
    let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 64);
    for milliseconds in [0, 15, 95, 250] {
        let elapsed = Duration::from_millis(milliseconds);
        let mut samples = [0.75_f32; 128];
        output.render(
            &mut samples,
            epoch + elapsed,
            steady_timestamp(elapsed, Duration::from_millis(20)),
        );
        assert_eq!(samples, [0.0; 128]);
        assert!(output.state.clock().is_none());
        assert!(output.first_playback.is_none());
        assert_eq!(output.next_frame, 0);
        assert_eq!(output.state.fault.load(Ordering::Acquire), 0);
    }
    output.state.admitted.store(true, Ordering::Release);
    output.render::<f32>(
        &mut [],
        epoch + Duration::from_millis(260),
        steady_timestamp(Duration::from_millis(260), Duration::ZERO),
    );
    assert!(output.state.clock().is_none());
    assert_eq!(source.frame().time, 0.0);
}

#[test]
fn first_audible_buffer_anchors_existing_clock_at_predicted_playback() {
    let epoch = Instant::now();
    let source = constant_session();
    let publication = source.publication_context();
    let mut output = callback(&source, epoch, 0.375, AudioMixFormat::STEREO_48_KHZ, 64);
    output.state.admitted.store(true, Ordering::Release);
    let now = epoch + Duration::from_millis(75);
    let mut samples = [0.0_f32; 128];
    output.render(
        &mut samples,
        now,
        steady_timestamp(Duration::ZERO, Duration::from_millis(25)),
    );
    let mut expected = [0.25; 128];
    apply_start_attack(&mut expected, AudioMixFormat::STEREO_48_KHZ, 0);
    assert_eq!(samples, expected);
    let clock = output.state.clock().unwrap();
    assert_eq!(clock.wall_origin, now + Duration::from_millis(25));
    assert_eq!(clock.scene_origin, 0.375);
    assert_eq!(clock.scene_time_at(now), 0.375);
    assert_eq!(
        clock.scene_time_at(clock.wall_origin + Duration::from_millis(125)),
        0.5
    );
    assert_eq!(output.next_frame, 64);
    assert_eq!(source.frame().time, 0.0);
    assert_eq!(source.publication_context(), publication);
}

#[test]
fn variable_device_buffers_and_callback_jitter_preserve_one_contiguous_pcm_grid() {
    let source = session(vec![(
        pcm(24_000, 2, &[0.1, -0.2, 0.3, -0.4, 0.2, -0.1]),
        AudioClipOptions {
            looped: true,
            duration: Some(2.0),
            fade_in: 0.015,
            fade_out: 0.025,
            ..Default::default()
        },
    )]);
    let epoch = Instant::now();
    let mut output = callback(&source, epoch, 0.0075, AudioMixFormat::STEREO_48_KHZ, 128);
    output.state.admitted.store(true, Ordering::Release);
    let storage = (output.samples.as_ptr(), output.samples.capacity());
    let mut joined = Vec::new();
    let mut first_frame = 0_u64;
    for (index, frames) in [17_usize, 2_000, 1, 3_713, 1_024, 4_096]
        .into_iter()
        .enumerate()
    {
        let elapsed = Duration::from_secs_f64(first_frame as f64 / 48_000.0);
        let jitter = Duration::from_millis([0, 3, 1, 7, 2, 4][index]);
        let mut samples = vec![0.875; frames * 2];
        let stream_callback = Duration::from_secs(10) + elapsed + jitter;
        let stream_playback = Duration::from_secs(10) + elapsed + Duration::from_millis(50);
        output.render(
            &mut samples,
            epoch + Duration::from_millis(40) + elapsed + jitter,
            timestamp(stream_callback, stream_playback),
        );
        assert_eq!(output.state.fault.load(Ordering::Acquire), 0);
        assert_eq!(
            (output.samples.as_ptr(), output.samples.capacity()),
            storage
        );
        joined.extend(samples);
        first_frame += frames as u64;
    }
    let mut expected = vec![0.0; joined.len()];
    source
        .mix_audio_block(0.0075, 0, AudioMixFormat::STEREO_48_KHZ, &mut expected)
        .unwrap();
    apply_start_attack(&mut expected, AudioMixFormat::STEREO_48_KHZ, 0);
    assert_eq!(joined, expected);
    assert_eq!(output.next_frame, first_frame);
    assert_eq!(
        output.state.clock().unwrap().wall_origin,
        epoch + Duration::from_millis(90)
    );
}

#[test]
fn cropped_native_start_uses_shared_trim_loop_fade_and_silent_gap_semantics() {
    const RATE: u32 = 8_192;
    let source = session(vec![
        (
            pcm(RATE, 1, &[-1.0, 0.25, 0.75, -1.0]),
            AudioClipOptions {
                start_time: 2.0 / f64::from(RATE),
                source_start_frame: 1,
                source_end_frame: Some(3),
                looped: true,
                duration: Some(8.0 / f64::from(RATE)),
                gain: 0.5,
                fade_in: 2.0 / f64::from(RATE),
                fade_out: 2.0 / f64::from(RATE),
            },
        ),
        (
            pcm(RATE, 1, &[0.5; 2]),
            AudioClipOptions {
                start_time: 12.0 / f64::from(RATE),
                ..Default::default()
            },
        ),
    ]);
    let epoch = Instant::now();
    let format = AudioMixFormat::new(RATE * 2, 1).unwrap();
    let origin = 2.5 / f64::from(RATE);
    let mut output = callback(&source, epoch, origin, format, 3);
    output.state.admitted.store(true, Ordering::Release);
    let mut samples = [0.875; 26];
    output.render(
        &mut samples,
        epoch,
        steady_timestamp(Duration::ZERO, Duration::ZERO),
    );
    let mut expected = [0.0; 26];
    source
        .mix_audio_block(origin, 0, format, &mut expected)
        .unwrap();
    assert_eq!(expected[0], 0.0625);
    assert_eq!(&expected[15..19], &[0.0; 4]);
    assert_eq!(&expected[19..23], &[0.5; 4]);
    assert_eq!(&expected[23..], &[0.0; 3]);
    apply_start_attack(&mut expected, format, 0);
    assert_eq!(samples, expected);
}

#[test]
fn device_discontinuity_policy_accepts_fifty_milliseconds_and_rejects_more() {
    for drift in [
        Duration::from_millis(50),
        Duration::from_millis(50) + Duration::from_nanos(1),
    ] {
        let source = constant_session();
        let epoch = Instant::now();
        let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 480);
        output.state.admitted.store(true, Ordering::Release);
        let mut samples = [0.0; 960];
        let latency = Duration::from_millis(20);
        output.render(
            &mut samples,
            epoch,
            steady_timestamp(Duration::ZERO, latency),
        );
        let elapsed = Duration::from_millis(10) + drift;
        output.render(
            &mut samples,
            epoch + elapsed,
            steady_timestamp(elapsed, latency),
        );
        if drift == MAX_CLOCK_ERROR {
            let mut expected = [0.25; 960];
            apply_start_attack(&mut expected, AudioMixFormat::STEREO_48_KHZ, 480);
            assert_eq!(samples, expected);
            assert_eq!(output.state.fault.load(Ordering::Acquire), 0);
            assert_eq!(output.next_frame, 960);
        } else {
            assert_eq!(samples, [0.0; 960]);
            assert_eq!(
                output.state.fault.load(Ordering::Acquire),
                AudioFault::Discontinuity as u8
            );
            assert_eq!(output.next_frame, 480);
        }
    }
}

#[test]
fn backward_playback_timestamps_and_wall_observations_stop_output() {
    for backward_device in [true, false] {
        let source = constant_session();
        let epoch = Instant::now();
        let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 480);
        output.state.admitted.store(true, Ordering::Release);
        let mut samples = [0.0; 960];
        output.render(
            &mut samples,
            epoch,
            steady_timestamp(Duration::ZERO, Duration::from_millis(20)),
        );
        let stamp = if backward_device {
            timestamp(Duration::from_millis(9_999), Duration::from_millis(10_019))
        } else {
            steady_timestamp(Duration::from_millis(10), Duration::ZERO)
        };
        output.render(&mut samples, epoch, stamp);
        assert_eq!(samples, [0.0; 960]);
        assert_eq!(
            output.state.fault.load(Ordering::Acquire),
            AudioFault::Discontinuity as u8
        );
    }
}

#[test]
fn invalid_or_excess_latency_cannot_publish_a_playback_anchor() {
    let source = constant_session();
    let epoch = Instant::now();
    for stamp in [
        timestamp(Duration::from_secs(10), Duration::from_millis(9_999)),
        steady_timestamp(Duration::ZERO, MAX_OUTPUT_LATENCY + Duration::from_nanos(1)),
    ] {
        let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 4);
        output.state.admitted.store(true, Ordering::Release);
        let mut samples = [0.75; 8];
        output.render(&mut samples, epoch, stamp);
        assert_eq!(samples, [0.0; 8]);
        assert!(output.state.clock().is_none());
        assert_eq!(
            output.state.fault.load(Ordering::Acquire),
            AudioFault::Timestamp as u8
        );
    }
}

#[test]
fn device_buffer_guards_reject_incomplete_and_oversized_blocks_silently() {
    let source = constant_session();
    let epoch = Instant::now();
    for sample_count in [3, (MAX_AUDIO_MIX_BLOCK_FRAMES + 1) * 2] {
        let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 8);
        output.state.admitted.store(true, Ordering::Release);
        let mut samples = vec![0.75; sample_count];
        output.render(
            &mut samples,
            epoch,
            steady_timestamp(Duration::ZERO, Duration::ZERO),
        );
        assert!(samples.iter().all(|sample| *sample == 0.0));
        assert!(output.state.clock().is_none());
        assert_eq!(
            output.state.fault.load(Ordering::Acquire),
            AudioFault::Buffer as u8
        );
    }
    let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 8);
    output.state.admitted.store(true, Ordering::Release);
    let mut maximum = vec![0.0; MAX_AUDIO_MIX_BLOCK_FRAMES * 2];
    output.render(
        &mut maximum,
        epoch,
        steady_timestamp(Duration::ZERO, Duration::ZERO),
    );
    let mut expected = vec![0.25; maximum.len()];
    apply_start_attack(&mut expected, AudioMixFormat::STEREO_48_KHZ, 0);
    assert_eq!(maximum, expected);
    assert_eq!(output.state.fault.load(Ordering::Acquire), 0);
}

#[test]
fn a_later_mix_chunk_failure_erases_the_entire_device_buffer() {
    let source = session(vec![(
        pcm(48_000, 1, &[0.25, 0.5, 0.75, 2.0]),
        AudioClipOptions::default(),
    )]);
    let epoch = Instant::now();
    let mut output = callback(
        &source,
        epoch,
        0.0,
        AudioMixFormat::new(48_000, 1).unwrap(),
        2,
    );
    output.state.admitted.store(true, Ordering::Release);
    let mut samples = [0.875; 4];
    output.render(
        &mut samples,
        epoch,
        steady_timestamp(Duration::ZERO, Duration::ZERO),
    );
    assert_eq!(samples, [0.0; 4]);
    assert_eq!(
        output.state.fault.load(Ordering::Acquire),
        AudioFault::Mix as u8
    );
}

#[test]
fn integer_pcm_uses_unsigned_equilibrium_and_never_wraps_positive_full_scale() {
    let source = session(vec![(
        pcm(48_000, 1, &[-1.0, 0.0, 1.0]),
        AudioClipOptions {
            start_time: 0.040,
            ..Default::default()
        },
    )]);
    let epoch = Instant::now();
    let format = AudioMixFormat::new(48_000, 1).unwrap();
    let mut unsigned = callback(&source, epoch, 0.0, format, 3);
    let mut samples = vec![0_u16; 1_923];
    unsigned.render(
        &mut samples,
        epoch,
        steady_timestamp(Duration::ZERO, Duration::ZERO),
    );
    assert!(samples.iter().all(|sample| *sample == 32_768));
    unsigned.state.admitted.store(true, Ordering::Release);
    unsigned.render(
        &mut samples,
        epoch,
        steady_timestamp(Duration::ZERO, Duration::ZERO),
    );
    assert!(samples[..1_920].iter().all(|sample| *sample == 32_768));
    assert_eq!(&samples[1_920..], &[0, 32_768, 65_535]);
    unsigned.state.retired.store(true, Ordering::Release);
    unsigned.render(
        &mut samples,
        epoch,
        steady_timestamp(Duration::ZERO, Duration::ZERO),
    );
    assert!(samples.iter().all(|sample| *sample == 32_768));

    let mut signed = callback(&source, epoch, 0.0, format, 3);
    signed.state.admitted.store(true, Ordering::Release);
    let mut samples = vec![0_i16; 1_923];
    signed.render(
        &mut samples,
        epoch,
        steady_timestamp(Duration::ZERO, Duration::ZERO),
    );
    assert!(samples[..1_920].iter().all(|sample| *sample == 0));
    assert_eq!(&samples[1_920..], &[i16::MIN, 0, i16::MAX]);
}

#[test]
fn retirement_and_device_faults_prevent_stale_output_and_preserve_first_fault() {
    for fault in [None, Some(AudioFault::Device), Some(AudioFault::Underrun)] {
        let source = constant_session();
        let epoch = Instant::now();
        let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 64);
        output.state.admitted.store(true, Ordering::Release);
        let mut samples = [0.0; 128];
        output.render(
            &mut samples,
            epoch,
            steady_timestamp(Duration::ZERO, Duration::ZERO),
        );
        let mut expected = [0.25; 128];
        apply_start_attack(&mut expected, AudioMixFormat::STEREO_48_KHZ, 0);
        assert_eq!(samples, expected);
        if let Some(fault) = fault {
            output.state.fail(fault);
            output.state.fail(AudioFault::Mix);
            assert_eq!(output.state.fault.load(Ordering::Acquire), fault as u8);
            assert!(matches!(
                output.state.check(epoch),
                Err(NativeHostError::Audio(_))
            ));
        } else {
            output.state.retired.store(true, Ordering::Release);
        }
        output.render(
            &mut samples,
            epoch + Duration::from_millis(10),
            steady_timestamp(Duration::from_millis(10), Duration::ZERO),
        );
        assert_eq!(samples, [0.0; 128]);
        assert_eq!(output.next_frame, 64);
    }
}

#[test]
fn a_replacement_stream_has_fresh_admission_and_anchor_state() {
    let source = constant_session();
    let epoch = Instant::now();
    let mut old = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 4);
    old.state.admitted.store(true, Ordering::Release);
    let mut samples = [0.0; 8];
    old.render(
        &mut samples,
        epoch,
        steady_timestamp(Duration::ZERO, Duration::from_millis(20)),
    );
    old.state.retired.store(true, Ordering::Release);
    let replacement_epoch = epoch + Duration::from_secs(1);
    let mut replacement = callback(
        &source,
        replacement_epoch,
        0.625,
        AudioMixFormat::STEREO_48_KHZ,
        4,
    );
    replacement.render(
        &mut samples,
        replacement_epoch,
        steady_timestamp(Duration::ZERO, Duration::ZERO),
    );
    assert_eq!(samples, [0.0; 8]);
    assert!(replacement.state.clock().is_none());
    replacement.state.admitted.store(true, Ordering::Release);
    replacement.render(
        &mut samples,
        replacement_epoch,
        steady_timestamp(Duration::ZERO, Duration::from_millis(30)),
    );
    let clock = replacement.state.clock().unwrap();
    assert_eq!(clock.scene_origin, 0.625);
    assert_eq!(
        clock.wall_origin,
        replacement_epoch + Duration::from_millis(30)
    );
    assert_ne!(clock.wall_origin, old.state.clock().unwrap().wall_origin);
    assert_eq!(replacement.next_frame, 4);
}

#[test]
fn drain_and_callback_timeout_follow_audible_wall_time() {
    let epoch = Instant::now();
    let state = OutputState::new(epoch, 0.375, 1.0);
    assert!(!state.drained(1.0, epoch + Duration::from_secs(10)));
    let anchor = epoch + Duration::from_millis(50);
    state
        .anchor
        .store(state.encode(anchor).unwrap(), Ordering::Release);
    let end = anchor + Duration::from_millis(625) + MAX_CLOCK_ERROR;
    assert!(!state.drained(1.0, end - Duration::from_nanos(1)));
    assert!(state.drained(1.0, end));
    assert!(state.check(epoch + CALLBACK_TIMEOUT).is_ok());
    assert!(matches!(
        state.check(epoch + CALLBACK_TIMEOUT + Duration::from_nanos(1)),
        Err(NativeHostError::Audio(_))
    ));
    let heartbeat = epoch + Duration::from_secs(1);
    state
        .heartbeat
        .store(state.encode(heartbeat).unwrap(), Ordering::Release);
    assert!(state.check(heartbeat + CALLBACK_TIMEOUT).is_ok());
}

#[test]
fn finite_audio_profile_passes_before_device_access_and_limits_clip_work() {
    let source = StaticExecutionSource::new(constant_session(), RustHostCallbackTable::new());
    admit_profile(&source).unwrap();
    let error = crate::run_with_config(
        constant_session(),
        NativeViewportConfig {
            width: 0,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(
        matches!(error, NativeHostError::Platform(message) if message == "initial viewport dimensions must be positive")
    );

    for clip_count in [MAX_CLIPS, MAX_CLIPS + 1] {
        let resource = pcm(48_000, 1, &[0.001]);
        let source = StaticExecutionSource::new(
            session(
                (0..clip_count)
                    .map(|_| (resource.clone(), AudioClipOptions::default()))
                    .collect(),
            ),
            RustHostCallbackTable::new(),
        );
        if clip_count == MAX_CLIPS {
            admit_profile(&source).unwrap();
        } else {
            assert!(
                matches!(admit_profile(&source), Err(NativeHostError::Audio(message)) if message.contains("256"))
            );
        }
    }
}

#[test]
fn required_callbacks_and_unfinished_sources_reject_before_device_access() {
    let mut scene = Scene::new();
    let marker = scene.circle(0.5).unwrap();
    scene.add(&marker).unwrap();
    scene
        .audio(pcm(48_000, 1, &[0.25]), AudioClipOptions::default())
        .unwrap();
    let mut transaction = noon_core::SemanticMutationTransaction::new();
    transaction.add_updater(
        marker.node_id(),
        noon_core::HostCallbackId::new(1),
        0.0,
        None,
    );
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let session = scene.execution_session().unwrap();
    assert!(session.has_required_callbacks());
    assert!(
        matches!(crate::run_with_config(session, NativeViewportConfig { width: 0, ..Default::default() }), Err(NativeHostError::Audio(message)) if message.contains("required callbacks"))
    );

    struct Waiting;
    impl noon::LiveContinuation for Waiting {
        type Error = noon::LiveSessionError;
        fn resume(
            &mut self,
            live: &mut noon::LiveSession<'_>,
        ) -> Result<noon::ContinuationStep, Self::Error> {
            live.wait_segment(1.0).map(noon::ContinuationStep::Await)
        }
    }
    let source = LiveProgramExecutionSource::new(
        Scene::new().into_live_program(Waiting).unwrap(),
        RustHostCallbackTable::new(),
    )
    .unwrap();
    assert!(source.source_active());
    assert!(
        matches!(admit_profile(&source), Err(NativeHostError::Audio(message)) if message.contains("source continuations"))
    );
}

#[cfg(target_os = "linux")]
fn device_fixture() -> ExecutionSession {
    use noon_core::{
        AnimationOptions, CompositionTimeMap, RateFunction, SemanticAnimationCompositionKind,
        SemanticAnimationIntent, SemanticMutationTransaction, SemanticObjectTrackProperty,
        SemanticObjectTrackValues, SemanticVec3, TrackTiming,
    };

    let mut scene = Scene::new();
    let mut marker = scene.circle(0.25).unwrap();
    marker.set_translation(-1.5, 0.0).unwrap();
    marker.set_fill(0.2, 0.8, 0.4, 1.0).unwrap();
    scene.add(&marker).unwrap();
    for (start, frames, right_frequency) in [(0.1, 9_600, 1_499.0), (0.55, 12_000, 449.0)] {
        let mut samples = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            let phase = std::f64::consts::TAU * frame as f64 / 48_000.0;
            samples.push((phase * 997.0).sin() as f32);
            samples.push((phase * right_frequency).sin() as f32);
        }
        scene
            .audio(
                AudioResource::from_interleaved(48_000, 2, samples, frames * 8).unwrap(),
                AudioClipOptions {
                    start_time: start,
                    gain: 0.15,
                    fade_in: 0.005,
                    fade_out: 0.005,
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        marker.node_id(),
        SemanticObjectTrackProperty::Position,
        SemanticObjectTrackValues::Vec3 {
            from: SemanticVec3::new(-1.5, 0.0, 0.0),
            to: SemanticVec3::new(1.5, 0.0, 0.0),
        },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let result = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let animation = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Parallel,
                children: vec![result.resolve(track).unwrap()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    scene
        .execution_session_with_animation_root(&animation)
        .unwrap()
}

#[cfg(target_os = "linux")]
fn proof_directory() -> std::path::PathBuf {
    let directory = std::env::var_os("NOON_NATIVE_AUDIO_PROOF_DIR")
        .map(std::path::PathBuf::from)
        .expect("set NOON_NATIVE_AUDIO_PROOF_DIR to the native audio qualification directory");
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[cfg(target_os = "linux")]
fn assert_selected_pulse_plugin_loaded() {
    if let Some(path) = std::env::var_os("NOON_NATIVE_AUDIO_PULSE_PLUGIN") {
        let path = std::path::PathBuf::from(path).canonicalize().unwrap();
        let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
        assert!(
            maps.lines()
                .any(|line| line.ends_with(path.to_str().unwrap())),
            "the selected private Pulse PCM plugin was not loaded: {}",
            path.display()
        );
        eprintln!("native audio fixture: loaded PCM plugin {}", path.display());
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires X11, WGPU and a controlled default output device; selected by native audio qualification"]
fn native_audio_device_smoke() {
    use std::io::Write;
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::platform::x11::EventLoopBuilderExtX11;
    use winit::window::WindowId;

    // A software X11 surface has no physical refresh cadence. Bound both
    // redraw requests and delivery so this PCM proof has controlled display
    // load, matching the separate native-controls device fixture.
    const PICTURE_INTERVAL: Duration = Duration::from_nanos(16_666_667);

    struct Harness {
        app: NativeApp,
        state: Option<Arc<OutputState>>,
        started: Instant,
        host_ready: Option<Instant>,
        silent_preparation: Duration,
        preparation_wait: Duration,
        silent_heartbeat: Option<Instant>,
        first_picture: Option<Instant>,
        last_picture: Option<Instant>,
        final_picture: Option<Instant>,
        presents: usize,
        max_display_lag: f64,
    }

    impl Harness {
        fn picture_due(&self, now: Instant) -> bool {
            self.app.publication_pending()
                || self
                    .last_picture
                    .is_none_or(|last| now.duration_since(last) >= PICTURE_INTERVAL)
        }
    }

    impl ApplicationHandler for Harness {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            self.app.resumed(event_loop);
            if self.app.error.is_some() {
                return;
            }
            self.host_ready = Some(Instant::now());
            assert_selected_pulse_plugin_loaded();
            eprintln!(
                "native audio fixture: host ready after {:.3}s; output is still unadmitted",
                self.started.elapsed().as_secs_f64()
            );
            assert!(self.app.pointer.presented.is_none());
            let output = self
                .app
                .audio_output
                .as_ref()
                .expect("native audio stream prepared");
            self.state = Some(output.state.clone());
            let begin = Instant::now();
            // Deliberately withhold the first picture while a real output stream
            // runs. A startup callback followed by an idle device does not prove
            // audible silent preparation: wait for paced callbacks instead of a
            // fixed wall delay. All waiting stays on this test host thread.
            let mut previous = None;
            let mut paced_start = None;
            loop {
                let now = Instant::now();
                output.state.check(now).unwrap();
                assert!(
                    begin.elapsed() < Duration::from_secs(5),
                    "device did not sustain paced silent callbacks before picture admission"
                );
                assert!(!output.state.admitted.load(Ordering::Acquire));
                assert!(output.clock().is_none());
                if let Some(heartbeat) = output
                    .state
                    .decode(output.state.heartbeat.load(Ordering::Acquire))
                {
                    if previous != Some(heartbeat) {
                        if previous.is_none_or(|last| {
                            heartbeat.saturating_duration_since(last) > Duration::from_millis(100)
                        }) {
                            paced_start = Some(heartbeat);
                        }
                        previous = Some(heartbeat);
                        let span = heartbeat.saturating_duration_since(paced_start.unwrap());
                        if span >= Duration::from_millis(250)
                            && now.saturating_duration_since(heartbeat) < Duration::from_millis(50)
                        {
                            self.silent_preparation = span;
                            self.silent_heartbeat = Some(heartbeat);
                            break;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            self.preparation_wait = begin.elapsed();
            assert!(!output.state.admitted.load(Ordering::Acquire));
            assert!(output.clock().is_none());
            assert_eq!(self.app.execution.frame_time(), 0.0);
            assert!(self.silent_heartbeat.unwrap() >= begin);
            output.state.check(Instant::now()).unwrap();
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            let redraw = matches!(event, WindowEvent::RedrawRequested);
            if redraw && !self.picture_due(Instant::now()) {
                return;
            }
            self.app.window_event(event_loop, id, event);
            if self.app.error.is_some() || !redraw {
                return;
            }
            let Some(time) = self.app.presented_frame_time else {
                return;
            };
            let now = Instant::now();
            if self.first_picture.is_none() {
                eprintln!(
                    "native audio fixture: first picture after {:.3}s at scene time {time}",
                    now.duration_since(self.started).as_secs_f64()
                );
            }
            self.first_picture.get_or_insert(now);
            self.last_picture = Some(now);
            self.presents += 1;
            if let Some(clock) = self.state.as_ref().and_then(|state| state.clock()) {
                let lag = clock.scene_time_at(now).min(1.0) - time;
                assert!(
                    lag >= -0.001,
                    "picture advanced ahead of the admitted audio clock: {lag}"
                );
                self.max_display_lag = self.max_display_lag.max(lag);
            }
            if time == 1.0 {
                self.final_picture.get_or_insert(now);
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            let now = Instant::now();
            if self.picture_due(now) {
                self.app.about_to_wait(event_loop);
            } else if let Err(error) = self.app.check_audio(now) {
                self.app.fail(event_loop, error);
            }
            if self.app.error.is_some() {
                return;
            }
            // Cold window/GPU initialization precedes playback admission. Its
            // time must not consume the independent finite-playback deadline.
            // The shell harness also bounds the entire process at 60 seconds.
            let now = Instant::now();
            let (phase, elapsed, budget) = self.first_picture.map_or(
                (
                    "startup",
                    now.duration_since(self.started),
                    Duration::from_secs(45),
                ),
                |first| {
                    (
                        "playback",
                        now.duration_since(first),
                        Duration::from_secs(15),
                    )
                },
            );
            assert!(
                elapsed < budget,
                "native audio fixture {phase} deadline exceeded: elapsed={elapsed:?}, runtime={}, presented={:?}, pictures={}, admitted={}, fault={}",
                self.app.execution.frame_time(),
                self.app.presented_frame_time,
                self.presents,
                self.state.as_ref().is_some_and(|state| state.admitted.load(Ordering::Acquire)),
                self.state.as_ref().map_or(0, |state| state.fault.load(Ordering::Acquire)),
            );
            if let Some(presented) = self.final_picture {
                let deadline = presented + Duration::from_millis(150);
                if Instant::now() >= deadline {
                    assert!(
                        self.app.audio_output.is_none(),
                        "device stream must drain before the fixture exits"
                    );
                    event_loop.exit();
                } else {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                }
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(5),
                ));
            }
        }

        fn exiting(&mut self, event_loop: &ActiveEventLoop) {
            self.app.exiting(event_loop);
        }
    }

    let directory = proof_directory();
    let session = device_fixture();
    assert_eq!(session.finite_timeline_end_time(), Some(1.0));
    let mut expected =
        std::io::BufWriter::new(std::fs::File::create(directory.join("expected.f32")).unwrap());
    let mut block = [0.0_f32; 1_024 * 2];
    let mut first_frame = 0_u64;
    while first_frame < 48_000 {
        let frames = (48_000 - first_frame).min(1_024) as usize;
        let samples = &mut block[..frames * 2];
        session
            .mix_audio_block(0.0, first_frame, AudioMixFormat::STEREO_48_KHZ, samples)
            .unwrap();
        for sample in samples {
            expected.write_all(&sample.to_le_bytes()).unwrap();
        }
        first_frame += frames as u64;
    }
    expected.flush().unwrap();
    let source = StaticExecutionSource::new(session, RustHostCallbackTable::new());
    admit_profile(&source).unwrap();
    let mut harness = Harness {
        app: NativeApp::from_source(
            Box::new(source),
            NativeViewportConfig {
                title: "Noon native audio synchronization proof".into(),
                width: 320,
                height: 180,
                ..Default::default()
            },
        ),
        state: None,
        started: Instant::now(),
        host_ready: None,
        silent_preparation: Duration::ZERO,
        preparation_wait: Duration::ZERO,
        silent_heartbeat: None,
        first_picture: None,
        last_picture: None,
        final_picture: None,
        presents: 0,
        max_display_lag: 0.0,
    };
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    // Fault injection for the real-device regression: model slow cold platform
    // startup before the host can prepare or admit audio. Default runs add no delay.
    let cold_start_delay = std::env::var("NOON_NATIVE_AUDIO_COLD_START_DELAY_MS")
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .expect("cold-start delay must be milliseconds")
        })
        .unwrap_or(0);
    assert!(
        cold_start_delay <= 25_000,
        "cold-start injection exceeds its bound"
    );
    std::thread::sleep(Duration::from_millis(cold_start_delay));
    event_loop.run_app(&mut harness).unwrap();
    assert!(harness.app.error.is_none(), "{:?}", harness.app.error);
    assert_eq!(harness.app.presented_frame_time, Some(1.0));
    assert_eq!(harness.app.execution.frame_time(), 1.0);
    assert!(harness.app.last_geometry_draw_calls > 0);
    assert!(harness.presents > 2);
    assert!(harness.silent_preparation >= Duration::from_millis(150));
    let state = harness.state.unwrap();
    let clock = state
        .clock()
        .expect("real device supplied the native A/V anchor");
    assert_eq!(clock.scene_origin, 0.0);
    assert_eq!(state.fault.load(Ordering::Acquire), 0);
    assert!(state.retired.load(Ordering::Acquire));
    assert!(state.drained(0.8, Instant::now()));
    let seconds = |time: Instant| time.saturating_duration_since(state.epoch).as_secs_f64();
    let report = format!(
        concat!(
            "{{\n  \"sample_rate\": 48000,\n  \"channels\": 2,\n  \"duration\": 1.0,\n",
            "  \"prepared_silent_seconds\": {:.9},\n  \"displayed_end_time\": 1.0,\n",
            "  \"preparation_wait_seconds\": {:.9},\n",
            "  \"host_setup_seconds\": {:.9},\n",
            "  \"injected_cold_start_milliseconds\": {},\n",
            "  \"silent_callback_seconds_from_stream_epoch\": {:.9},\n",
            "  \"first_picture_seconds_from_stream_epoch\": {:.9},\n",
            "  \"audio_anchor_seconds_from_stream_epoch\": {:.9},\n",
            "  \"final_picture_seconds_from_stream_epoch\": {:.9},\n",
            "  \"displayed_frames\": {},\n  \"max_display_lag_seconds\": {:.9},\n",
            "  \"fixture_refresh_hz\": 60,\n",
            "  \"audio_drained\": true,\n  \"stream_retired\": true\n}}\n"
        ),
        harness.silent_preparation.as_secs_f64(),
        harness.preparation_wait.as_secs_f64(),
        harness
            .host_ready
            .unwrap()
            .duration_since(harness.started)
            .as_secs_f64(),
        cold_start_delay,
        seconds(harness.silent_heartbeat.unwrap()),
        seconds(harness.first_picture.unwrap()),
        seconds(clock.wall_origin),
        seconds(harness.final_picture.unwrap()),
        harness.presents,
        harness.max_display_lag,
    );
    std::fs::write(directory.join("report.json"), report).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires a private output daemon that the qualification harness terminates"]
fn native_audio_device_loss_smoke() {
    let directory = proof_directory();
    // Simulate a window thread that does not wake until its short audio range
    // has already drained. The actual CPAL callback keeps running, while the
    // native Runtime and its wall clock remain unobserved at their initial state.
    let mut late = NativeApp::new(
        session(vec![(
            pcm(48_000, 1, &[0.05]),
            AudioClipOptions {
                looped: true,
                duration: Some(0.2),
                ..Default::default()
            },
        )]),
        NativeViewportConfig::default(),
    );
    late.prepare_audio().unwrap();
    assert_selected_pulse_plugin_loaded();
    let short_output = late.audio_output.as_ref().unwrap();
    let short_state = short_output.state.clone();
    short_output.admit();
    let started = Instant::now();
    let resumed_at = loop {
        let now = Instant::now();
        short_state.check(now).unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "short native audio stream did not reach its audible endpoint"
        );
        assert!(late.realtime_clock.is_none());
        assert_eq!(late.execution.frame_time(), 0.0);
        if short_state.drained(0.2, now) {
            break now;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let original = short_state.clock().unwrap();
    late.check_audio(resumed_at).unwrap();
    assert!(late.audio_output.is_none());
    assert!(short_state.retired.load(Ordering::Acquire));
    let adopted = late
        .realtime_clock
        .expect("late host wake must retain the device anchor before retiring its stream");
    let late_host_wake_preserved_anchor = adopted.wall_origin == original.wall_origin
        && adopted.scene_origin == original.scene_origin;
    assert!(late_host_wake_preserved_anchor);
    late.advance_realtime_timeline(resumed_at).unwrap();
    let late_host_wake_end_time = late.execution.frame_time();
    assert_eq!(late_host_wake_end_time, 0.2);
    drop(late);

    let source = session(vec![(
        pcm(48_000, 1, &[0.05]),
        AudioClipOptions {
            looped: true,
            duration: Some(30.0),
            ..Default::default()
        },
    )]);
    let output = NativeAudioOutput::prepare(&source, 1.0).unwrap();
    let state = output.state.clone();
    output.admit();
    let startup = Instant::now();
    while output.clock().is_none() {
        state.check(Instant::now()).unwrap();
        assert!(
            startup.elapsed() < Duration::from_secs(5),
            "audio device never supplied a clock anchor"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    state.check(Instant::now()).unwrap();
    std::fs::write(
        directory.join("device-loss-ready.txt"),
        "active native output; terminate only the private test daemon\n",
    )
    .unwrap();
    let began = Instant::now();
    let error = loop {
        if let Err(error) = state.check(Instant::now()) {
            break error.to_string();
        }
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "device loss did not stop synchronized playback"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let detection_seconds = began.elapsed().as_secs_f64();
    let fault_code = state.fault.load(Ordering::Acquire);
    drop(output);
    assert!(state.retired.load(Ordering::Acquire));
    let report = format!(
        "{{\n  \"error_observed\": true,\n  \"fault_code\": {fault_code},\n  \"error\": {error:?},\n  \"detection_seconds\": {detection_seconds:.9},\n  \"stream_dropped\": true,\n  \"late_host_wake_preserved_anchor\": {late_host_wake_preserved_anchor},\n  \"late_host_wake_end_time\": {late_host_wake_end_time:.9}\n}}\n"
    );
    std::fs::write(directory.join("device-loss-report.json"), report).unwrap();
}
