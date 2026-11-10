//! Export the unchanged ordinary source also used by the native window example.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon_export::{
        output::{export_blocking_scene, OutputOptions},
        CaptureOptions,
    };
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let output = match args.as_slice() {
        [path] => OutputOptions::mp4(path),
        [flag, path] if flag == "--png" => OutputOptions::png_sequence(path),
        _ => return Err("usage: first_lesson OUTPUT.mp4 | first_lesson --png OUTPUT_DIR".into()),
    };
    let summary = export_blocking_scene(
        noon::example_scenes::first_lesson::construct,
        ExportFrameOptions {
            frame_rate: FrameRate::new(30, 1)?,
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 3_000,
            max_transitions_per_sample: 64,
            final_hold_seconds: 0.0,
        },
        CaptureOptions::new(320, 180),
        output,
    )?;
    println!(
        "exported {} frames to {}",
        summary.capture.sampling.frames,
        summary.path.display()
    );
    println!("adapter: {:?}", summary.capture.adapter);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
