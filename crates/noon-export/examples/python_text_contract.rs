//! Independent Rust text resource and live-construction pixel counterpart.
#[cfg(all(not(target_arch = "wasm32"), feature = "native-text"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, FrameRate};
    use noon::{
        ContinuationStep, LiveContinuation, LiveSession, LiveSessionError, MarkupText, Mobject,
        Scene, Text,
    };
    use noon_export::{
        output::{export_file, OutputOptions},
        CaptureOptions,
    };
    struct Source {
        label: Mobject,
        stage: u8,
    }
    impl LiveContinuation for Source {
        type Error = LiveSessionError;
        fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            self.stage += 1;
            match self.stage {
                1 => Ok(ContinuationStep::Await(live.wait_segment(0.1)?)),
                2 => {
                    let other = live.create_text(
                        MarkupText::new("<span foreground=\"#ff0000\">Red</span>")
                            .with_font("DejaVu Sans Mono")
                            .with_font_size(30.0)
                            .into(),
                    )?;
                    live.move_to_point(&other, 2.0, 0.0)?;
                    live.add(&other)?;
                    live.shift(&self.label, 1.0, 0.0)?;
                    Ok(ContinuationStep::Await(live.wait_segment(0.1)?))
                }
                _ => Ok(ContinuationStep::Finished),
            }
        }
    }
    let output = std::env::args().nth(1).ok_or("missing output directory")?;
    let mut scene = Scene::new();
    let label = scene.text(
        Text::new("Native text")
            .with_font("DejaVu Sans Mono")
            .with_font_size(30.0),
    )?;
    scene.add(&label)?;
    let mut program = scene.into_live_program(Source { label, stage: 0 })?;
    let mut capture = CaptureOptions::new(320, 180);
    capture.force_fallback_adapter = match std::env::var("NOON_EXPORT_FORCE_FALLBACK") {
        Ok(value) if value == "0" => false,
        Ok(value) if value == "1" => true,
        Err(std::env::VarError::NotPresent) => true,
        _ => return Err("NOON_EXPORT_FORCE_FALLBACK must be 0 or 1".into()),
    };
    let frames = ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1)?,
        max_frames: 16,
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
#[cfg(any(target_arch = "wasm32", not(feature = "native-text")))]
fn main() {}
