//! Encode the existing Rust FollowingGraphCamera scene directly to MP4.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon_export::video::{export_video, VideoOptions};
    use noon_export::CaptureOptions;

    let mut args = std::env::args_os().skip(1);
    let output = args.next().ok_or("usage: export_video OUTPUT.mp4")?;
    if args.next().is_some() { return Err("usage: export_video OUTPUT.mp4".into()); }
    let (mut program, mut callbacks) = noon::example_scenes::following_graph_camera::program()
        .map_err(std::io::Error::other)?;
    let frames = ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1)?, start_frame: 0, stop: ExportStop::SourceEnd,
        max_frames: 1_000, max_transitions_per_sample: 64, final_hold_seconds: 0.0,
    };
    let result = export_video(&mut program, &mut callbacks, frames, CaptureOptions::new(1280, 720), VideoOptions::mp4(output))
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    eprintln!("{} frames, {} bytes, {:?}; adapter {:?}", result.video.frames, result.video.bytes, result.video.output, result.capture.adapter);
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
