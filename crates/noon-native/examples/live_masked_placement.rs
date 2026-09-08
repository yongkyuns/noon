//! Paired with web/python/examples/live_masked_placement.py.
use noon::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveLayoutTarget, LiveSession, Mobject,
    RateFunction, Scene,
};

struct Placement {
    source: Mobject,
    reference: Mobject,
    stage: u8,
}

impl LiveContinuation for Placement {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.stage == 0 {
            self.stage = 1;
            return live
                .wait_segment(0.25)
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string());
        }
        if self.stage > 2 {
            return Ok(ContinuationStep::Finished);
        }
        let target = live
            .target_editor(&self.source)
            .map_err(|error| error.to_string())?;
        let (destination, mask) = if self.stage == 1 {
            (LiveLayoutTarget::Point(99.0, 5.0), (0.0, 1.0))
        } else {
            (LiveLayoutTarget::Mobject(&self.reference), (0.5, 1.0))
        };
        live.move_to(&target, destination, (0.0, 1.0), mask)
            .map_err(|error| error.to_string())?;
        self.stage += 1;
        live.declare_and_activate_transform_to(
            &self.source,
            &target,
            AnimationOptions::new()
                .run_time(0.5)
                .rate_func(RateFunction::Linear),
        )
        .map(ContinuationStep::Await)
        .map_err(|error| error.to_string())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut source = scene.rectangle(4.0, 2.0)?;
    source.set_translation(2.0, -1.0)?;
    let mut reference = scene.rectangle(2.0, 4.0)?;
    reference.set_translation(-3.0, 3.0)?;
    for object in [&mut source, &mut reference] {
        object.set_color(1.0, 1.0, 1.0, 1.0)?;
        object.set_fill(1.0, 1.0, 1.0, 0.0)?;
    }
    scene.add(&source)?;
    scene.add(&reference)?;
    noon_native::run_live_program(scene.into_live_program(Placement {
        source,
        reference,
        stage: 0,
    })?)?;
    Ok(())
}
