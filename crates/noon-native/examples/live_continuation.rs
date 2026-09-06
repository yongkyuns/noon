//! Native Rust continuation over the same ordinary timeline/runtime as static scenes.

use noon::{
    AnimationOptions, Color, ContinuationStep, LiveContinuation, LiveSession, LiveSessionError,
    Mobject, RateFunction, Scene,
};

struct MovingCircle {
    circle: Mobject,
    target: Mobject,
    step: usize,
}

impl LiveContinuation for MovingCircle {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        match self.step {
            0 => {
                self.step = 1;
                Ok(ContinuationStep::Await(
                    live.declare_and_activate_transform_to(
                        &self.circle,
                        &self.target,
                        AnimationOptions::new()
                            .run_time(2.0)
                            .rate_func(RateFunction::Linear),
                    )?,
                ))
            }
            1 => {
                self.step = 2;
                Ok(ContinuationStep::Await(live.wait_segment(0.5)?))
            }
            2 => {
                self.step = 3;
                live.set_color(&self.circle, Color::YELLOW)?;
                Ok(ContinuationStep::Finished)
            }
            _ => unreachable!("finished continuation must not resume"),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let circle = scene.circle(0.7)?;
    scene.add(&circle)?;
    let mut target = circle.target_editor()?;
    target.set_translation(2.0, 1.0)?;

    let program = scene.into_live_program(MovingCircle {
        circle,
        target,
        step: 0,
    })?;
    noon_native::run_live_program(program)?;
    Ok(())
}
