//! Export the shared Rust FollowingGraphCamera example directly to MP4.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon_export::{capture_mp4_ffmpeg, CaptureOptions, FfmpegMp4Options};

    let destination = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("scene.mp4"));
    let (mut program, mut callbacks) =
        noon::example_scenes::following_graph_camera::program()
            .map_err(std::io::Error::other)?;
    let frames = ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1)?,
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 1_000,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    };
    let summary = capture_mp4_ffmpeg(
        &mut program,
        &mut callbacks,
        frames,
        CaptureOptions::new(1920, 1080),
        FfmpegMp4Options::new(&destination),
    )
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    eprintln!(
        "{} frames finalized to {}",
        summary.capture.sampling.frames,
        summary.path.display()
    );
    Ok(())
}
