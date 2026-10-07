#![cfg(all(not(target_arch = "wasm32"), feature = "ffmpeg"))]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use noon::integration::{ExportFrame, ExportFrameOptions, ExportStop, FrameGrid, FrameRate, SampleObservation};
use noon_export::{capture_frames, CaptureCancellation, CaptureOptions, CapturePixelFormat, CaptureWork, CapturedFrame};
use noon_export::video::{export_video, Mp4Encoder, VideoOptions};

fn proof_root() -> PathBuf {
    let root = std::env::var_os("NOON_VIDEO_PROOF_DIR").map(PathBuf::from).unwrap_or_else(|| {
        std::env::temp_dir().join(format!("noon-video-proof-{}", std::process::id()))
    });
    fs::create_dir_all(&root).unwrap();
    root
}

fn config() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1).unwrap(), start_frame: 0, stop: ExportStop::SourceEnd,
        max_frames: 1_000, max_transitions_per_sample: 64, final_hold_seconds: 0.0,
    }
}

fn capture() -> CaptureOptions {
    let mut options = CaptureOptions::new(320, 180);
    options.backends = noon_export::Backends::VULKAN;
    options.force_fallback_adapter = true;
    options
}

fn specification(root: &Path, name: &str, width: u32, height: u32, count: u64, rate: FrameRate, mae: f64) {
    // Fixed test names only. JSON is an external qualification artifact, never an engine boundary.
    fs::write(root.join(format!("{name}.case.json")), format!(
        "{{\"name\":\"{name}\",\"width\":{width},\"height\":{height},\"frames\":{count},\"fps\":\"{}/{}\",\"max_mae\":{mae}}}\n",
        rate.numerator(), rate.denominator())).unwrap();
}

#[test]
#[ignore = "requires FFmpeg/libx264 and Vulkan; selected by Native Video Export"]
fn native_camera_video_matches_a_fresh_raw_capture_and_aborts_safely() {
    let root = proof_root();
    let path = root.join("camera.mp4");
    let (mut program, mut callbacks) = noon::example_scenes::following_graph_camera::program().unwrap();
    let mut video = VideoOptions::mp4(&path);
    video.overwrite = true; // Explicitly test-owned artifact, including local reruns.
    video.encoder_threads = 1;
    let result = export_video(&mut program, &mut callbacks, config(), capture(), video.clone()).unwrap();
    assert_eq!(result.video.frames, 90);
    assert_eq!(result.video.frames, result.capture.sampling.frames);
    assert!(result.video.bytes > 0);
    eprintln!("native MP4: {:?}, adapter {:?}, 90 frames at 30 FPS", result.video.output, result.capture.adapter);

    let (mut reference, mut callbacks) = noon::example_scenes::following_graph_camera::program().unwrap();
    let mut raw = fs::File::create(root.join("camera.rgba")).unwrap();
    let reference = capture_frames(&mut reference, &mut callbacks, config(), capture(), |frame| raw.write_all(frame.rgba)).unwrap();
    raw.flush().unwrap();
    assert_eq!(reference.sampling.frames, 90);
    specification(&root, "camera", 320, 180, 90, config().frame_rate, 3.0);

    let original = fs::read(&path).unwrap();
    let mut no_overwrite = video.clone();
    no_overwrite.overwrite = false;
    assert!(Mp4Encoder::new(no_overwrite, 320, 180, config().frame_rate, CaptureCancellation::default()).is_err());
    let (mut limited, mut callbacks) = noon::example_scenes::following_graph_camera::program().unwrap();
    let bounds = ExportFrameOptions { max_frames: 2, ..config() };
    assert!(export_video(&mut limited, &mut callbacks, bounds, capture(), video).is_err());
    assert_eq!(fs::read(&path).unwrap(), original, "failed overwrite modified the prior video");
    assert!(fs::read_dir(&root).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with(".noon-video-")));
}

#[test]
#[ignore = "requires FFmpeg/libx264; selected by Native Video Export"]
fn fractional_video_keeps_all_numbered_frames_and_explicit_color_profile() {
    let root = proof_root();
    let rate = FrameRate::new(60_000, 1_001).unwrap();
    let grid = FrameGrid::new(rate, 0.0).unwrap();
    let mut video = VideoOptions::mp4(root.join("fractional.mp4"));
    video.overwrite = true;
    video.crf = 12;
    video.encoder_threads = 1;
    let mut encoder = Mp4Encoder::new(video, 128, 96, rate, CaptureCancellation::default()).unwrap();
    let mut raw = fs::File::create(root.join("fractional.rgba")).unwrap();
    let mut scene = noon::Scene::new();
    let session = scene.execution_session().unwrap();
    for index in 0..12 {
        let mut pixels = Vec::new();
        // Uniform, macroblock-aligned color/gray patches. Each whole frame has
        // distinct values, so an independent decoded comparison detects reordering.
        for _y in 0..96 {
            for x in 0..128 {
                let rgb = match x / 32 {
                    0 => [index as u8 * 17; 3],
                    1 => [40 + index as u8 * 11, 70, 170],
                    2 => [25, 210 - index as u8 * 9, 65],
                    _ => [170, 30 + index as u8 * 13, 90],
                };
                pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        let sample = grid.sample(index + 4).unwrap(); // Crop identity, zero output PTS.
        encoder.write_frame(&CapturedFrame {
            frame: ExportFrame { source_sample: sample, pts: index, held: false },
            observation: SampleObservation { requested_time: sample.authored_time(), published_time: sample.authored_time(), publication: session.publication_context() },
            width: 128, height: 96, format: CapturePixelFormat::RendererRgba8UnormOpaque,
            rgba: &pixels, work: CaptureWork::default(),
        }).unwrap();
        raw.write_all(&pixels).unwrap();
    }
    raw.flush().unwrap();
    let summary = encoder.finish(12).unwrap();
    assert_eq!(summary.frames, 12);
    specification(&root, "fractional", 128, 96, 12, rate, 3.0);

    let path = root.join("mismatched.mp4");
    let mut options = VideoOptions::mp4(&path);
    options.encoder_threads = 1;
    let encoder = Mp4Encoder::new(options, 128, 96, rate, CaptureCancellation::default()).unwrap();
    assert!(encoder.finish(1).is_err());
    assert!(!path.exists());
}
