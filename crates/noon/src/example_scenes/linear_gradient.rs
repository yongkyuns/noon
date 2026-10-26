//! A painted surface changes direction, opacity and paint kind on authored time.
//! Paired with `web/python/examples/linear_gradient.py`.
use crate::{
    AnimationOptions, Color, ContinuationStep, GradientStop, LinearGradient, LiveContinuation,
    LiveProgram, LiveSession, Mobject, RateFunction, Scene,
};
use std::sync::Arc;

pub struct PaintedSurface {
    panel: Mobject,
    stage: u8,
    glow: bool,
}

fn paint(vertical: bool) -> Result<Arc<LinearGradient>, String> {
    let first = if vertical {
        Color::rgb(0.0, 1.0, 0.0)
    } else {
        Color::rgb(1.0, 0.0, 0.0)
    };
    LinearGradient::new(
        if vertical { [0.5, 0.0] } else { [0.0, 0.5] },
        if vertical { [0.5, 1.0] } else { [1.0, 0.5] },
        [
            GradientStop::new(0.0, first).map_err(message)?,
            GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).map_err(message)?,
        ],
    )
    .map(Arc::new)
    .map_err(message)
}

impl LiveContinuation for PaintedSurface {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 => live.wait_segment(0.5),
            1 | 3 | 5 => {
                let target = live.target_editor(&self.panel).map_err(message)?;
                match self.stage {
                    1 => {
                        live.set_fill_gradient(
                            &target,
                            paint(true)?,
                            Some(if self.glow { 1.0 } else { 0.5 }),
                        )
                        .map_err(message)?;
                        if self.glow {
                            live.set_glow(
                                &target,
                                crate::effects::EffectUpdate::default()
                                    .radius(crate::effects::Pixels(10.0))
                                    .intensity(1.4),
                            )
                            .map_err(message)?;
                        }
                    }
                    3 => {
                        live.set_fill(&target, 1.0, 0.0, 0.0, 1.0)
                            .map_err(message)?;
                    }
                    _ => {
                        live.set_fill_gradient(&target, paint(false)?, None)
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
            7 if self.glow => {
                live.remove_glow(&self.panel).map_err(message)?;
                live.wait_segment(0.25)
            }
            7 | 8 => return Ok(ContinuationStep::Finished),
            _ => return Err("painted surface resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<PaintedSurface>, String> {
    program_inner(false)
}

/// The same paint lifecycle with a reusable glow, paired with gradient_glow.py.
pub fn glow_program() -> Result<LiveProgram<PaintedSurface>, String> {
    program_inner(true)
}

fn program_inner(glow: bool) -> Result<LiveProgram<PaintedSurface>, String> {
    let mut scene = Scene::new();
    let mut panel =
        Mobject::manim_rectangle(std::rc::Rc::clone(scene.integration_store()), 4.0, 2.0)
            .map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    panel
        .set_fill_gradient(paint(false)?, Some(1.0))
        .map_err(message)?;
    if glow {
        panel
            .set_glow(
                crate::effects::EffectUpdate::default()
                    .color(Color::WHITE)
                    .radius(crate::effects::Pixels(6.0))
                    .intensity(0.4),
            )
            .map_err(message)?;
    }
    scene.add(&panel).map_err(message)?;
    scene
        .into_live_program(PaintedSurface {
            panel,
            stage: 0,
            glow,
        })
        .map_err(message)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
