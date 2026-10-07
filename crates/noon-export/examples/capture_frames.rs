//! Stream a normal shared Rust scene as raw RGBA, without a window.
//! Encoding and atomic file finalization are separate output-adapter work.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon_export::{capture_frames, CaptureOptions};

    let (mut program, mut callbacks) = noon::example_scenes::following_graph_camera::program()
        .map_err(std::io::Error::other)?;
    let options = ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1)?,
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 1_000,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    };
    let mut output = std::io::stdout().lock();
    let summary = capture_frames(&mut program, &mut callbacks, options,
        CaptureOptions::new(320, 180), |frame| output.write_all(frame.rgba))
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    output.flush()?;
    eprintln!("{} frames captured on {:?}; no viewer", summary.sampling.frames, summary.adapter);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {
    // Native output integration is intentionally not a browser entrypoint.
}
