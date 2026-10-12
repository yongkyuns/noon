//! Shared Rust counterpart of the Manim-compatible ApplyMatrix shear example.
use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
    RateFunction, Scene,
};

pub struct ApplyMatrix {
    square: Mobject,
    stage: u8,
}

impl LiveContinuation for ApplyMatrix {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                let target = live
                    .target_editor(&self.square)
                    .map_err(|e| e.to_string())?;
                live.set_translation(&target, 1.0, 1.0)
                    .map_err(|e| e.to_string())?;
                self.stage = 1;
                live.declare_and_activate_transform_to(
                    &self.square,
                    &target,
                    AnimationOptions::new()
                        .run_time(0.25)
                        .rate_func(RateFunction::Linear),
                )
                .map(ContinuationStep::Await)
                .map_err(|e| e.to_string())
            }
            1 => {
                let target = live
                    .target_editor(&self.square)
                    .map_err(|e| e.to_string())?;
                live.apply_matrix(&target, &[1.0, 1.0, 0.0, 1.0], 2, 2, 0.0, 0.0)
                    .map_err(|e| e.to_string())?;
                self.stage = 2;
                live.declare_and_activate_transform_to(
                    &self.square,
                    &target,
                    AnimationOptions::new()
                        .run_time(3.0)
                        .rate_func(RateFunction::Smooth),
                )
                .map(ContinuationStep::Await)
                .map_err(|e| e.to_string())
            }
            2 => {
                let layout = live
                    .effective_layout(&self.square)
                    .map_err(|e| e.to_string())?;
                if (layout.center.0 - 2.0).abs() > 1.0e-5
                    || (layout.center.1 - 1.0).abs() > 1.0e-5
                    || (layout.width - 4.0).abs() > 1.0e-5
                    || (layout.height - 2.0).abs() > 1.0e-5
                {
                    return Err(
                        "ApplyMatrix did not capture and shear the prior segment endpoint".into(),
                    );
                }
                self.stage = 3;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("ApplyMatrix continuation resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<ApplyMatrix>, String> {
    let mut scene = Scene::new();
    let mut square = scene.square(2.0).map_err(|error| error.to_string())?;
    square
        .set_fill_color(0.0, 0.0, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    square
        .set_fill_opacity(0.6)
        .map_err(|error| error.to_string())?;
    scene.add(&square).map_err(|error| error.to_string())?;
    scene
        .into_live_program(ApplyMatrix { square, stage: 0 })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn apply_matrix_target_captures_prior_segment_endpoint() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for end in [0.25, 3.25] {
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
