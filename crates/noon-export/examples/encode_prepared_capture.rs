//! Encode explicit RGBA/PCM media captured by an external execution owner.
//!
//! The producer uses the normal shared runtime, frame policy, renderer and mixer.
//! This small consumer uses the same native MP4 sink as native scene export.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon_export::output::{export_prepared_capture, OutputOptions, PreparedCaptureLimits};
    use noon_export::CaptureCancellation;

    const USAGE: &str = "usage: encode_prepared_capture DIRECTORY OUTPUT.mp4 [--overwrite]";
    let mut args = std::env::args_os().skip(1);
    let directory = args.next().ok_or(USAGE)?;
    let path = args.next().ok_or(USAGE)?;
    let mut output = OutputOptions::mp4(path);
    match args.next() {
        None => {}
        Some(flag) if flag == "--overwrite" => output.overwrite = true,
        Some(_) => return Err(USAGE.into()),
    }
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    let summary = export_prepared_capture(
        directory,
        output,
        PreparedCaptureLimits::default(),
        CaptureCancellation::default(),
    )?;
    let sampling = summary.sampling;
    println!(
        "{}",
        serde_json::json!({
            "path": summary.path,
            "frames": sampling.frames,
            "width": summary.width,
            "height": summary.height,
            "frame_rate": {
                "numerator": sampling.frame_rate.numerator(),
                "denominator": sampling.frame_rate.denominator(),
            },
            "start_time": sampling.start_time,
            "end_time": sampling.end_time,
            "source_end": sampling.source_end,
            "scheduled_duration": sampling.scheduled_duration,
            "audio_frames": summary.audio_frames,
        })
    );
    if !summary.encoder_diagnostics.is_empty() {
        eprintln!("{}", summary.encoder_diagnostics);
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
