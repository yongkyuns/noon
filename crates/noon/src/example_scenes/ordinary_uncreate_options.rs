//! Uncreate easing, retained membership, and forward reveal on the shared runtime.

use crate::{
    AnimationOptions, Color, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
    MobjectFamilyMember, RateFunction, Scene,
};

pub struct UncreateOptions {
    first: Mobject,
    kept: Mobject,
    forward: Mobject,
    stage: u8,
}

impl LiveContinuation for UncreateOptions {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let (target, options) = match self.stage {
            0 => {
                live.add_many(&[
                    MobjectFamilyMember::Mobject(&self.first),
                    MobjectFamilyMember::Mobject(&self.kept),
                    MobjectFamilyMember::Mobject(&self.forward),
                ])
                .map_err(|error| error.to_string())?;
                (
                    &self.first,
                    AnimationOptions::new()
                        .run_time(2.0)
                        .rate_func(RateFunction::RushInto),
                )
            }
            1 => {
                if live
                    .contains(&self.first)
                    .map_err(|error| error.to_string())?
                {
                    return Err("default Uncreate did not remove its target".into());
                }
                (
                    &self.kept,
                    AnimationOptions::new().run_time(1.0).remover(false),
                )
            }
            2 => {
                if !live
                    .contains(&self.kept)
                    .map_err(|error| error.to_string())?
                {
                    return Err("Uncreate with remover=false removed its target".into());
                }
                (
                    &self.forward,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .reverse_rate_function(false),
                )
            }
            3 => {
                if live
                    .contains(&self.forward)
                    .map_err(|error| error.to_string())?
                {
                    return Err("forward Uncreate did not remove its target".into());
                }
                self.stage += 1;
                return Ok(ContinuationStep::Finished);
            }
            _ => return Err("Uncreate continuation resumed after completion".into()),
        };
        let segment = live
            .declare_and_activate_uncreate(target, options)
            .map_err(|error| error.to_string())?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<UncreateOptions>, String> {
    let scene = Scene::new();
    let mut first = scene.square(0.6)?;
    let mut kept = scene.circle(0.25)?;
    let mut forward = scene.square(0.4)?;
    for (object, color) in [
        (&mut first, Color::BLUE),
        (&mut kept, Color::PINK),
        (&mut forward, Color::GREEN),
    ] {
        object.set_color(
            f64::from(color.red),
            f64::from(color.green),
            f64::from(color.blue),
            1.0,
        )?;
    }
    scene
        .into_live_program(UncreateOptions {
            first,
            kept,
            forward,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}
