//! Finite session output keeps authored audio, visual samples and muxed time aligned.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use noon::integration::{
    ExportEndReason, ExportFrameOptions, ExportStop, FrameRate, HostCallbackId,
    SemanticMutationTransaction,
};
use noon::{
    AnimationOptions, AudioClipOptions, AudioResource, ExecutionSession, RateFunction, Scene,
    SemanticAnimationCompositionKind, SemanticVec3,
};
use noon_core::{
    CompositionTimeMap, SemanticAnimationIntent, SemanticObjectTrackProperty,
    SemanticObjectTrackValues, TrackTiming,
};
use noon_export::output::{export_session_file, OutputOptions, SessionFileExportError};
use noon_export::{Backends, CaptureOptions};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 90;
const FRAME_BYTES: usize = WIDTH as usize * HEIGHT as usize * 4;
const AUDIO_RATE: u32 = 48_000;
const SOURCE_RATE: u32 = 24_000;
const SOURCE_START: usize = 13;
const LOOP_FRAMES: usize = 3_072;
const FADE_IN: f64 = 0.025;
const FADE_OUT: f64 = 0.035;
const FREQUENCIES: [f64; 2] = [500.0, 875.0];
const AMPLITUDES: [f64; 2] = [0.32, 0.20];

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory {
    path: PathBuf,
    preserve: bool,
}

impl TestDirectory {
    fn new(name: &str, proof: bool) -> Self {
        let proof_root = proof
            .then(|| std::env::var_os("NOON_OUTPUT_PROOF_DIR"))
            .flatten();
        let preserve = proof_root.is_some();
        let root = proof_root
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "noon-session-{name}-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path, preserve }
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        if !self.preserve {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[derive(Clone, Copy)]
struct Tone {
    start: f64,
    duration: f64,
    gain: f64,
}

impl Tone {
    fn new(start: f64, duration: f64) -> Self {
        Self {
            start,
            duration,
            gain: 0.75,
        }
    }
}

fn fixture(visual_duration: f64, tone: Option<Tone>) -> ExecutionSession {
    let mut scene = Scene::new();
    let mut marker = scene.square(0.8).unwrap();
    marker.set_translation(-2.0, 0.0).unwrap();
    marker.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    marker.set_stroke_width(0.0).unwrap();
    scene.add(&marker).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        marker.node_id(),
        SemanticObjectTrackProperty::Position,
        SemanticObjectTrackValues::Vec3 {
            from: SemanticVec3::new(-2.0, 0.0, 0.0),
            to: SemanticVec3::new(2.0, 0.0, 0.0),
        },
        TrackTiming::new(0.0, visual_duration, RateFunction::Linear),
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
    if let Some(tone) = tone {
        let samples: Vec<f32> = (0..SOURCE_START + LOOP_FRAMES + 17)
            .flat_map(|frame| {
                std::array::from_fn::<_, 2, _>(|channel| {
                    let phase = std::f64::consts::TAU * FREQUENCIES[channel] * frame as f64
                        / f64::from(SOURCE_RATE);
                    (AMPLITUDES[channel] * phase.sin()) as f32
                })
            })
            .collect();
        let resource = AudioResource::from_interleaved(SOURCE_RATE, 2, samples, 64 * 1024).unwrap();
        scene
            .audio(
                resource,
                AudioClipOptions {
                    start_time: tone.start,
                    source_start_frame: SOURCE_START,
                    source_end_frame: Some(SOURCE_START + LOOP_FRAMES),
                    duration: Some(tone.duration),
                    looped: true,
                    gain: tone.gain,
                    fade_in: FADE_IN,
                    fade_out: FADE_OUT,
                },
            )
            .unwrap();
    }
    scene
        .execution_session_with_animation_root(&animation)
        .unwrap()
}

fn frame_options() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 128,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    }
}

fn capture_options() -> CaptureOptions {
    CaptureOptions {
        backends: Backends::VULKAN,
        force_fallback_adapter: true,
        ..CaptureOptions::new(WIDTH, HEIGHT)
    }
}

#[test]
fn invalid_configuration_and_cancellation_do_not_touch_session_or_destination() {
    let directory = TestDirectory::new("preflight", false);
    let path = directory.path.join("existing.mp4");
    fs::write(&path, b"existing user video").unwrap();
    for cancelled in [false, true] {
        let mut session = fixture(0.73, Some(Tone::new(0.125, 0.35)));
        let publication = session.publication_context();
        let mut capture = capture_options();
        if cancelled {
            capture.cancellation.cancel();
        } else {
            capture.width = 0;
        }
        let mut output = OutputOptions::mp4(&path);
        output.overwrite = true;
        output.ffmpeg = directory.path.join("must-not-be-executed");
        let error =
            export_session_file(&mut session, frame_options(), capture, output).unwrap_err();
        if cancelled {
            assert!(matches!(
                error,
                SessionFileExportError::Output(ref error)
                    if error.kind() == io::ErrorKind::Interrupted
            ));
        }
        assert_eq!(session.frame().time, 0.0);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(fs::read(&path).unwrap(), b"existing user video");
        assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 1);
    }
}

