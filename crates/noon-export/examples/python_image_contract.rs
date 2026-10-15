//! Existing shared raster-image source exported independently of Python.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, FrameRate};
    use noon_export::{
        output::{export_file, OutputOptions},
        CaptureOptions,
    };
    let output = std::env::args().nth(1).ok_or("missing output directory")?;
    let mut program = noon::example_scenes::raster_image::program()?;
    let mut capture = CaptureOptions::new(320, 180);
    capture.force_fallback_adapter = match std::env::var("NOON_EXPORT_FORCE_FALLBACK") {
        Ok(value) if value == "0" => false,
        Ok(value) if value == "1" => true,
        Err(std::env::VarError::NotPresent) => true,
        _ => return Err("NOON_EXPORT_FORCE_FALLBACK must be 0 or 1".into()),
    };
    let frames = ExportFrameOptions {
        frame_rate: FrameRate::new(4, 1)?,
        max_frames: 32,
        start_frame: 0,
        stop: noon::integration::ExportStop::SourceEnd,
        max_transitions_per_sample: 4096,
        final_hold_seconds: 0.0,
    };
    let summary = export_file(
        &mut program,
        &mut noon::RustHostCallbackTable::new(),
        frames,
        capture,
        OutputOptions::png_sequence(output),
    )
    .map_err(|e| e.to_string())?;
    println!("{}", summary.capture.sampling.frames);
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
