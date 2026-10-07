//! Export the ordinary Rust camera program with built-in file finalization.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon_export::output::{export_file, OutputOptions};
    use noon_export::CaptureOptions;
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("usage: render_video OUTPUT.mp4 [--png]")?;
    let output = match args.next() {
        None => OutputOptions::mp4(path),
        Some(flag) if flag == "--png" => OutputOptions::png_sequence(path),
        _ => return Err("unknown output option".into()),
    };
    if args.next().is_some() { return Err("too many arguments".into()); }
    let (mut program, mut callbacks) = noon::example_scenes::following_graph_camera::program()
        .map_err(std::io::Error::other)?;
    let frames = ExportFrameOptions { frame_rate: FrameRate::new(30, 1)?, start_frame: 0,
        stop: ExportStop::SourceEnd, max_frames: 1_000, max_transitions_per_sample: 64,
        final_hold_seconds: 0.0 };
    let summary = export_file(&mut program, &mut callbacks, frames, CaptureOptions::new(1280, 720), output)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    eprintln!("Finalized {} frames: {}", summary.capture.sampling.frames, summary.path.display());
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