#[test]
fn an_advanced_session_is_rejected_before_any_files_are_created() {
    let directory = TestDirectory::new("advanced", false);
    let mut session = fixture(0.73, Some(Tone::new(0.125, 0.35)));
    session.seek(0.1).unwrap();
    let publication = session.publication_context();
    let result = export_session_file(
        &mut session,
        frame_options(),
        capture_options(),
        OutputOptions::mp4(directory.path.join("video.mp4")),
    );
    assert!(matches!(
        result,
        Err(SessionFileExportError::SessionNotAtStart)
    ));
    assert_eq!(session.frame().time, 0.1);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 0);
}

#[test]
fn callback_driven_sessions_do_not_enter_the_immutable_export_profile() {
    let directory = TestDirectory::new("callbacks", false);
    let mut scene = Scene::new();
    let marker = scene.square(0.8).unwrap();
    scene.add(&marker).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(marker.node_id(), HostCallbackId::new(226_300), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let publication = session.publication_context();
    assert!(matches!(
        export_session_file(
            &mut session,
            frame_options(),
            capture_options(),
            OutputOptions::mp4(directory.path.join("video.mp4")),
        ),
        Err(SessionFileExportError::UnsupportedSession)
    ));
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 0);
}

#[test]
fn retained_source_history_does_not_enter_the_immutable_export_profile() {
    let directory = TestDirectory::new("history", false);
    let mut session = fixture(0.73, None);
    session.begin_replay_retention(Default::default()).unwrap();
    let publication = session.publication_context();
    assert!(session.replay_scope_active());
    assert!(matches!(
        export_session_file(
            &mut session,
            frame_options(),
            capture_options(),
            OutputOptions::mp4(directory.path.join("video.mp4")),
        ),
        Err(SessionFileExportError::UnsupportedSession)
    ));
    assert!(session.replay_scope_active());
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 0);
}

#[test]
fn safety_cap_failure_does_not_publish_a_partial_movie() {
    let directory = TestDirectory::new("frame-limit", false);
    let mut session = fixture(0.73, None);
    let publication = session.publication_context();
    assert!(matches!(
        export_session_file(
            &mut session,
            ExportFrameOptions {
                max_frames: 2,
                ..frame_options()
            },
            capture_options(),
            OutputOptions::mp4(directory.path.join("video.mp4")),
        ),
        Err(SessionFileExportError::Sampling(_))
    ));
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 0);
}

#[test]
fn existing_destination_is_preserved_before_encoder_or_gpu_creation() {
    let directory = TestDirectory::new("existing", false);
    let path = directory.path.join("existing.mp4");
    fs::write(&path, b"existing user video").unwrap();
    let mut session = fixture(0.73, Some(Tone::new(0.125, 0.35)));
    let publication = session.publication_context();
    let mut output = OutputOptions::mp4(&path);
    output.ffmpeg = directory.path.join("must-not-be-executed");
    assert!(matches!(
        export_session_file(&mut session, frame_options(), capture_options(), output),
        Err(SessionFileExportError::Output(error)) if error.kind() == io::ErrorKind::AlreadyExists
    ));
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(fs::read(&path).unwrap(), b"existing user video");
    assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 1);
}

