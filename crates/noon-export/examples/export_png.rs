//! Export the shared Rust FollowingGraphCamera example to a PNG sequence.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon_export::png::{export_png_sequence, PngSequenceOptions};
    use noon_export::CaptureOptions;

    let mut args = std::env::args_os().skip(1);
    let output = args.next().unwrap_or_else(|| "frames".into());
    if args.next().is_some() {
        return Err("usage: export_png [OUTPUT_DIRECTORY]".into());
    }
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
    let result = export_png_sequence(
        &mut program,
        &mut callbacks,
        frames,
        CaptureOptions::new(1920, 1080),
        PngSequenceOptions::new(output),
    )
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    eprintln!(
        "{} PNG frames finalized in {}",
        result.capture.sampling.frames,
        result.directory.display()
    );
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
