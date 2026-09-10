//! Fixed-point spotlight with a live continuation on both direct Rust hosts.
use crate::{
    AnimationOptions, Color, ContinuationStep, FocusOnOptions, LiveContinuation, LiveProgram,
    LiveSession, Mobject, RateFunction, Scene,
};

pub struct OrdinaryFocusOn {
    square: Mobject,
    stage: u8,
}
impl LiveContinuation for OrdinaryFocusOn {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let result = match self.stage {
            0 => live.wait_segment(0.25),
            1 => live.declare_and_activate_focus_on(
                FocusOnOptions {
                    point: (2.0, 1.0),
                    opacity: 0.8,
                    color: Color {
                        red: 0.0,
                        green: 1.0,
                        blue: 1.0,
                        alpha: 1.0,
                    },
                },
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            ),
            2 => {
                if !live
                    .contains(&self.square)
                    .map_err(|error| error.to_string())?
                {
                    return Err("FocusOn removed unrelated scene content".into());
                }
                live.wait_segment(0.25)
            }
            3 => {
                self.stage = 4;
                return Ok(ContinuationStep::Finished);
            }
            _ => return Err("FocusOn continuation resumed after completion".into()),
        };
        self.stage += 1;
        result
            .map(ContinuationStep::Await)
            .map_err(|error| error.to_string())
    }
}

pub fn program() -> Result<LiveProgram<OrdinaryFocusOn>, String> {
    let mut scene = Scene::new();
    let mut square = scene.square(1.0).map_err(|error| error.to_string())?;
    square
        .set_translation(-3.0, -2.0)
        .map_err(|error| error.to_string())?;
    square
        .set_fill_color(0.0, 0.0, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    square
        .set_fill_opacity(1.0)
        .map_err(|error| error.to_string())?;
    square.disable_stroke().map_err(|error| error.to_string())?;
    scene.add(&square).map_err(|error| error.to_string())?;
    scene
        .into_live_program(OrdinaryFocusOn { square, stage: 0 })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use crate::{LiveProgramStatus, RustHostCallbackTable};
    #[test]
    fn focus_on_stages_one_spotlight_and_removes_only_its_membership() {
        let mut program = super::program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        program.resume().unwrap();
        program.take_renderer_publication();
        assert_eq!(
            program.drive_to(&mut callbacks, 0.25).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        program.resume().unwrap();
        program.take_renderer_publication();
        program.drive_to(&mut callbacks, 1.25).unwrap();
        let frame = program.session().frame();
        assert_eq!(frame.objects.len(), 2);
        assert!(frame.is_present(0) && frame.is_present(1));
        let spotlight = &frame.objects[1];
        assert_eq!(spotlight.transform.translation, crate::Vec2::new(1.0, 0.5));
        assert_eq!(spotlight.transform.scale, crate::Vec2::new(0.5, 0.5));
        assert!((spotlight.style.fill.unwrap().alpha - 0.4).abs() < 1e-6);
        let LiveProgramStatus::PublicationPending(expected) =
            program.drive_to(&mut callbacks, 2.25).unwrap()
        else {
            panic!("missing endpoint publication")
        };
        let context = program.take_renderer_publication().context();
        assert_eq!(context, expected);
        program.admit_publication(context).unwrap();
        assert!(program.session().frame().is_present(0));
        assert!(!program.session().frame().is_present(1));
        program.resume().unwrap();
        program.take_renderer_publication();
        program.drive_to(&mut callbacks, 2.5).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
