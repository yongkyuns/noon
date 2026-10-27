//! A hatched surface uses ordinary fill targets and authored playback time.
//! Paired with `web/python/examples/hatch_paint.py`.
use crate::{
    AnimationOptions, Color, ContinuationStep, HatchPaint, HatchPaintOptions, LiveContinuation,
    LiveProgram, LiveSession, Mobject, RateFunction, Scene,
};

pub struct HatchedSurface {
    panel: Mobject,
    stage: u8,
}

fn paint(second: bool) -> Result<HatchPaint, String> {
    HatchPaint::new(
        if second {
            Color::rgb(0.0, 1.0, 0.0)
        } else {
            Color::rgba(1.0, 0.0, 0.0, 0.5)
        },
        Color::rgba(0.0, 0.0, 1.0, if second { 0.0 } else { 1.0 }),
        HatchPaintOptions {
            spacing: if second { 0.35 } else { 0.6 },
            width: if second { 0.12 } else { 0.09 },
            angle: if second { 0.85 } else { 0.2 },
            phase: if second {
                [0.531, -0.719]
            } else {
                [-0.173, 0.231]
            },
            cross: true,
        },
    )
    .map_err(message)
}

impl LiveContinuation for HatchedSurface {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 => live.wait_segment(0.5),
            1 | 3 | 5 => {
                let target = live.target_editor(&self.panel).map_err(message)?;
                match self.stage {
                    1 => {
                        live.set_fill_hatch(&target, paint(true)?, Some(0.5))
                            .map_err(message)?;
                    }
                    3 => {
                        live.set_fill(&target, 1.0, 0.0, 0.0, 1.0)
                            .map_err(message)?;
                    }
                    _ => {
                        live.set_fill_hatch(&target, paint(false)?, None)
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
            _ => return Err("hatched surface resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<HatchedSurface>, String> {
    let mut scene = Scene::new();
    let mut panel = scene.rectangle(4.0, 2.0).map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    panel
        .set_fill_hatch(paint(false)?, Some(1.0))
        .map_err(message)?;
    scene.add(&panel).map_err(message)?;
    scene
        .into_live_program(HatchedSurface { panel, stage: 0 })
        .map_err(message)
}

/// Static corpus covers stripes/crosshatching, wrapped phase, duty endpoints,
/// antialiasing, tiny patterns and local geometry under a nonuniform pose.
pub fn profile_scene(profile: u32) -> Result<Scene, String> {
    let (spacing, width, angle, phase, cross, posed) = match profile {
        0 => (0.6, 0.09, 0.2, [-0.173, 0.231], false, false),
        1 => (0.6, 0.09, 0.2, [-0.173, 0.231], true, false),
        2 => (0.035, 0.011, 0.7, [-0.173, 0.231], true, false),
        3 => (0.0001, 0.000035, 0.7, [-0.173, 0.231], true, false),
        4 => (0.6, 0.0, 0.7, [0.0; 2], true, false),
        5 => (0.6, 0.6, 0.7, [0.0; 2], true, false),
        6 => (
            0.6,
            0.09,
            0.2,
            [4_294_967_296.173, -4_294_967_296.231],
            true,
            false,
        ),
        7 => (0.6, 0.09, 0.2, [-1e-20, 0.0], true, false),
        8 => (0.35, 0.12, 0.85, [0.531, -0.719], true, true),
        _ => return Err("hatch profile must be in [0, 8]".into()),
    };
    let mut scene = Scene::new();
    let mut panel = scene.rectangle(4.0, 2.0).map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    panel
        .set_fill_hatch(
            HatchPaint::new(
                Color::rgba(1.0, 0.0, 0.0, 0.5),
                Color::rgb(0.0, 0.0, 1.0),
                HatchPaintOptions {
                    spacing,
                    width,
                    angle,
                    phase,
                    cross,
                },
            )
            .map_err(message)?,
            Some(0.7),
        )
        .map_err(message)?;
    panel.set_object_opacity(0.6).map_err(message)?;
    if posed {
        panel.set_scale(-0.8, 1.2).map_err(message)?;
        panel.set_rotation(0.3).map_err(message)?;
        panel.set_translation(0.2, -0.1).map_err(message)?;
    }
    scene.add(&panel).map_err(message)?;
    Ok(scene)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
