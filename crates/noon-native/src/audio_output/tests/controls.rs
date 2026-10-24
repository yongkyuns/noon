use super::*;

#[test]
fn device_period_tracks_twenty_milliseconds_with_supported_range_clamps() {
    let range = cpal::SupportedBufferSize::Range {
        min: 1,
        max: 32_768,
    };
    for (rate, frames) in [
        (8_000, 160),
        (8_192, 164),
        (44_100, 882),
        (47_999, 960),
        (48_000, 960),
        (96_000, 1_920),
        (192_000, 3_840),
    ] {
        assert_eq!(
            device_buffer_size(rate, range),
            cpal::BufferSize::Fixed(frames)
        );
        assert!(frames as usize <= MAX_AUDIO_MIX_BLOCK_FRAMES);
        assert!(f64::from(frames) / f64::from(rate) >= 0.020);
        assert!(f64::from(frames - 1) / f64::from(rate) < 0.020);
    }
    for (min, max, expected) in [(2_048, 4_096, 2_048), (64, 256, 256), (512, 512, 512)] {
        assert_eq!(
            device_buffer_size(48_000, cpal::SupportedBufferSize::Range { min, max }),
            cpal::BufferSize::Fixed(expected),
        );
    }
    assert_eq!(
        device_buffer_size(48_000, cpal::SupportedBufferSize::Unknown),
        cpal::BufferSize::Default,
    );
}

fn render_frames(output: &mut OutputCallback, frames: usize) -> Vec<f32> {
    let elapsed = Duration::from_secs_f64(
        output.next_frame as f64 / f64::from(output.mixer.format().sample_rate()),
    );
    let mut samples = vec![0.875; frames * usize::from(output.mixer.format().channels())];
    output.render(
        &mut samples,
        output.state.epoch + elapsed,
        steady_timestamp(elapsed, Duration::from_millis(20)),
    );
    assert_eq!(output.state.fault.load(Ordering::Acquire), 0);
    samples
}

fn set_gain(output: &OutputCallback, gain: f64) {
    validate_gain(gain).unwrap();
    output
        .state
        .viewer_gain
        .store(gain.to_bits(), Ordering::Release);
}

fn expected_segment(
    source: &ExecutionSession,
    first_frame: u64,
    frames: usize,
    from: f64,
    to: f64,
    ramp_frames: usize,
) -> Vec<f32> {
    let mut expected = vec![0.0; frames * 2];
    source
        .mix_audio_block(
            0.0,
            first_frame,
            AudioMixFormat::STEREO_48_KHZ,
            &mut expected,
        )
        .unwrap();
    apply_start_attack(&mut expected, AudioMixFormat::STEREO_48_KHZ, first_frame);
    for (offset, pair) in expected.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let gain = if offset >= ramp_frames {
            to
        } else {
            from + (to - from) * offset as f64 / ramp_frames as f64
        };
        for sample in pair {
            *sample = (f64::from(*sample) * gain) as f32;
        }
    }
    expected
}

