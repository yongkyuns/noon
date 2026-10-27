//! Seeded local noise changes frequency, phase, colors and opacity on authored time.
//! Paired with `web/python/examples/noise_paint.py`.
use crate::{
    AnimationOptions, Color, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
    NoisePaint, RateFunction, Scene,
};
use std::sync::Arc;

pub struct NoiseSurface {
    panel: Mobject,
    stage: u8,
}

fn paint(vertical: bool) -> Result<Arc<NoisePaint>, String> {
    let first = if vertical {
        Color::rgba(0.0, 1.0, 0.0, 1.0)
    } else {
        Color::rgba(1.0, 0.0, 0.0, 0.5)
    };
    NoisePaint::new(
        7,
        if vertical { [32.0, 12.0] } else { [8.0, 8.0] },
        if vertical {
            [0.531, -0.719]
        } else {
            [-0.173, 0.231]
        },
        [
            first,
            Color::rgba(0.0, 0.0, 1.0, if vertical { 0.0 } else { 1.0 }),
        ],
    )
    .map(Arc::new)
    .map_err(message)
}

impl LiveContinuation for NoiseSurface {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 => live.wait_segment(0.5),
            1 | 3 | 5 => {
                let target = live.target_editor(&self.panel).map_err(message)?;
                match self.stage {
                    1 => {
                        live.set_fill_noise(&target, paint(true)?, Some(0.5))
                            .map_err(message)?;
                    }
                    3 => {
                        live.set_fill(&target, 1.0, 0.0, 0.0, 1.0)
                            .map_err(message)?;
                    }
                    _ => {
                        live.set_fill_noise(&target, paint(false)?, None)
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

pub fn program() -> Result<LiveProgram<NoiseSurface>, String> {
    let mut scene = Scene::new();
    let mut panel =
        Mobject::manim_rectangle(std::rc::Rc::clone(scene.integration_store()), 4.0, 2.0)
            .map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    panel
        .set_fill_noise(paint(false)?, Some(1.0))
        .map_err(message)?;
    scene.add(&panel).map_err(message)?;
    scene
        .into_live_program(NoiseSurface { panel, stage: 0 })
        .map_err(message)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Finite profile corpus: point/box/coarse filters, full u32 seed and phase wrap.
pub fn profile_scene(profile: u32) -> Result<Scene, String> {
    let (seed, frequency, phase, size) = match profile {
        0 => (0, [0.125; 2], [0.0; 2], [4.0, 2.0]),
        1 => (7, [8.0; 2], [-0.173, 0.231], [4.0, 2.0]),
        2 => (
            u32::MAX,
            [32.0, 12.0],
            [65_535.827, -65_535.769],
            [4.0, 2.0],
        ),
        3 => (7, [256.0; 2], [-0.173, 0.231], [4.0, 2.0]),
        4 => (7, [256.0; 2], [-0.173, 0.231], [0.5; 2]),
        5 => (
            7,
            [8.0; 2],
            [4_294_967_296.173, -4_294_967_296.231],
            [4.0, 2.0],
        ),
        6 => (7, [8.0; 2], [-1e-20, 0.0], [4.0, 2.0]),
        _ => return Err("noise profile must be in [0, 6]".into()),
    };
    let mut scene = Scene::new();
    let mut panel = Mobject::manim_rectangle(
        std::rc::Rc::clone(scene.integration_store()),
        size[0],
        size[1],
    )
    .map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    panel
        .set_fill_noise(
            NoisePaint::new(
                seed,
                frequency,
                phase,
                [Color::rgba(1.0, 0.0, 0.0, 0.5), Color::rgb(0.0, 0.0, 1.0)],
            )
            .map_err(message)?,
            Some(0.7),
        )
        .map_err(message)?;
    panel.set_object_opacity(0.6).map_err(message)?;
    scene.add(&panel).map_err(message)?;
    Ok(scene)
}
