//! Ordered ordinary-family subset display through shared Rust semantics.

use std::rc::Rc;

use crate::{
    AnimationOptions, Color, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
    MobjectFamily, RateFunction, Scene, SubsetDisplayMode,
};

pub struct OrdinarySubsetDisplay {
    increasing: MobjectFamily,
    one_by_one: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for OrdinarySubsetDisplay {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let options = AnimationOptions::new()
            .run_time(3.0)
            .rate_func(RateFunction::Linear);
        match self.stage {
            0 => {
                self.stage = 1;
                live.declare_and_activate_family_subset_display(
                    &self.increasing,
                    SubsetDisplayMode::IncreasingFloor,
                    options,
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            1 => {
                self.stage = 2;
                live.declare_and_activate_family_subset_display(
                    &self.one_by_one,
                    SubsetDisplayMode::OneByOneCeil,
                    options,
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                self.stage = 3;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            3 => {
                self.stage = 4;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("ordinary subset-display continuation resumed after completion".into()),
        }
    }
}

fn colored_circle(scene: &Scene, x: f64, y: f64, color: Color) -> Result<Mobject, String> {
    let mut circle = Mobject::manim_circle(Rc::clone(scene.integration_store()), 0.3)?;
    circle.set_translation(x, y)?;
    circle.set_fill(
        f64::from(color.red),
        f64::from(color.green),
        f64::from(color.blue),
        1.0,
    )?;
    Ok(circle)
}

pub fn program() -> Result<LiveProgram<OrdinarySubsetDisplay>, String> {
    let scene = Scene::new();
    let increasing_members = [
        colored_circle(&scene, -1.0, 0.7, Color::RED)?,
        colored_circle(&scene, 0.0, 0.7, Color::GREEN)?,
        colored_circle(&scene, 1.0, 0.7, Color::BLUE)?,
    ];
    let one_by_one_members = [
        colored_circle(&scene, -1.0, -0.7, Color::ORANGE)?,
        colored_circle(&scene, 0.0, -0.7, Color::PINK)?,
        colored_circle(&scene, 1.0, -0.7, Color::YELLOW)?,
    ];
    let increasing = scene.family(&[
        (&increasing_members[0]).into(),
        (&increasing_members[1]).into(),
        (&increasing_members[2]).into(),
    ])?;
    let one_by_one = scene.family(&[
        (&one_by_one_members[0]).into(),
        (&one_by_one_members[1]).into(),
        (&one_by_one_members[2]).into(),
    ])?;
    increasing.prepare_subset_display()?;
    one_by_one.prepare_subset_display()?;
    scene
        .into_live_program(OrdinarySubsetDisplay {
            increasing,
            one_by_one,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}
