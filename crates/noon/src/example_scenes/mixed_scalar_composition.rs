//! Mixed scalar and object animation through one shared composition and completion.

use std::{f64::consts::PI, rc::Rc};

use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, Color, ContinuationStep,
    LiveContinuation, LiveProgram, LiveSession, Mobject, RateFunction, Scene,
    SemanticAnimationCompositionKind as Kind, SemanticVec3, ValueTracker,
};

pub struct MixedScalarComposition {
    circle: Mobject,
    square: Mobject,
    tracker: ValueTracker,
    stage: u8,
}

impl LiveContinuation for MixedScalarComposition {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                let options = AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear);
                let request = Request::Composition {
                    kind: Kind::Sequence,
                    options: AnimationOptions::new().rate_func(RateFunction::Linear),
                    children: vec![
                        Request::Wait { duration: 0.5 },
                        Request::Composition {
                            kind: Kind::Parallel,
                            options: AnimationOptions::new().rate_func(RateFunction::Smooth),
                            children: vec![
                                Request::ValueTracker {
                                    tracker: &self.tracker,
                                    target: 4.0,
                                    options,
                                },
                                Request::Rotate {
                                    target: &self.square,
                                    angle: PI,
                                    options,
                                },
                            ],
                        },
                    ],
                };
                let segment = live
                    .declare_and_activate_composition(&request, AnimationOptions::new())
                    .map_err(|error| error.to_string())?;
                self.stage = 1;
                Ok(ContinuationStep::Await(segment))
            }
            1 => {
                let circle = live.effective(&self.circle).map_err(|e| e.to_string())?;
                let square = live.effective(&self.square).map_err(|e| e.to_string())?;
                if (circle.transform.translation.x - 2.0).abs() > 1e-6
                    || (square.transform.rotation - PI as f32).abs() > 1e-6
                {
                    return Err("mixed completion did not publish both endpoints".into());
                }
                self.stage = 2;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|e| e.to_string())
            }
            2 => {
                live.set_value(&self.tracker, 3.0)
                    .map_err(|e| e.to_string())?;
                self.stage = 3;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|e| e.to_string())
            }
            3 => {
                let circle = live.effective(&self.circle).map_err(|e| e.to_string())?;
                if (circle.transform.translation.x - 1.0).abs() > 1e-6 {
                    return Err("mixed completion did not release the scalar driver".into());
                }
                self.stage = 4;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("mixed composition resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<MixedScalarComposition>, String> {
    let mut scene = Scene::new();
    let mut circle = Mobject::manim_circle(Rc::clone(scene.store()), 0.3)?;
    let mut square = Mobject::manim_square(Rc::clone(scene.store()), 1.0)?;
    for (object, color) in [(&mut circle, Color::BLUE), (&mut square, Color::PINK)] {
        object.set_fill(
            f64::from(color.red),
            f64::from(color.green),
            f64::from(color.blue),
            0.7,
        )?;
    }
    square.set_translation(0.0, -1.0)?;
    scene.add(&circle).map_err(|e| e.to_string())?;
    scene.add(&square).map_err(|e| e.to_string())?;
    let tracker = scene.value_tracker(0.0)?;
    let position = scene.position_from_tracker(
        &tracker,
        SemanticVec3::new(1.0, 0.0, 0.0),
        SemanticVec3::new(-2.0, 1.0, 0.0),
    )?;
    scene.bind_position(&circle, &position)?;
    scene
        .into_live_program(MixedScalarComposition {
            circle,
            square,
            tracker,
            stage: 0,
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn native_program_shares_mapped_progress_and_releases_both_endpoints() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        // Fixed Manim Smooth quarter sample catches both missing and repeated easing.
        for (time, x, angle) in [
            (0.25, -2.0, 0.0),
            (1.0, -1.7195852, 0.2202373),
            (1.5, 0.0, std::f32::consts::FRAC_PI_2),
        ] {
            assert!(matches!(
                program.drive_to(&mut callbacks, time).unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            let frame = program.session().frame();
            assert!((frame.render_transform(0).translation.x - x).abs() < 1e-5);
            assert!((frame.render_transform(1).rotation - angle).abs() < 1e-5);
        }
        for end in [2.5, 2.75, 3.0] {
            match program.drive_to(&mut callbacks, end).unwrap() {
                LiveProgramStatus::PublicationPending(expected) => {
                    let context = program.take_renderer_publication().context();
                    assert_eq!(context, expected);
                    program.admit_publication(context).unwrap();
                }
                LiveProgramStatus::ReadyToResume if end > 2.5 => {}
                status => panic!("expected completion barrier at {end}, got {status:?}"),
            }
            let status = program.resume().unwrap();
            assert_eq!(status == LiveProgramStatus::Finished, end == 3.0);
        }
        assert_eq!(program.session().frame().time, 3.0);
        assert_eq!(
            program.session().frame().render_transform(0).translation.x,
            1.0
        );
    }
}
