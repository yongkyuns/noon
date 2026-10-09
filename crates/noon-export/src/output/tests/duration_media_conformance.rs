//! Real encoded duration must follow authored time, never producer throughput.
#![cfg(not(target_arch = "wasm32"))]

use super::super::{FileSink, OutputOptions};
use crate::{capture_frames, Backends, CaptureOptions};
use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveSession, LiveSessionError, Mobject,
    RateFunction, RustHostCallbackTable, Scene,
};
use noon_core::DEFAULT_FRAME_HEIGHT;
use std::{fs, io, path::Path, process::Command, time::Duration};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 128;
const FRAME_BYTES: usize = WIDTH as usize * HEIGHT as usize * 4;

struct Source {
    marker: Mobject,
    target: Mobject,
    started: bool,
}

impl LiveContinuation for Source {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        if self.started {
            return Ok(ContinuationStep::Finished);
        }
        self.started = true;
        live.declare_and_activate_transform_to(
            &self.marker,
            &self.target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .map(ContinuationStep::Await)
    }
}

fn command_output(command: &mut Command) -> Vec<u8> {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

fn capture_movie(path: &Path, p: u32, q: u32, delayed: bool) -> Vec<u8> {
    let mut scene = Scene::new();
    let mut marker = scene.square(0.75).unwrap();
    marker.set_translation(-2.0, 0.0).unwrap();
    marker.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    marker.set_stroke_width(0.0).unwrap();
    scene.add(&marker).unwrap();
    let mut target = marker.target_editor().unwrap();
    target.set_translation(2.0, 0.0).unwrap();
    let mut program = scene
        .into_live_program(Source {
            marker,
            target,
            started: false,
        })
        .unwrap();
    let rate = FrameRate::new(p, q).unwrap();
    let capture = CaptureOptions {
        backends: Backends::VULKAN,
        force_fallback_adapter: true,
        ..CaptureOptions::new(WIDTH, HEIGHT)
    };
    let mut sink = FileSink::new(
        OutputOptions::mp4(path),
        rate,
        WIDTH,
        HEIGHT,
        capture.cancellation.clone(),
    )
    .unwrap();
    let mut raw = Vec::new();
    let summary = capture_frames(
        &mut program,
        &mut RustHostCallbackTable::new(),
        ExportFrameOptions {
            frame_rate: rate,
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 512,
            max_transitions_per_sample: 64,
            final_hold_seconds: 0.0,
        },
        capture,
        |frame| {
            if delayed && p == 5 && frame.frame.pts == 0 {
                // This one blocked producer outlasts the entire two-second movie.
                std::thread::sleep(Duration::from_millis(2_100));
            }
            if delayed && frame.frame.pts.is_multiple_of(7) {
                // Inject real consumer stalls, without changing sample times.
                std::thread::sleep(Duration::from_millis(20));
            }
            let time = frame.frame.pts as f64 * f64::from(q) / f64::from(p);
            assert!((frame.observation.requested_time - time).abs() < 1.0e-12);
            assert!((frame.observation.published_time - time).abs() < 1.0e-12);
            verify_image_position(frame.rgba, time);
            raw.extend_from_slice(frame.rgba);
            sink.write(frame)
        },
    )
    .unwrap();
    let expected = (2 * u64::from(p)).div_ceil(u64::from(q));
    assert_eq!(summary.sampling.source_end, Some(2.0));
    assert_eq!(summary.sampling.frames, expected);
    sink.finish(expected).unwrap();
    raw
}

fn verify_movie(path: &Path, p: u32, q: u32) -> Vec<u8> {
    let probe = command_output(
        Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-count_frames",
                "-show_frames",
                "-show_entries",
                "frame=pts:stream=time_base,nb_read_frames,duration_ts,width,height",
                "-of",
                "default=noprint_wrappers=1",
            ])
            .arg(path),
    );
    let probe = String::from_utf8(probe).unwrap();
    fs::write(path.with_extension("ffprobe.txt"), &probe).unwrap();
    let value = |key: &str| -> &str {
        probe
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once('=')?;
                (name == key).then_some(value)
            })
            .unwrap_or_else(|| panic!("missing {key}: {probe}"))
    };
    let frames = (2 * u64::from(p)).div_ceil(u64::from(q));
    assert_eq!(value("width").parse::<u32>().unwrap(), WIDTH);
    assert_eq!(value("height").parse::<u32>().unwrap(), HEIGHT);
    assert_eq!(value("time_base"), format!("1/{p}"));
    assert_eq!(value("nb_read_frames").parse::<u64>().unwrap(), frames);
    assert_eq!(
        value("duration_ts").parse::<u64>().unwrap(),
        frames * u64::from(q)
    );
    let pts: Vec<u64> = probe
        .lines()
        .filter_map(|line| line.strip_prefix("pts="))
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(
        pts,
        (0..frames).map(|n| n * u64::from(q)).collect::<Vec<_>>()
    );
    let encoded_seconds = frames as f64 * f64::from(q) / f64::from(p);
    assert!(encoded_seconds >= 2.0);
    assert!(encoded_seconds - 2.0 < f64::from(q) / f64::from(p));
    let pixels = command_output(
        Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(path)
            .args([
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
    for (index, rgba) in pixels.as_chunks::<FRAME_BYTES>().0.iter().enumerate() {
        let time = index as f64 * f64::from(q) / f64::from(p);
        verify_image_position(rgba, time);
    }
    pixels
}

// Independent image-space oracle for the white square on the default camera.
// Compression and rasterization permit a subpixel centroid error, not a time
// accumulator or a renderer-owned position oracle.
fn verify_image_position(rgba: &[u8], time: f64) {
    assert_eq!(rgba.len(), FRAME_BYTES);
    let mut weight = 0.0;
    let mut x = 0.0;
    let mut y = 0.0;
    for (index, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
        let intensity: u32 = pixel[..3].iter().map(|v| u32::from(*v)).sum();
        if intensity > 36 {
            let intensity = f64::from(intensity);
            weight += intensity;
            x += intensity * ((index % WIDTH as usize) as f64 + 0.5);
            y += intensity * ((index / WIDTH as usize) as f64 + 0.5);
        }
    }
    assert!(weight > 0.0, "authored moving square is missing");
    let world_x = -2.0 + 2.0 * time;
    let pixels_per_unit = f64::from(HEIGHT) / f64::from(DEFAULT_FRAME_HEIGHT);
    let expected_x = f64::from(WIDTH) / 2.0 + world_x * pixels_per_unit;
    assert!((x / weight - expected_x).abs() <= 0.8);
    assert!((y / weight - f64::from(HEIGHT) / 2.0).abs() <= 0.8);
}

#[test]
#[ignore = "requires software Vulkan and FFmpeg; selected by native output qualification"]
fn encoded_timing_and_pixels_are_independent_of_capture_throughput() -> io::Result<()> {
    let root = std::env::var_os("NOON_OUTPUT_PROOF_DIR")
        .map(std::path::PathBuf::from)
        .expect("set NOON_OUTPUT_PROOF_DIR to preserve decoded timing proof")
        .join("duration-conformance");
    fs::create_dir_all(&root)?;
    for (p, q) in [
        (5, 1),
        (15, 1),
        (24, 1),
        (30, 1),
        (60, 1),
        (120, 1),
        (30_000, 1_001),
        (60_000, 1_001),
    ] {
        let fast = root.join(format!("{p}-{q}-fast.mp4"));
        let slow = root.join(format!("{p}-{q}-delayed.mp4"));
        let fast_raw = capture_movie(&fast, p, q, false);
        let slow_raw = capture_movie(&slow, p, q, true);
        fs::write(fast.with_extension("rgba"), &fast_raw)?;
        assert!(
            fast_raw == slow_raw,
            "consumer delay changed captured {p}/{q} pixels"
        );
        let fast_pixels = verify_movie(&fast, p, q);
        let slow_pixels = verify_movie(&slow, p, q);
        assert!(
            fast_pixels == slow_pixels,
            "consumer delay changed decoded {p}/{q} pixels"
        );
        let (raw_frames, raw_remainder) = fast_raw.as_chunks::<FRAME_BYTES>();
        let (decoded_frames, decoded_remainder) = fast_pixels.as_chunks::<FRAME_BYTES>();
        assert!(raw_remainder.is_empty());
        assert!(decoded_remainder.is_empty());
        assert_eq!(raw_frames.len(), decoded_frames.len());
        assert!(
            raw_frames.iter().skip(1).any(|f| f != &raw_frames[0]),
            "timing proof must contain a moving image"
        );
        // Keep the existing media oracle's per-frame error limits, including
        // foreground error so a frozen or black movie cannot pass on its background.
        for (raw, decoded) in raw_frames.iter().zip(decoded_frames) {
            let total: u64 = raw
                .iter()
                .zip(decoded)
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            assert!(total as f64 / FRAME_BYTES as f64 <= 3.0);
            let mut active_error = 0_u64;
            let mut active_channels = 0_u64;
            let (raw_pixels, _) = raw.as_chunks::<4>();
            let (decoded_pixels, _) = decoded.as_chunks::<4>();
            for (a, b) in raw_pixels.iter().zip(decoded_pixels) {
                if a[..3].iter().any(|value| *value > 12) {
                    active_channels += 3;
                    active_error += a[..3]
                        .iter()
                        .zip(&b[..3])
                        .map(|(x, y)| u64::from(x.abs_diff(*y)))
                        .sum::<u64>();
                }
            }
            assert!(active_channels > 0);
            assert!(active_error as f64 / active_channels as f64 <= 25.0);
        }
    }
    Ok(())
}
