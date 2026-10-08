//! Independent compiled Rust counterpart of the native Python export fixture.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon::{ContinuationStep, LiveContinuation, LiveSession, LiveSessionError, Mobject, Scene};
    use noon_export::{output::{export_file, OutputOptions}, CaptureOptions};
    struct Source { marker: Mobject, stage: u8 }
    impl LiveContinuation for Source {
        type Error = LiveSessionError;
        fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            let stage = self.stage;
            self.stage += 1;
            match stage {
                0 => Ok(ContinuationStep::Await(live.wait_segment(0.105)?)),
                1 => {
                    live.set_translation(&self.marker, 0.5, 1.0)?;
                    Ok(ContinuationStep::Await(live.wait_segment(0.207)?))
                }
                _ => {
                    live.set_translation(&self.marker, 2.0, -1.0)?;
                    Ok(ContinuationStep::Finished)
                }
            }
        }
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 { return Err("usage: python_export_contract OUTPUT P Q START HOLD".into()); }
    let mut scene = Scene::new();
    let mut marker = scene.square(0.75)?;
    marker.set_fill(0.0, 0.0, 1.0, 1.0)?;
    marker.set_stroke_width(0.0)?;
    marker.set_translation(-2.0, 0.0)?;
    scene.add(&marker)?;
    let mut program = scene.into_live_program(Source { marker, stage: 0 })?;
    let frames = ExportFrameOptions {
        frame_rate: FrameRate::new(args[1].parse()?, args[2].parse()?)?,
        start_frame: args[3].parse()?,
        stop: ExportStop::SourceEnd,
        max_frames: 64,
        max_transitions_per_sample: 4096,
        final_hold_seconds: args[4].parse()?,
    };
    let mut capture = CaptureOptions::new(65, 33);
    capture.force_fallback_adapter = true;
    let summary = export_file(&mut program, &mut noon::RustHostCallbackTable::new(), frames,
        capture, OutputOptions::png_sequence(&args[0])).map_err(|e| e.to_string())?;
    println!("{}", summary.capture.sampling.frames);
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
