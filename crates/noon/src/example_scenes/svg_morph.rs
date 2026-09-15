//! Direct Rust counterpart of `manim_compatible_svg_tiger_morph.py`.
//!
//! Artwork is supplied by the caller, never bundled or fetched by the engine.
//! The same continuation can be hosted natively or in a direct Rust/WASM host.
use std::rc::Rc;
use crate::{AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession,
    Mobject, MobjectFamily, RateFunction, Scene, SvgImportOptions};

pub struct SvgMorph {
    source: MobjectFamily,
    target: MobjectFamily,
    restored: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for SvgMorph {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 => live.wait_segment(0.5).map_err(|error| error.to_string())?,
            1 | 3 => {
                let target = if self.stage == 1 { &self.target } else { &self.restored };
                live.declare_and_activate_family_transform_to(
                    &self.source, target,
                    AnimationOptions::new().run_time(1.8).rate_func(RateFunction::Smooth),
                ).map_err(|error| error.to_string())?
            }
            2 => live.wait_segment(0.75).map_err(|error| error.to_string())?,
            4 => live.wait_segment(0.85).map_err(|error| error.to_string())?,
            5 => {
                // Completion/hold must not leave execution-only padding hidden.
                live.copy_family(&self.source).map_err(|error| error.to_string())?;
                self.stage = 6;
                return Ok(ContinuationStep::Finished);
            }
            _ => return Err("SVG morph resumed after completion".into()),
        };
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

/// Import two path-only SVG families and perform an ordinary filled round trip.
///
/// For exact gallery qualification pass the same SVG inputs as the Python demo,
/// including its parse-only invisible strokes; no frontend is needed at runtime.
pub fn program(source_svg: &str, target_svg: &str) -> Result<LiveProgram<SvgMorph>, String> {
    let mut scene = Scene::new();
    let options = SvgImportOptions { height: Some(5.2), ..SvgImportOptions::default() };
    let source = scene.svg_from_str_with_options(source_svg, options)
        .map_err(|error| error.to_string())?;
    let target = scene.svg_from_str_with_options(target_svg, options)
        .map_err(|error| error.to_string())?;
    for family in [&source, &target] {
        for &leaf in family.layout().map_err(|error| error.to_string())?.leaves() {
            let mut object = Mobject::from_node(Rc::clone(scene.integration_store()), leaf)
                .map_err(|error| error.to_string())?;
            if object.fill_color().map_err(|error| error.to_string())?.is_none() {
                object.set_fill(0.0, 0.0, 0.0, 0.0).map_err(|error| error.to_string())?;
            }
            // Rust widths are scene units: Manim/Cairo 0.4 maps to 0.004.
            object.set_stroke_width(0.004).map_err(|error| error.to_string())?;
        }
    }
    let restored = source.copy_family().map_err(|error| error.to_string())?.root().clone();
    scene.add_many(&[(&source).into()]).map_err(|error| error.to_string())?;
    scene.into_live_program(SvgMorph { source, target, restored, stage: 0 })
        .map_err(|error| error.to_string())
}
