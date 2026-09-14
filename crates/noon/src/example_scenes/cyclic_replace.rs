//! Shared Rust counterpart of ManimCE CyclicReplace over a flat three-member family.

use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
    MobjectFamily, RateFunction, Scene,
};

pub struct CyclicReplace {
    first: Mobject,
    second: Mobject,
    third: Mobject,
    source: MobjectFamily,
    shifted: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for CyclicReplace {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                self.stage = 1;
                live.declare_and_activate_family_transform_to(
                    &self.source,
                    &self.shifted,
                    AnimationOptions::new()
                        .run_time(0.25)
                        .rate_func(RateFunction::Linear),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            1 => {
                let target = live
                    .cyclic_replace_target(&self.source)
                    .map_err(|error| error.to_string())?;
                self.stage = 2;
                live.declare_and_activate_family_transform_to(
                    &self.source,
                    target.root(),
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear)
                        .path_arc(std::f64::consts::PI / 2.0),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                let expected = [(0.0, 1.0), (2.0, 1.0), (-2.0, 1.0)];
                for (mobject, expected) in [
                    (&self.first, expected[0]),
                    (&self.second, expected[1]),
                    (&self.third, expected[2]),
                ] {
                    let center = live
                        .effective_layout(mobject)
                        .map_err(|error| error.to_string())?
                        .center;
                    if (center.0 - expected.0).abs() > 1.0e-5
                        || (center.1 - expected.1).abs() > 1.0e-5
                    {
                        return Err("CyclicReplace did not preserve cyclic member placement".into());
                    }
                }
                self.stage = 3;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("CyclicReplace continuation resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<CyclicReplace>, String> {
    let mut scene = Scene::new();
    let mut first = scene.circle(0.35).map_err(|error| error.to_string())?;
    let mut second = scene.circle(0.35).map_err(|error| error.to_string())?;
    let mut third = scene.circle(0.35).map_err(|error| error.to_string())?;
    first
        .set_translation(-2.0, 0.0)
        .map_err(|error| error.to_string())?;
    second
        .set_translation(0.0, 0.0)
        .map_err(|error| error.to_string())?;
    third
        .set_translation(2.0, 0.0)
        .map_err(|error| error.to_string())?;
    for member in [&first, &second, &third] {
        scene.add(member).map_err(|error| error.to_string())?;
    }
    let source = scene
        .family(&[(&first).into(), (&second).into(), (&third).into()])
        .map_err(|error| error.to_string())?;
    let shifted = source.copy_family().map_err(|error| error.to_string())?;
    shifted
        .root()
        .shift(0.0, 1.0)
        .map_err(|error| error.to_string())?;
    scene
        .into_live_program(CyclicReplace {
            first,
            second,
            third,
            source,
            shifted: shifted.root().clone(),
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn cyclic_target_captures_the_prior_family_transform_endpoint() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for end in [0.25, 1.25] {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            if let LiveProgramStatus::PublicationPending(expected) =
                program.drive_to(&mut callbacks, end).unwrap()
            {
                let publication = program.take_renderer_publication().context();
                assert_eq!(publication, expected);
                program.admit_publication(publication).unwrap();
            }
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
