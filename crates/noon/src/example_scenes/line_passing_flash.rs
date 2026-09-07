//! Exact analytic Line PassingFlash with transient membership and stable re-entry.

use std::rc::Rc;

use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession,
    ManimGeometryOptions, Mobject, RateFunction, Scene,
};

pub struct LinePassingFlash {
    line: Mobject,
    identity: noon_core::SemanticNodeId,
    stage: u8,
}

impl LiveContinuation for LinePassingFlash {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                if live
                    .contains(&self.line)
                    .map_err(|error| error.to_string())?
                {
                    return Err("PassingFlash Line must begin detached".into());
                }
                self.stage = 1;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            1 => {
                let segment = live
                    .declare_and_activate_passing_flash(
                        &self.line,
                        0.25,
                        AnimationOptions::new()
                            .run_time(2.0)
                            .rate_func(RateFunction::Linear),
                    )
                    .map_err(|error| error.to_string())?;
                if !live
                    .contains(&self.line)
                    .map_err(|error| error.to_string())?
                {
                    return Err("PassingFlash did not admit its transient Line".into());
                }
                self.stage = 2;
                Ok(ContinuationStep::Await(segment))
            }
            2 => {
                if live
                    .contains(&self.line)
                    .map_err(|error| error.to_string())?
                {
                    return Err("PassingFlash did not remove its transient Line".into());
                }
                if self.line.node_id() != self.identity {
                    return Err("PassingFlash changed the Line semantic identity".into());
                }
                live.add(&self.line).map_err(|error| error.to_string())?;
                if !live
                    .contains(&self.line)
                    .map_err(|error| error.to_string())?
                {
                    return Err(
                        "PassingFlash Line could not re-enter with the same identity".into(),
                    );
                }
                self.stage = 3;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            3 => {
                self.stage = 4;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("PassingFlash continuation resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<LinePassingFlash>, String> {
    let scene = Scene::new();
    let mut options = ManimGeometryOptions::line(-2.0, 0.0, 2.0, 0.0)?;
    options.set_scale(1.25, 0.75)?;
    options.set_rotation(std::f64::consts::PI / 6.0)?;
    options.set_translation(0.5, -0.5)?;
    options.disable_fill();
    options.set_stroke_color(0.0, 1.0, 1.0, 1.0)?;
    options.set_stroke_width(0.08)?;
    let line = Mobject::from_manim_geometry(Rc::clone(scene.store()), options)?;
    let identity = line.node_id();
    scene
        .into_live_program(LinePassingFlash {
            line,
            identity,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use crate::{
        AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveProgramStatus,
        Mobject, RustHostCallbackTable, Scene,
    };

    fn admit_completion<C>(
        program: &mut LiveProgram<C>,
        callbacks: &mut RustHostCallbackTable,
        time: f64,
    ) where
        C: LiveContinuation<Error = String>,
    {
        let status = program.drive_to(callbacks, time).unwrap();
        let LiveProgramStatus::PublicationPending(expected) = status else {
            panic!("expected publication at {time}, got {status:?}");
        };
        let context = program.take_renderer_publication().context();
        assert_eq!(context, expected);
        program.admit_publication(context).unwrap();
    }

    #[test]
    fn passing_flash_removes_then_reuses_one_stable_line_slot() {
        let mut program = super::program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(program.session().frame().objects.is_empty());
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        program.take_renderer_publication();
        assert_eq!(
            program.drive_to(&mut callbacks, 0.25).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(program.session().frame().objects.len(), 1);
        assert!(program.session().frame().is_present(0));
        program.take_renderer_publication();
        assert!(matches!(
            program.drive_to(&mut callbacks, 1.25).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(program.session().frame().reveals[0], 0.25);
        admit_completion(&mut program, &mut callbacks, 2.25);
        assert_eq!(program.session().frame().objects.len(), 1);
        assert!(!program.session().frame().is_present(0));
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(program.session().frame().objects.len(), 1);
        assert!(program.session().frame().is_present(0));
        program.take_renderer_publication();
        assert_eq!(
            program.drive_to(&mut callbacks, 2.5).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }

    struct PresentLineFlash {
        line: Mobject,
        activated: bool,
    }

    impl LiveContinuation for PresentLineFlash {
        type Error = String;

        fn resume(
            &mut self,
            live: &mut crate::LiveSession<'_>,
        ) -> Result<ContinuationStep, Self::Error> {
            if self.activated {
                if live
                    .contains(&self.line)
                    .map_err(|error| error.to_string())?
                {
                    return Err("PassingFlash did not remove its initially present Line".into());
                }
                return Ok(ContinuationStep::Finished);
            }
            self.activated = true;
            live.declare_and_activate_passing_flash(
                &self.line,
                0.25,
                AnimationOptions::new().run_time(1.0),
            )
            .map(ContinuationStep::Await)
            .map_err(|error| error.to_string())
        }
    }

    #[test]
    fn passing_flash_accepts_and_removes_an_initially_present_line() {
        let mut scene = Scene::new();
        let line = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
        scene.add(&line).unwrap();
        let mut program = scene
            .into_live_program(PresentLineFlash {
                line,
                activated: false,
            })
            .unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        admit_completion(&mut program, &mut callbacks, 1.0);
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
