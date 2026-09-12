//! Shared family become, target animation, and restore on the ordinary live path.
use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession,
    LiveSessionError, ManimBecomeOptions, MobjectFamily, RateFunction, Scene,
};

pub struct FamilyState {
    source: MobjectFamily,
    saved: MobjectFamily,
    target: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for FamilyState {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let step = (|| -> Result<ContinuationStep, LiveSessionError> {
            let segment = match self.stage {
                0 => live.declare_and_activate_family_transform_to(
                    &self.source,
                    &self.target,
                    AnimationOptions::new()
                        .run_time(0.4)
                        .rate_func(RateFunction::Linear),
                )?,
                1 => {
                    live.become_family(&self.source, &self.saved, Default::default())?;
                    live.wait_segment(0.2)?
                }
                _ => return Ok(ContinuationStep::Finished),
            };
            self.stage += 1;
            Ok(ContinuationStep::Await(segment))
        })();
        step.map_err(|error| error.to_string())
    }
}

pub fn program() -> Result<LiveProgram<FamilyState>, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut a = scene.square(0.6)?;
        let mut b = scene.square(0.6)?;
        a.shift(-2.0, 0.0)?;
        for (object, color) in [(&mut a, (1.0, 0.0, 0.0)), (&mut b, (0.0, 0.0, 1.0))] {
            object.set_fill(color.0, color.1, color.2, 1.0)?;
            object.set_stroke_width(0.0)?;
        }
        let nested = scene.family(&[(&a).into(), (&b).into()])?;
        let source = scene.family(&[(&a).into(), (&nested).into()])?;
        let saved = source.copy_family()?.root().clone();
        let replacement = source.copy_family()?.root().clone();
        replacement.shift(1.0, 0.0)?;
        replacement.scale(1.5, 1.5)?;
        replacement.rotate(0.3, crate::ManimRotationPivot::Center)?;
        source.become_family(
            &replacement,
            ManimBecomeOptions {
                stretch: true,
                ..Default::default()
            },
        )?;
        let target = source.copy_family()?.root().clone();
        target.shift(1.0, 1.0)?;
        scene.add_many(&[(&source).into()])?;
        Ok(scene.into_live_program(FamilyState {
            source,
            saved,
            target,
            stage: 0,
        })?)
    };
    build().map_err(|error| error.to_string())
}
