//! A centered full turn, then a signed quarter turn through one shared runtime.
use crate::{
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
    LiveSession, ManimRotationPivot, Mobject, RateFunction, Scene,
};

pub struct OrdinaryRotating {
    rectangle: Mobject,
    stage: u8,
}
impl LiveContinuation for OrdinaryRotating {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let result = match self.stage {
            0 => live.wait_segment(0.25),
            1 | 2 => live.declare_and_activate_composition(
                &AnimationCompositionRequest::ManimRotate {
                    target: &self.rectangle,
                    angle: if self.stage == 1 {
                        std::f64::consts::TAU
                    } else {
                        -std::f64::consts::FRAC_PI_2
                    },
                    pivot: if self.stage == 1 {
                        ManimRotationPivot::Center
                    } else {
                        ManimRotationPivot::Point(2.0, 1.0)
                    },
                    options: AnimationOptions::new()
                        .run_time(if self.stage == 1 { 2.0 } else { 1.0 })
                        .rate_func(RateFunction::Linear),
                },
                AnimationOptions::new(),
            ),
            3 => live.wait_segment(0.25),
            4 => {
                self.stage = 5;
                return Ok(ContinuationStep::Finished);
            }
            _ => return Err("rotation continuation resumed after completion".into()),
        };
        self.stage += 1;
        result
            .map(ContinuationStep::Await)
            .map_err(|error| error.to_string())
    }
}

pub fn program() -> Result<LiveProgram<OrdinaryRotating>, String> {
    let mut scene = Scene::new();
    let mut rectangle = scene
        .rectangle(3.0, 0.6)
        .map_err(|error| error.to_string())?;
    rectangle
        .set_translation(2.0, 1.0)
        .map_err(|error| error.to_string())?;
    rectangle
        .set_fill_color(0.0, 1.0, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    rectangle
        .set_fill_opacity(1.0)
        .map_err(|error| error.to_string())?;
    rectangle
        .disable_stroke()
        .map_err(|error| error.to_string())?;
    scene.add(&rectangle).map_err(|error| error.to_string())?;
    scene
        .into_live_program(OrdinaryRotating {
            rectangle,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}