fn command_output(command: &mut Command) -> Vec<u8> {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

fn probe(path: &Path, selector: Option<&str>, entries: &str, packets: bool) -> String {
    let mut command = Command::new("ffprobe");
    command.args(["-v", "error"]);
    if let Some(selector) = selector {
        command.args(["-select_streams", selector]);
    }
    if packets {
        command.arg("-show_packets");
    }
    command
        .args([
            "-show_entries",
            entries,
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path);
    String::from_utf8(command_output(&mut command)).unwrap()
}

fn field<'a>(probe: &'a str, name: &str) -> &'a str {
    probe
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key == name).then_some(value)
        })
        .unwrap_or_else(|| panic!("missing {name}: {probe}"))
}

fn packet_times(probe: &str) -> Vec<(i64, u64)> {
    let mut pts = None;
    let mut packets = Vec::new();
    for line in probe.lines() {
        if let Some(value) = line.strip_prefix("pts=") {
            assert!(pts.replace(value.parse::<i64>().unwrap()).is_none());
        }
        if let Some(value) = line.strip_prefix("duration=") {
            packets.push((pts.take().unwrap(), value.parse().unwrap()));
        }
    }
    assert!(pts.is_none());
    packets
}

fn verify_movie_timing(path: &Path, frames: u64, duration: f64, audio: bool) {
    let video = probe(
        path,
        Some("v:0"),
        "stream=codec_name,width,height,time_base,duration_ts,start_time,nb_frames",
        false,
    );
    assert_eq!(field(&video, "codec_name"), "h264");
    assert_eq!(field(&video, "width"), WIDTH.to_string());
    assert_eq!(field(&video, "height"), HEIGHT.to_string());
    assert_eq!(field(&video, "time_base"), "1/30000");
    assert_eq!(field(&video, "nb_frames").parse::<u64>().unwrap(), frames);
    assert_eq!(field(&video, "start_time").parse::<f64>().unwrap(), 0.0);
    let end_ticks = (duration * 30_000.0).round() as u64;
    assert_eq!(
        field(&video, "duration_ts").parse::<u64>().unwrap(),
        end_ticks
    );
    let video_packets = probe(path, Some("v:0"), "packet=pts,duration", true);
    let packets = packet_times(&video_packets);
    assert_eq!(packets.len() as u64, frames);
    for (index, &(pts, duration)) in packets.iter().enumerate() {
        assert_eq!(pts, index as i64 * 1_001);
        assert_eq!(
            duration,
            if index + 1 == packets.len() {
                end_ticks - pts as u64
            } else {
                1_001
            }
        );
    }
    let container = probe(path, None, "format=duration", false);
    assert!(
        (field(&container, "duration").parse::<f64>().unwrap() - duration).abs() < 1.0e-6,
        "container endpoint: {container}; expected {duration}"
    );
    let audio_stream = probe(
        path,
        Some("a:0"),
        "stream=codec_name,sample_rate,channels,time_base,start_time,duration_ts",
        false,
    );
    let mut evidence =
        format!("VIDEO\n{video}\nVIDEO PACKETS\n{video_packets}\nFORMAT\n{container}");
    if audio {
        assert_eq!(field(&audio_stream, "codec_name"), "aac");
        assert_eq!(field(&audio_stream, "sample_rate"), AUDIO_RATE.to_string());
        assert_eq!(field(&audio_stream, "channels"), "2");
        assert_eq!(field(&audio_stream, "time_base"), "1/48000");
        assert_eq!(
            field(&audio_stream, "start_time").parse::<f64>().unwrap(),
            0.0
        );
        let audio_end = field(&audio_stream, "duration_ts").parse::<u64>().unwrap();
        assert!((audio_end as f64 / f64::from(AUDIO_RATE) - duration).abs() <= 1.0 / 48_000.0);
        let audio_packets = probe(path, Some("a:0"), "packet=pts,duration", true);
        let packets = packet_times(&audio_packets);
        assert_eq!(packets.first().copied(), Some((-1_024, 1_024)));
        assert!(packets.windows(2).all(|pair| pair[1].0 > pair[0].0));
        let packet_end = packets
            .iter()
            .map(|(pts, duration)| *pts + *duration as i64)
            .max()
            .unwrap();
        assert!((packet_end as f64 / 48_000.0 - duration).abs() <= 1.0 / 48_000.0);
        evidence.push_str(&format!(
            "\nAUDIO\n{audio_stream}\nAUDIO PACKETS\n{audio_packets}"
        ));
    } else {
        assert!(
            audio_stream.is_empty(),
            "unexpected soundtrack: {audio_stream}"
        );
    }
    fs::write(path.with_extension("ffprobe.txt"), evidence).unwrap();
}

