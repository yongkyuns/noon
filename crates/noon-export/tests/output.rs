use std::path::{Path, PathBuf};
use std::time::Duration;

use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon_export::{
    capture_mp4_ffmpeg, capture_png_sequence, Backends, CaptureOptions, FfmpegMp4Options,
    NativeOutputError, PngSequenceOptions,
};

fn frame_options(rate: FrameRate) -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: rate,
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 1_000,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    }
}

fn software(width: u32, height: u32) -> CaptureOptions {
    let mut options = CaptureOptions::new(width, height);
    options.backends = Backends::VULKAN;
    options.force_fallback_adapter = true;
    options.gpu_wait_timeout = Duration::from_secs(30);
    options
}

fn root() -> (PathBuf, bool) {
    if let Some(base) = std::env::var_os("NOON_OUTPUT_PROOF_DIR") {
        let path = PathBuf::from(base).join("files");
        if path.exists() {
            std::fs::remove_dir_all(&path).unwrap();
        }
        std::fs::create_dir_all(&path).unwrap();
        return (path, false);
    }
    let path = std::env::temp_dir().join(format!(
        "noon-output-integration-{}",
        std::process::id()
    ));
    if path.exists() {
        std::fs::remove_dir_all(&path).unwrap();
    }
    std::fs::create_dir_all(&path).unwrap();
    (path, true)
}

fn ffmpeg() -> PathBuf {
    std::env::var_os("NOON_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

fn following() -> (
    noon::LiveProgram<noon::example_scenes::following_graph_camera::FollowingGraphCamera>,
    noon::RustHostCallbackTable,
) {
    noon::example_scenes::following_graph_camera::program().unwrap()
}

fn scratch_entries(root: &Path) -> Vec<String> {
    std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".noon-"))
        .collect()
}

#[test]
#[ignore = "requires software Vulkan and FFmpeg/libx264; run by Native Host Smoke"]
fn native_png_and_ffmpeg_outputs_finalize_without_partial_publication() {
    let (root, cleanup) = root();

    let png_dir = root.join("frames");
    let (mut png_program, mut png_callbacks) = following();
    let png = capture_png_sequence(
        &mut png_program,
        &mut png_callbacks,
        frame_options(FrameRate::new(30, 1).unwrap()),
        software(160, 90),
        PngSequenceOptions::new(&png_dir),
    )
    .unwrap();
    assert_eq!(png.capture.sampling.frames, 90);
    assert_eq!(png.path, png_dir);

    let mut pngs = std::fs::read_dir(&png.path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
        .collect::<Vec<_>>();
    pngs.sort();
    assert_eq!(pngs.len(), 90);
    assert_eq!(pngs.first().unwrap().file_name().unwrap(), "frame_000000.png");
    assert_eq!(pngs.last().unwrap().file_name().unwrap(), "frame_000089.png");
    for path in [pngs.first().unwrap(), pngs.last().unwrap()] {
        let bytes = std::fs::read(path).unwrap();
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap();
        assert_eq!((image.width(), image.height()), (160, 90));
    }

    let mp4_path = root.join("following.mp4");
    let (mut mp4_program, mut mp4_callbacks) = following();
    let mut mp4_options = FfmpegMp4Options::new(&mp4_path);
    mp4_options.executable = ffmpeg();
    let mp4 = capture_mp4_ffmpeg(
        &mut mp4_program,
        &mut mp4_callbacks,
        frame_options(FrameRate::new(30_000, 1_001).unwrap()),
        software(160, 90),
        mp4_options,
    )
    .unwrap();
    assert_eq!(mp4.capture.sampling.frames, 90);
    assert_eq!(mp4.capture.sampling.frame_rate.time_base(), (1_001, 30_000));
    assert_eq!(mp4.path, mp4_path);
    assert!(std::fs::metadata(&mp4.path).unwrap().len() > 0);

    let preserved = root.join("preserved.mp4");
    std::fs::write(&preserved, b"existing-output").unwrap();
    let (mut preserved_program, mut preserved_callbacks) = following();
    let mut preserved_options = FfmpegMp4Options::new(&preserved);
    preserved_options.executable = ffmpeg();
    let result = capture_mp4_ffmpeg(
        &mut preserved_program,
        &mut preserved_callbacks,
        frame_options(FrameRate::new(2, 1).unwrap()),
        software(160, 90),
        preserved_options,
    );
    assert!(matches!(
        result,
        Err(NativeOutputError::Finalize(
            noon_export::FfmpegMp4Error::DestinationExists(_)
        ))
    ));
    assert_eq!(std::fs::read(&preserved).unwrap(), b"existing-output");
    assert_eq!(preserved_program.session().frame().time, 0.0);

    let broken = root.join("broken.mp4");
    let (mut broken_program, mut broken_callbacks) = following();
    let mut broken_options = FfmpegMp4Options::new(&broken);
    broken_options.executable = ffmpeg();
    broken_options.preset = "definitely-not-an-x264-preset".to_owned();
    let broken_result = capture_mp4_ffmpeg(
        &mut broken_program,
        &mut broken_callbacks,
        frame_options(FrameRate::new(2, 1).unwrap()),
        software(160, 90),
        broken_options,
    );
    assert!(broken_result.is_err());
    assert!(!broken.exists(), "failed encoder published a partial MP4");

    let cancelled = root.join("cancelled.mp4");
    let (mut cancelled_program, mut cancelled_callbacks) = following();
    let mut cancelled_capture = software(160, 90);
    cancelled_capture.cancellation.cancel();
    let mut cancelled_options = FfmpegMp4Options::new(&cancelled);
    cancelled_options.executable = ffmpeg();
    let cancelled_result = capture_mp4_ffmpeg(
        &mut cancelled_program,
        &mut cancelled_callbacks,
        frame_options(FrameRate::new(2, 1).unwrap()),
        cancelled_capture,
        cancelled_options,
    );
    assert!(cancelled_result.is_err());
    assert!(!cancelled.exists(), "cancelled export published an MP4");
    assert_eq!(cancelled_program.session().frame().time, 0.0);

    let replaced = root.join("replaced.mp4");
    std::fs::write(&replaced, b"old-output").unwrap();
    let (mut replaced_program, mut replaced_callbacks) = following();
    let mut replaced_options = FfmpegMp4Options::new(&replaced);
    replaced_options.executable = ffmpeg();
    replaced_options.overwrite = true;
    let replaced_summary = capture_mp4_ffmpeg(
        &mut replaced_program,
        &mut replaced_callbacks,
        frame_options(FrameRate::new(1, 1).unwrap()),
        software(64, 64),
        replaced_options,
    )
    .unwrap();
    assert_eq!(replaced_summary.capture.sampling.frames, 3);
    assert_ne!(std::fs::read(&replaced).unwrap(), b"old-output");

    assert!(
        scratch_entries(&root).is_empty(),
        "temporary output paths leaked after success/failure"
    );

    let recovery = root.join("recovery");
    let (mut recovery_program, mut recovery_callbacks) = following();
    let recovery_summary = capture_png_sequence(
        &mut recovery_program,
        &mut recovery_callbacks,
        frame_options(FrameRate::new(1, 1).unwrap()),
        software(64, 64),
        PngSequenceOptions::new(&recovery),
    )
    .unwrap();
    assert_eq!(recovery_summary.capture.sampling.frames, 3);

    if cleanup {
        std::fs::remove_dir_all(root).unwrap();
    }
}
