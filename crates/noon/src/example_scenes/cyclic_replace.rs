//! Cyclic family replacement through ordinary family Transform path arcs.

use std::rc::Rc;

use crate::{
    AnimationOptions, Color, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
    MobjectFamily, RateFunction, Scene,
};

pub struct CyclicReplace {
    first: Mobject,
    second: Mobject,
    third: Mobject,
    source: MobjectFamily,
    shifted_target: MobjectFamily,
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
                    &self.shifted_target,
                    AnimationOptions::new()
                        .run_time(0.5)
                        .rate_func(RateFunction::Linear),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            1 => {
                let centers = [
                    live.effective_layout(&self.first)
                        .map_err(|error| error.to_string())?
                        .center,
                    live.effective_layout(&self.second)
                        .map_err(|error| error.to_string())?
                        .center,
                    live.effective_layout(&self.third)
                        .map_err(|error| error.to_string())?
                        .center,
                ];
                let copied = live
                    .copy_family_with_references(
                        &self.source,
                        &[
                            (&self.first).into(),
                            (&self.second).into(),
                            (&self.third).into(),
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                for (source, center) in [
                    (&self.first, centers[2]),
                    (&self.second, centers[0]),
                    (&self.third, centers[1]),
                ] {
                    let target = copied.mobject(source).map_err(|error| error.to_string())?;
                    live.set_translation(&target, f64::from(center.0), f64::from(center.1))
                        .map_err(|error| error.to_string())?;
                }
                self.stage = 2;
                live.declare_and_activate_family_transform_to(
                    &self.source,
                    copied.root(),
                    AnimationOptions::new()
                        .run_time(2.0)
                        .rate_func(RateFunction::Linear)
                        .path_arc(std::f64::consts::FRAC_PI_2),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                let expected = [(2.5, 0.75), (-1.5, 0.75), (0.5, 0.75)];
                for (object, expected) in [
                    (&self.first, expected[0]),
                    (&self.second, expected[1]),
                    (&self.third, expected[2]),
                ] {
                    let center = live
                        .effective_layout(object)
                        .map_err(|error| error.to_string())?
                        .center;
                    if (center.0 - expected.0).abs() > 1e-5 || (center.1 - expected.1).abs() > 1e-5
                    {
                        return Err("CyclicReplace did not commit the cyclic endpoint".into());
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
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut first = Mobject::manim_square(Rc::clone(scene.integration_store()), 0.7)?;
        let mut second = Mobject::manim_circle(Rc::clone(scene.integration_store()), 0.4)?;
        let mut third = Mobject::manim_square(Rc::clone(scene.integration_store()), 0.5)?;
        for (object, x, color) in [
            (&mut first, -2.0, Color::BLUE),
            (&mut second, 0.0, Color::PINK),
            (&mut third, 2.0, Color::YELLOW),
        ] {
            object.set_translation(x, 0.0)?;
            object.set_fill(
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                0.9,
            )?;
            object.set_stroke_opacity(0.0)?;
            scene.add(object)?;
        }
        let source = scene.family(&[(&first).into(), (&second).into(), (&third).into()])?;
        let shifted_target = source.copy_family()?.root().clone();
        shifted_target.shift(0.5, 0.75)?;
        Ok(scene.into_live_program(CyclicReplace {
            first,
            second,
            third,
            source,
            shifted_target,
            stage: 0,
        })?)
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    fn admit(
        program: &mut LiveProgram<CyclicReplace>,
        callbacks: &mut RustHostCallbackTable,
        time: f64,
    ) {
        let status = program.drive_to(callbacks, time).unwrap();
        let LiveProgramStatus::PublicationPending(expected) = status else {
            panic!("expected publication at {time}, got {status:?}");
        };
        let context = program.take_renderer_publication().context();
        assert_eq!(context, expected);
        program.admit_publication(context).unwrap();
    }

    #[test]
    fn cyclic_target_uses_prior_segment_endpoint_and_curves_between_members() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        admit(&mut program, &mut callbacks, 0.5);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 1.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let first = program.session().frame().render_transform(0).translation;
        assert!((first.x - 0.5).abs() < 1e-5);
        assert!((first.y - 0.75).abs() > 0.1);
        admit(&mut program, &mut callbacks, 2.5);
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
