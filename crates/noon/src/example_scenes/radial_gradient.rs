//! An elliptical field changes center, radii, opacity and paint kind on authored time.
//! Paired with `web/python/examples/radial_gradient.py`.
use crate::{
    AnimationOptions, Color, ContinuationStep, GradientStop, LiveContinuation, LiveProgram,
    LiveSession, Mobject, RadialGradient, RateFunction, Scene,
};
use std::sync::Arc;

pub struct RadialSurface {
    panel: Mobject,
    stage: u8,
}

fn paint(vertical: bool) -> Result<Arc<RadialGradient>, String> {
    let first = if vertical {
        Color::rgba(0.0, 1.0, 0.0, 1.0)
    } else {
        Color::rgba(1.0, 0.0, 0.0, 0.5)
    };
    RadialGradient::new(
        if vertical { [0.65, 0.35] } else { [0.25, 0.5] },
        if vertical { [0.75, 0.4] } else { [0.5, 0.75] },
        [
            GradientStop::new(0.0, first).map_err(message)?,
            GradientStop::new(
                1.0,
                Color::rgba(0.0, 0.0, 1.0, if vertical { 0.0 } else { 1.0 }),
            )
            .map_err(message)?,
        ],
    )
    .map(Arc::new)
    .map_err(message)
}

impl LiveContinuation for RadialSurface {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 => live.wait_segment(0.5),
            1 | 3 | 5 => {
                let target = live.target_editor(&self.panel).map_err(message)?;
                match self.stage {
                    1 => {
                        live.set_fill_radial_gradient(&target, paint(true)?, Some(0.5))
                            .map_err(message)?;
                    }
                    3 => {
                        live.set_fill(&target, 1.0, 0.0, 0.0, 1.0)
                            .map_err(message)?;
                    }
                    _ => {
                        live.set_fill_radial_gradient(&target, paint(false)?, None)
                            .map_err(message)?;
                    }
                }
                live.declare_and_activate_transform_to(
                    &self.panel,
                    &target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )
            }
            2 | 4 | 6 => live.wait_segment(0.25),
            7 => return Ok(ContinuationStep::Finished),
            _ => return Err("painted surface resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<RadialSurface>, String> {
    let mut scene = Scene::new();
    let mut panel =
        Mobject::manim_rectangle(std::rc::Rc::clone(scene.integration_store()), 4.0, 2.0)
            .map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    panel
        .set_fill_radial_gradient(paint(false)?, Some(1.0))
        .map_err(message)?;
    scene.add(&panel).map_err(message)?;
    scene
        .into_live_program(RadialSurface { panel, stage: 0 })
        .map_err(message)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
