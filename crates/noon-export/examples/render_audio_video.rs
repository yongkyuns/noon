//! Render the shared 30-second background-music scene directly to an MP4.
//!
//! The original score and animation use the same authored timeline as the
//! browser example. Playback buttons are ordinary scene objects and remain
//! visible in the movie; an encoded movie does not make those buttons interactive.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon_export::output::{export_session_file, OutputOptions};
    use noon_export::CaptureOptions;

    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("usage: render_audio_video OUTPUT.mp4")?;
    if args.next().is_some() {
        return Err("usage: render_audio_video OUTPUT.mp4".into());
    }
    let (scene, animation) =
        noon::example_scenes::background_audio::scene().map_err(std::io::Error::other)?;
    let mut session = scene
        .execution_session_with_animation_root(&animation)
        .map_err(std::io::Error::other)?;
    let summary = export_session_file(
        &mut session,
        ExportFrameOptions {
            frame_rate: FrameRate::new(30, 1)?,
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 1_000,
            max_transitions_per_sample: 64,
            final_hold_seconds: 0.0,
        },
        CaptureOptions::new(1280, 720),
        OutputOptions::mp4(path),
    )
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    eprintln!(
        "Finalized {} frames with the authored soundtrack: {}",
        summary.capture.sampling.frames,
        summary.path.display()
    );
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