#[test]
fn viewer_volume_retargets_continuously_without_restarting_or_mutating_source() {
    let source = session(vec![(
        pcm(
            48_000,
            2,
            &[
                0.25, -0.5, -0.25, 0.5, 0.5, -0.25, 0.125, -0.375, -0.375, 0.125, 0.75, -0.625,
                -0.125, 0.625,
            ],
        ),
        AudioClipOptions {
            looped: true,
            duration: Some(2.0),
            ..Default::default()
        },
    )]);
    let publication = source.publication_context();
    let epoch = Instant::now();
    let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 73);
    output.state.admitted.store(true, Ordering::Release);
    render_frames(&mut output, 2_400);
    let anchor = output.state.clock().unwrap();
    let storage = (output.samples.as_ptr(), output.samples.capacity());

    set_gain(&output, 0.0);
    assert_eq!(
        render_frames(&mut output, 480),
        expected_segment(&source, 2_400, 480, 1.0, 0.0, 960)
    );
    // Halfway through muting, reverse from the instantaneous 0.5 value.
    set_gain(&output, 0.8);
    assert_eq!(
        render_frames(&mut output, 960),
        expected_segment(&source, 2_880, 960, 0.5, 0.8, 960)
    );
    set_gain(&output, 0.0);
    assert_eq!(
        render_frames(&mut output, 960),
        expected_segment(&source, 3_840, 960, 0.8, 0.0, 960)
    );
    assert_eq!(render_frames(&mut output, 480), vec![0.0; 960]);
    // Unmuting continues at source frame5,280; it never replays frame zero.
    set_gain(&output, 0.6);
    assert_eq!(
        render_frames(&mut output, 960),
        expected_segment(&source, 5_280, 960, 0.0, 0.6, 960)
    );
    assert_eq!(
        render_frames(&mut output, 17),
        expected_segment(&source, 6_240, 17, 0.6, 0.6, 0)
    );
    assert_eq!(output.next_frame, 6_257);
    assert_eq!(
        output.state.clock().unwrap().wall_origin,
        anchor.wall_origin
    );
    assert_eq!(
        output.state.clock().unwrap().scene_origin,
        anchor.scene_origin
    );
    assert_eq!(
        (output.samples.as_ptr(), output.samples.capacity()),
        storage
    );
    assert_eq!(source.publication_context(), publication);
    assert_eq!(source.frame().time, 0.0);
}

#[test]
fn initial_mute_preserves_anchor_and_contiguous_source_phase() {
    let source = constant_session();
    let epoch = Instant::now();
    let state = Arc::new(OutputState::new(epoch, 0.375, 0.0));
    let mut output = OutputCallback::new(
        state.clone(),
        source
            .prepare_audio_mix(AudioMixFormat::STEREO_48_KHZ, 64)
            .unwrap(),
    );
    state.admitted.store(true, Ordering::Release);
    assert_eq!(render_frames(&mut output, 2_400), vec![0.0; 4_800]);
    let anchor = state.clock().unwrap();
    assert_eq!(anchor.scene_origin, 0.375);
    assert_eq!(anchor.wall_origin, epoch + Duration::from_millis(20));
    set_gain(&output, 1.0);
    let samples = render_frames(&mut output, 961);
    assert_eq!(samples[0], 0.0);
    assert_eq!(samples[480 * 2], 0.125);
    assert_eq!(samples[960 * 2], 0.25);
    assert_eq!(output.next_frame, 3_361);
    assert_eq!(state.clock().unwrap().scene_origin, 0.375);
}

#[test]
fn a_viewer_change_during_silent_preparation_applies_before_any_audio() {
    for gain in [0.0, 0.25] {
        let source = constant_session();
        let epoch = Instant::now();
        let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 64);
        assert_eq!(render_frames(&mut output, 480), vec![0.0; 960]);
        set_gain(&output, gain);
        output.state.admitted.store(true, Ordering::Release);
        let mut expected = vec![(0.25 * gain) as f32; 4_800];
        apply_start_attack(&mut expected, AudioMixFormat::STEREO_48_KHZ, 0);
        assert_eq!(render_frames(&mut output, 2_400), expected);
        assert!(output.state.clock().is_some());
    }
}

#[test]
fn release_during_attack_keeps_continuity_and_original_drain_deadline() {
    let source = constant_session();
    let epoch = Instant::now();
    let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 79);
    output.state.admitted.store(true, Ordering::Release);
    render_frames(&mut output, 480);
    output.state.release();
    let mut samples = render_frames(&mut output, 480);
    let end = epoch + Duration::from_millis(90);
    let deadline = output
        .state
        .decode(output.state.release_end.load(Ordering::Acquire))
        .unwrap();
    assert_eq!(deadline, end);
    assert_eq!(samples[0], 0.25 * 0.25);
    // Repeated pause/seek requests cannot extend the old release envelope.
    output.state.release();
    samples.extend(render_frames(&mut output, 2_401));
    assert_eq!(
        output
            .state
            .decode(output.state.release_end.load(Ordering::Acquire)),
        Some(end)
    );
    for (frame, pair) in samples.as_chunks::<2>().0.iter().enumerate() {
        let gain = (1.0 - frame as f64 / 2_880.0).max(0.0);
        let expected = (0.25_f64 * 0.25 * gain) as f32;
        assert_eq!(pair, [expected; 2]);
    }
    assert_eq!(&samples[samples.len() - 2..], &[0.0; 2]);
    assert!(!output
        .state
        .released(end + MAX_CLOCK_ERROR - Duration::from_nanos(1)));
    assert!(output.state.released(end + MAX_CLOCK_ERROR));
    assert_eq!(render_frames(&mut output, 480), vec![0.0; 960]);
}