fn verify_pixels(path: &Path, frames: u64, crop_start: f64, visual_duration: f64) {
    let pixels = command_output(
        Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:v:0",
                "-vf",
                "scale=in_range=limited:out_range=full:in_color_matrix=bt709,format=rgba",
                "-fps_mode",
                "passthrough",
                "-f",
                "rawvideo",
                "pipe:1",
            ]),
    );
    assert_eq!(pixels.len(), frames as usize * FRAME_BYTES);
    for (frame, rgba) in pixels.as_chunks::<FRAME_BYTES>().0.iter().enumerate() {
        let time = crop_start + frame as f64 * 1_001.0 / 30_000.0;
        let world_x = -2.0 + 4.0 * (time / visual_duration).min(1.0);
        let expected_x = f64::from(WIDTH) / 2.0
            + world_x * f64::from(HEIGHT) / f64::from(noon_core::DEFAULT_FRAME_HEIGHT);
        let mut total = 0.0;
        let mut centroid = [0.0; 2];
        for (index, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
            let weight = pixel[..3]
                .iter()
                .map(|value| u32::from(*value))
                .sum::<u32>();
            if weight > 36 {
                let weight = f64::from(weight);
                total += weight;
                centroid[0] += weight * ((index % WIDTH as usize) as f64 + 0.5);
                centroid[1] += weight * ((index / WIDTH as usize) as f64 + 0.5);
            }
        }
        assert!(total > 0.0, "missing marker in frame {frame}");
        assert!(
            (centroid[0] / total - expected_x).abs() < 1.1,
            "frame {frame}"
        );
        assert!((centroid[1] / total - f64::from(HEIGHT) / 2.0).abs() < 1.1);
    }
}

// Independent continuous-tone oracle. The trimmed loop contains exactly 64
// left-channel cycles and 112 right-channel cycles, so wrapping does not alter
// this analytic phase. It does not call clip evaluation or the block mixer.
fn expected_sample(tone: Tone, scene_time: f64, cutoff: f64, channel: usize) -> f64 {
    let elapsed = scene_time - tone.start;
    if elapsed < 0.0 || elapsed >= tone.duration || scene_time >= cutoff {
        return 0.0;
    }
    let envelope = (elapsed / FADE_IN)
        .min((tone.duration - elapsed) / FADE_OUT)
        .min(1.0);
    let source_time = SOURCE_START as f64 / f64::from(SOURCE_RATE) + elapsed;
    AMPLITUDES[channel]
        * tone.gain
        * envelope
        * (std::f64::consts::TAU * FREQUENCIES[channel] * source_time).sin()
}

fn spectral_amplitude(samples: &[f32], channel: usize, start: usize, end: usize, hz: f64) -> f64 {
    let mut quadrature = [0.0; 2];
    for frame in start..end {
        let phase = std::f64::consts::TAU * hz * frame as f64 / f64::from(AUDIO_RATE);
        let value = f64::from(samples[frame * 2 + channel]);
        quadrature[0] += value * phase.cos();
        quadrature[1] += value * phase.sin();
    }
    2.0 * quadrature[0].hypot(quadrature[1]) / (end - start) as f64
}

