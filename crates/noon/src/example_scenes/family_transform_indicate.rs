//! Ordered family transform followed by a restoring family Indicate.

use std::rc::Rc;

use crate::{
    AnimationOptions, Color, ContinuationStep, IndicateOptions, LiveContinuation, LiveProgram,
    LiveSession, Mobject, MobjectFamily, RateFunction, Scene,
};

pub struct FamilyTransformIndicate {
    left: Mobject,
    right: Mobject,
    source: MobjectFamily,
    target: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for FamilyTransformIndicate {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                let segment = live
                    .declare_and_activate_family_transform_to(
                        &self.source,
                        &self.target,
                        AnimationOptions::new()
                            .run_time(2.0)
                            .rate_func(RateFunction::Linear)
                            .lag_ratio(0.5),
                    )
                    .map_err(|error| error.to_string())?;
                self.stage = 1;
                Ok(ContinuationStep::Await(segment))
            }
            1 => {
                self.require_positions(live, -1.0, 1.0)?;
                let segment = live
                    .declare_and_activate_family_indicate(
                        &self.source,
                        IndicateOptions::default(),
                        AnimationOptions::new()
                            .run_time(2.0)
                            .rate_func(RateFunction::ThereAndBack)
                            .lag_ratio(0.5),
                    )
                    .map_err(|error| error.to_string())?;
                self.stage = 2;
                Ok(ContinuationStep::Await(segment))
            }
            2 => {
                self.require_positions(live, -1.0, 1.0)?;
                self.stage = 3;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            3 => {
                live.shift(&self.left, 1.0, 0.0)
                    .map_err(|error| error.to_string())?;
                self.stage = 4;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            4 => {
                self.require_positions(live, 0.0, 1.0)?;
                self.stage = 5;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("family transform/Indicate continuation resumed after completion".into()),
        }
    }
}

impl FamilyTransformIndicate {
    fn require_positions(
        &self,
        live: &LiveSession<'_>,
        left_x: f32,
        right_x: f32,
    ) -> Result<(), String> {
        let left = live
            .effective(&self.left)
            .map_err(|error| error.to_string())?;
        let right = live
            .effective(&self.right)
            .map_err(|error| error.to_string())?;
        if (left.transform.translation.x - left_x).abs() > 1e-6
            || (right.transform.translation.x - right_x).abs() > 1e-6
        {
            return Err("family operation did not preserve the expected transformed state".into());
        }
        Ok(())
    }
}

pub fn program() -> Result<LiveProgram<FamilyTransformIndicate>, String> {
    let mut scene = Scene::new();
    let mut left = Mobject::manim_square(Rc::clone(scene.store()), 0.6)?;
    let mut right = Mobject::manim_circle(Rc::clone(scene.store()), 0.3)?;
    left.set_translation(-2.0, 0.0)?;
    right.set_translation(0.0, 0.0)?;
    for (object, color) in [(&mut left, Color::PINK), (&mut right, Color::BLUE)] {
        object.set_fill(
            f64::from(color.red),
            f64::from(color.green),
            f64::from(color.blue),
            0.9,
        )?;
        object.set_stroke_opacity(0.0)?;
    }
    scene.add(&left).map_err(|error| error.to_string())?;
    scene.add(&right).map_err(|error| error.to_string())?;
    let source = scene.family(&[&left, &right])?;
    let mut left_target = left.target_editor()?;
    let mut right_target = right.target_editor()?;
    left_target.shift(1.0, 0.0)?;
    right_target.shift(1.0, 0.0)?;
    let target = scene.family(&[&left_target, &right_target])?;
    scene
        .into_live_program(FamilyTransformIndicate {
            left,
            right,
            source,
            target,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    fn admit_completion(
        program: &mut LiveProgram<FamilyTransformIndicate>,
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
    fn native_family_lag_and_indicate_restore_the_transformed_activation_state() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        for (time, left_x, right_x) in [(0.5, -1.625, 0.0), (1.0, -1.25, 0.25)] {
            assert!(matches!(
                program.drive_to(&mut callbacks, time).unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            assert!(
                (program.session().frame().render_transform(0).translation.x - left_x).abs() < 1e-5
            );
            assert!(
                (program.session().frame().render_transform(1).translation.x - right_x).abs()
                    < 1e-5
            );
        }
        admit_completion(&mut program, &mut callbacks, 2.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        for (time, highlighted, resting) in [(8.0 / 3.0, 0, 1), (10.0 / 3.0, 1, 0)] {
            assert!(matches!(
                program.drive_to(&mut callbacks, time).unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            let highlighted = program.session().frame().render_transform(highlighted);
            let resting = program.session().frame().render_transform(resting);
            assert!((highlighted.translation.x.abs() - 1.2).abs() < 1e-5);
            assert!((highlighted.scale.x - 1.2).abs() < 1e-5);
            assert!((resting.translation.x.abs() - 1.0).abs() < 1e-5);
            assert!((resting.scale.x - 1.0).abs() < 1e-5);
        }
        admit_completion(&mut program, &mut callbacks, 4.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(
            program.session().frame().render_transform(0).translation.x,
            -1.0
        );
        assert_eq!(
            program.session().frame().render_transform(1).translation.x,
            1.0
        );
        for time in [4.25, 4.5] {
            match program.drive_to(&mut callbacks, time).unwrap() {
                LiveProgramStatus::PublicationPending(expected) => {
                    let context = program.take_renderer_publication().context();
                    assert_eq!(context, expected);
                    program.admit_publication(context).unwrap();
                }
                LiveProgramStatus::ReadyToResume => {}
                status => panic!("expected wait completion at {time}, got {status:?}"),
            }
            let status = program.resume().unwrap();
            assert_eq!(status == LiveProgramStatus::Finished, time == 4.5);
        }
        assert_eq!(
            program.session().frame().render_transform(0).translation.x,
            0.0
        );
    }
}
