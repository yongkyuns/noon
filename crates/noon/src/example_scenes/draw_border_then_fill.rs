//! Forward ordinary-vector Write through shared family DrawBorderThenFill.

use std::rc::Rc;

use crate::{
    AnimationOptions, Color, ContinuationStep, DrawBorderThenFillOptions, LiveContinuation,
    LiveProgram, LiveSession, Mobject, MobjectFamily, RateFunction, Scene,
};

pub struct DrawBorderThenFill {
    family: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for DrawBorderThenFill {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                let segment = live
                    .declare_and_activate_family_draw_border_then_fill(
                        &self.family,
                        DrawBorderThenFillOptions::new(0.04, Some(Color::YELLOW))
                            .with_phase_rate_function(RateFunction::Linear),
                        AnimationOptions::new()
                            .run_time(3.0)
                            .rate_func(RateFunction::Linear)
                            .lag_ratio(0.5)
                            .introducer(true),
                    )
                    .map_err(|error| error.to_string())?;
                self.stage = 1;
                Ok(ContinuationStep::Await(segment))
            }
            1 => {
                self.stage = 2;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            2 => {
                self.stage = 3;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("DrawBorderThenFill continuation resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<DrawBorderThenFill>, String> {
    let scene = Scene::new();
    let mut square = Mobject::manim_square(Rc::clone(scene.store()), 0.8)?;
    square.set_translation(-1.0, 0.0)?;
    square.set_fill(
        f64::from(Color::ORANGE.red),
        f64::from(Color::ORANGE.green),
        f64::from(Color::ORANGE.blue),
        1.0,
    )?;
    square.set_stroke_color(
        f64::from(Color::BLUE.red),
        f64::from(Color::BLUE.green),
        f64::from(Color::BLUE.blue),
        1.0,
    )?;
    square.set_stroke_width(0.06)?;
    let mut circle = Mobject::manim_circle(Rc::clone(scene.store()), 0.4)?;
    circle.set_translation(1.0, 0.0)?;
    circle.set_fill(
        f64::from(Color::PINK.red),
        f64::from(Color::PINK.green),
        f64::from(Color::PINK.blue),
        1.0,
    )?;
    // A zero-width visible stroke proves the outline override appears, then
    // returns to a no-stroke final style during the fill phase.
    circle.set_stroke_width(0.0)?;
    let family = scene.family(&[&square, &circle])?;
    scene
        .into_live_program(DrawBorderThenFill { family, stage: 0 })
        .map_err(|error| error.to_string())
}