fn verify_audio(path: &Path, duration: f64, crop_start: f64, cutoff: f64, tone: Tone) {
    let bytes = command_output(
        Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:a:0",
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ]),
    );
    assert_eq!(bytes.len() % 8, 0);
    let samples: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect();
    assert!(samples.iter().all(|value| value.is_finite()));
    let frames = (duration * f64::from(AUDIO_RATE)).floor() as usize;
    // AAC decoding may expose the final padded codec block. Presentation ends
    // are checked from packets and stream/edit timing above, never byte length.
    assert!(samples.len() >= frames * 2);
    for channel in 0..2 {
        let mut error = 0.0;
        let mut expected_power = 0.0;
        let mut actual_power = 0.0;
        let mut cross = 0.0;
        let mut silence_power = 0.0;
        let mut silent_frames = 0;
        for frame in 0..frames {
            let time = crop_start + frame as f64 / f64::from(AUDIO_RATE);
            let expected = expected_sample(tone, time, cutoff, channel);
            let actual = f64::from(samples[frame * 2 + channel]);
            error += (actual - expected).powi(2);
            expected_power += expected * expected;
            actual_power += actual * actual;
            cross += actual * expected;
            // Leave AAC's finite transform overlap around abrupt clip/stop edges.
            if time < tone.start - 0.025
                || time > tone.start + tone.duration + 0.025
                || time > cutoff + 0.025
            {
                silence_power += actual * actual;
                silent_frames += 1;
            }
        }
        let error_rms = (error / frames as f64).sqrt();
        let correlation = cross / (expected_power * actual_power).sqrt();
        let rms_ratio = (actual_power / expected_power).sqrt();
        assert!(
            error_rms < 0.025,
            "channel {channel}: RMS error {error_rms}"
        );
        assert!(
            correlation > 0.97,
            "channel {channel}: phase correlation {correlation}"
        );
        assert!(
            (0.85..1.15).contains(&rms_ratio),
            "channel {channel}: RMS ratio {rms_ratio}"
        );
        if silent_frames >= 64 {
            let silence_rms = (silence_power / silent_frames as f64).sqrt();
            assert!(
                silence_rms < 0.008,
                "channel {channel}: silent-gap RMS {silence_rms}"
            );
        }
    }
    let active_start = (tone.start + FADE_IN + 0.025).max(crop_start + 0.025);
    let active_end = (tone.start + tone.duration - FADE_OUT - 0.025).min(cutoff - 0.025);
    let first = ((active_start - crop_start) * 48_000.0).ceil() as usize;
    let end = ((active_end - crop_start) * 48_000.0).floor() as usize;
    assert!(end > first + 2_400);
    for channel in 0..2 {
        let own = spectral_amplitude(&samples, channel, first, end, FREQUENCIES[channel]);
        let other = spectral_amplitude(&samples, channel, first, end, FREQUENCIES[1 - channel]);
        assert!(
            own > 0.1,
            "channel {channel}: missing authored tone ({own})"
        );
        assert!(
            own > 4.0 * other,
            "channel {channel}: stereo frequency crossover"
        );
    }
}

#[test]
#[ignore = "requires software Vulkan, FFmpeg and ffprobe; validates decoded audiovisual output"]
fn finite_movies_keep_audio_phase_gaps_crop_holds_and_exact_presentation_endpoints() {
    let directory = TestDirectory::new("audio-proof", true);
    struct Case {
        name: &'static str,
        visual_duration: f64,
        tone: Tone,
        start_frame: u64,
        stop: ExportStop,
        hold: f64,
        frames: u64,
        cutoff: f64,
    }
    let cases = [
        Case {
            name: "short-audio-with-gaps",
            visual_duration: 0.73,
            tone: Tone::new(0.125, 0.35),
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            hold: 0.0,
            frames: 22,
            cutoff: 0.73,
        },
        Case {
            name: "audio-outlasts-animation",
            visual_duration: 0.25,
            tone: Tone::new(0.05, 0.73),
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            hold: 0.0,
            frames: 24,
            cutoff: 0.78,
        },
        Case {
            name: "cropped-fractional-source-phase",
            visual_duration: 0.73,
            tone: Tone::new(0.05, 0.68),
            start_frame: 7,
            stop: ExportStop::SourceEnd,
            hold: 0.0,
            frames: 15,
            cutoff: 0.73,
        },
        Case {
            name: "explicit-stop-with-silent-grid-tail",
            visual_duration: 0.9,
            tone: Tone::new(0.0, 0.9),
            start_frame: 0,
            stop: ExportStop::EndTime(0.501),
            hold: 0.0,
            frames: 16,
            cutoff: 0.501,
        },
        Case {
            name: "terminal-hold-is-silent",
            visual_duration: 0.43,
            tone: Tone::new(0.0, 0.43),
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            hold: 0.15,
            frames: 18,
            cutoff: 0.58,
        },
    ];
    for case in cases {
        let path = directory.path.join(format!("{}.mp4", case.name));
        let mut session = fixture(case.visual_duration, Some(case.tone));
        let source_end = case
            .visual_duration
            .max(case.tone.start + case.tone.duration);
        assert!((session.finite_timeline_end_time().unwrap() - source_end).abs() < 1e-12);
        let summary = export_session_file(
            &mut session,
            ExportFrameOptions {
                start_frame: case.start_frame,
                stop: case.stop,
                final_hold_seconds: case.hold,
                ..frame_options()
            },
            capture_options(),
            OutputOptions::mp4(&path),
        )
        .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        assert_eq!(
            summary.capture.sampling.frames, case.frames,
            "{}",
            case.name
        );
        let crop_start = case.start_frame as f64 * 1_001.0 / 30_000.0;
        assert!((summary.capture.sampling.start_time - crop_start).abs() < 1e-12);
        assert!((summary.capture.sampling.end_time - case.cutoff).abs() < 1e-12);
        let natural = case.stop == ExportStop::SourceEnd;
        assert_eq!(
            summary.capture.sampling.reason,
            if natural {
                ExportEndReason::SourceEnd
            } else {
                ExportEndReason::RequestedStop
            }
        );
        if natural {
            assert!((summary.capture.sampling.source_end.unwrap() - source_end).abs() < 1e-12);
        } else {
            assert_eq!(summary.capture.sampling.source_end, None);
        }
        let duration = if natural && case.hold == 0.0 {
            case.cutoff - crop_start
        } else {
            case.frames as f64 * 1_001.0 / 30_000.0
        };
        verify_movie_timing(&path, case.frames, duration, true);
        verify_pixels(&path, case.frames, crop_start, case.visual_duration);
        verify_audio(&path, duration, crop_start, case.cutoff, case.tone);
        eprintln!("{}: {}", case.name, path.display());
    }
}

