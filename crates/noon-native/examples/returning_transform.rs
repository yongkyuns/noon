//! Paired with web/python/examples/returning_transform.py.
use noon::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveSession, Mobject, RateFunction,
    Scene, SemanticAnimationCompositionKind, TransformToRequest,
};

struct ReturningTargets {
    circle: Mobject,
    first: Mobject,
    second: Mobject,
    activated: bool,
}

impl LiveContinuation for ReturningTargets {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.activated {
            return Ok(ContinuationStep::Finished);
        }
        self.activated = true;
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let children = [
            TransformToRequest::new(&self.circle, &self.first, options),
            TransformToRequest::new(
                &self.circle,
                &self.second,
                options.rate_func(RateFunction::ThereAndBack),
            ),
        ];
        let segment = live
            .declare_and_activate_transform_composition(
                SemanticAnimationCompositionKind::Sequence,
                &children,
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new(),
            )
            .map_err(|error| error.to_string())?;
        Ok(ContinuationStep::Await(segment))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.4)?;
    circle.set_fill(1.0, 1.0, 1.0, 1.0)?;
    let mut first = circle.target_editor()?;
    first.set_translation(2.0, 1.0)?;
    let mut second = circle.target_editor()?;
    second.set_translation(4.0, 3.0)?;
    scene.add(&circle)?;
    noon_native::run_live_program(scene.into_live_program(ReturningTargets {
        circle,
        first,
        second,
        activated: false,
    })?)?;
    Ok(())
}
