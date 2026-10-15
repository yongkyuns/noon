//! Independent static SVG import and existing live-family mutation counterpart.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, FrameRate};
    use noon::{
        Color, ContinuationStep, LiveContinuation, LiveSession, LiveSessionError, MobjectFamily,
        Scene,
    };
    use noon_export::{
        output::{export_file, OutputOptions},
        CaptureOptions,
    };
    struct Source {
        svg: MobjectFamily,
        stage: u8,
    }
    impl LiveContinuation for Source {
        type Error = LiveSessionError;
        fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            self.stage += 1;
            match self.stage {
                1 => Ok(ContinuationStep::Await(live.wait_segment(0.1)?)),
                2 => {
                    live.shift_family(&self.svg, 1.0, 0.0)?;
                    live.set_family_fill(&self.svg, Some(Color::rgb(0.0, 1.0, 0.0)), None)?;
                    Ok(ContinuationStep::Await(live.wait_segment(0.1)?))
                }
                _ => Ok(ContinuationStep::Finished),
            }
        }
    }
    let output = std::env::args().nth(1).ok_or("missing output directory")?;
    let mut scene = Scene::new();
    let svg = scene.svg_from_str(r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><rect width="12" height="20" fill="#ff0000"/><path d="M20 20 L40 20 L30 0 Z" fill="#0000ff" fill-opacity=".5"/></svg>"##)?;
    scene.add_many(&[(&svg).into()])?;
    let mut program = scene.into_live_program(Source { svg, stage: 0 })?;
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
#[cfg(target_arch = "wasm32")]
fn main() {}