#[test]
fn release_before_admission_cannot_be_revived_by_a_late_admission() {
    let source = constant_session();
    let epoch = Instant::now();
    let mut output = callback(&source, epoch, 0.0, AudioMixFormat::STEREO_48_KHZ, 32);
    output.state.release();
    assert!(output.state.released(epoch));
    // Simulate a picture admission that was already in flight on the host.
    output.state.admitted.store(true, Ordering::Release);
    assert_eq!(render_frames(&mut output, 480), vec![0.0; 960]);
    assert!(output.state.clock().is_none());
    assert_eq!(output.next_frame, 0);
}

#[test]
fn release_duration_never_exceeds_sixty_milliseconds_at_fractional_grid_rates() {
    let source = constant_session();
    for rate in [8_192, 44_100, 47_999, 48_000, 192_000] {
        let epoch = Instant::now();
        let mut output = callback(
            &source,
            epoch,
            0.0,
            AudioMixFormat::new(rate, 2).unwrap(),
            32,
        );
        output.state.admitted.store(true, Ordering::Release);
        render_frames(&mut output, 480);
        let release_begin =
            epoch + Duration::from_secs_f64(480.0 / f64::from(rate)) + Duration::from_millis(20);
        output.state.release();
        render_frames(&mut output, 1);
        let release_end = output
            .state
            .decode(output.state.release_end.load(Ordering::Acquire))
            .unwrap();
        let duration = release_end.duration_since(release_begin);
        assert!(duration <= Duration::from_millis(60));
        assert!(
            Duration::from_millis(60) - duration <= Duration::from_secs_f64(1.0 / f64::from(rate))
        );
    }
}

#[test]
fn identical_control_offsets_produce_identical_pcm_for_different_callback_sizes() {
    fn run(source: &ExecutionSession, block_frames: usize, scratch_frames: usize) -> Vec<f32> {
        let epoch = Instant::now();
        let mut output = callback(
            source,
            epoch,
            0.0,
            AudioMixFormat::STEREO_48_KHZ,
            scratch_frames,
        );
        output.state.admitted.store(true, Ordering::Release);
        let mut samples = Vec::new();
        for (target, gain, release) in [
            (2_400, 1.0, false),
            (2_880, 0.0, false),
            (4_000, 0.7, false),
            (7_200, 0.7, true),
        ] {
            set_gain(&output, gain);
            if release {
                output.state.release();
            }
            while output.next_frame < target {
                let frames = (target - output.next_frame).min(block_frames as u64) as usize;
                samples.extend(render_frames(&mut output, frames));
            }
        }
        samples
    }
    let source = constant_session();
    let expected = run(&source, 4_096, 1_024);
    for (callback, scratch) in [(1, 1), (17, 7), (511, 113), (2_048, 1_024)] {
        assert_eq!(run(&source, callback, scratch), expected);
    }
}

#[test]
fn invalid_viewer_gain_is_rejected_instead_of_reaching_device_math() {
    for gain in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.001, 1.001] {
        assert!(matches!(
            validate_gain(gain),
            Err(NativeHostError::Audio(_))
        ));
    }
    for gain in [0.0, 0.5, 1.0] {
        validate_gain(gain).unwrap();
    }
}