#[test]
#[ignore = "requires software Vulkan, FFmpeg and ffprobe"]
fn video_only_sessions_and_explicit_visual_png_output_keep_their_profiles() {
    let directory = TestDirectory::new("profiles", true);
    let path = directory.path.join("video-only.mp4");
    let mut session = fixture(0.23, None);
    let summary = export_session_file(
        &mut session,
        frame_options(),
        capture_options(),
        OutputOptions::mp4(&path),
    )
    .unwrap();
    assert_eq!(summary.capture.sampling.frames, 7);
    verify_movie_timing(&path, 7, 0.23, false);
    verify_pixels(&path, 7, 0.0, 0.23);

    // PNG is explicitly visual-only. Even audio that would overload a movie
    // does not enter the mixer or create an undocumented soundtrack sidecar.
    let mut tone = Tone::new(0.0, 0.18);
    tone.gain = 8.0;
    let mut session = fixture(0.23, Some(tone));
    let images = directory.path.join("images");
    let summary = export_session_file(
        &mut session,
        ExportFrameOptions {
            stop: ExportStop::FrameCount(1),
            ..frame_options()
        },
        capture_options(),
        OutputOptions::png_sequence(&images),
    )
    .unwrap();
    assert_eq!(summary.capture.sampling.frames, 1);
    let mut entries: Vec<_> = fs::read_dir(images.join("frames"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    entries.sort();
    assert_eq!(entries, ["frame-0000000000.png", "timing.tsv"]);
    assert_eq!(fs::read_dir(&images).unwrap().count(), 1);
}

#[test]
#[ignore = "requires FFmpeg codec probe; overload fails before GPU creation"]
fn soundtrack_failure_precedes_visual_advancement_and_preserves_existing_video() {
    let directory = TestDirectory::new("mix-failure", false);
    let path = directory.path.join("existing.mp4");
    fs::write(&path, b"existing user video").unwrap();
    let mut tone = Tone::new(0.0, 0.18);
    tone.gain = 8.0;
    let mut session = fixture(0.23, Some(tone));
    let publication = session.publication_context();
    let mut output = OutputOptions::mp4(&path);
    output.overwrite = true;
    let error =
        export_session_file(&mut session, frame_options(), capture_options(), output).unwrap_err();
    assert!(
        matches!(error, SessionFileExportError::Output(_)),
        "{error}"
    );
    assert!(error.to_string().contains("exceeds [-1, 1]"), "{error}");
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(fs::read(&path).unwrap(), b"existing user video");
    assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 1);
}
