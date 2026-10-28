//! Animated painted-source blur, paired with web/python/examples/blur.py.
use super::image_glow::image_options;
use crate::{
    effects::{Blur, EffectUpdate, Pixels},
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
    RateFunction, Scene,
};

pub struct ImageBlur {
    image: Mobject,
    stage: u8,
}

impl LiveContinuation for ImageBlur {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let motion = AnimationOptions::new()
            .run_time(1.5)
            .rate_func(RateFunction::Smooth);
        let segment = match self.stage {
            0 | 2 | 4 => live.wait_segment(0.75),
            1 => {
                let target = live.target_editor(&self.image).map_err(message)?;
                live.shift(&target, 0.0, 0.6).map_err(message)?;
                live.rotate(&target, std::f64::consts::PI / 12.0)
                    .map_err(message)?;
                live.set_effect(
                    &target,
                    "soften",
                    EffectUpdate::default().radius(Pixels(20.0)),
                )
                .map_err(message)?;
                live.declare_and_activate_transform_to(&self.image, &target, motion)
            }
            3 => {
                // Return to the original pose while making the effect neutral.
                let target = live.target_editor(&self.image).map_err(message)?;
                live.shift(&target, 0.0, -0.6).map_err(message)?;
                live.rotate(&target, -std::f64::consts::PI / 12.0)
                    .map_err(message)?;
                live.set_effect(
                    &target,
                    "soften",
                    EffectUpdate::default().radius(Pixels(0.0)),
                )
                .map_err(message)?;
                live.declare_and_activate_transform_to(&self.image, &target, motion)
            }
            5 => {
                // Removing a neutral blur preserves the same ordinary image.
                live.remove_effect(&self.image, "soften").map_err(message)?;
                live.wait_segment(0.75)
            }
            6 => return Ok(ContinuationStep::Finished),
            _ => return Err("image blur continuation resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<ImageBlur>, String> {
    let mut scene = Scene::new();
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background
        .set_fill(8.0 / 255.0, 16.0 / 255.0, 31.0 / 255.0, 1.0)
        .map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;

    // Both objects share one immutable image resource; the left is a reference.
    let options = image_options()?;
    let mut reference = scene.image(options.clone()).map_err(message)?;
    reference.set_translation(-2.2, 0.0).map_err(message)?;
    scene.add(&reference).map_err(message)?;
    let mut image = scene.image(options).map_err(message)?;
    image.set_translation(2.2, 0.0).map_err(message)?;
    image
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(10.0))).map_err(message)?,
            "soften",
        )
        .map_err(message)?;
    scene.add(&image).map_err(message)?;
    scene
        .into_live_program(ImageBlur { image, stage: 0 })
        .map_err(message)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    fn drive(
        program: &mut LiveProgram<ImageBlur>,
        callbacks: &mut RustHostCallbackTable,
        time: f64,
    ) -> LiveProgramStatus {
        let status = program.drive_to(callbacks, time).unwrap();
        if let LiveProgramStatus::PublicationPending(expected) = status {
            let context = program.take_renderer_publication().context();
            assert_eq!(context, expected);
            program.admit_publication(context).unwrap();
            program.resume().unwrap()
        } else if status == LiveProgramStatus::ReadyToResume {
            program.resume().unwrap()
        } else {
            status
        }
    }

    #[test]
    fn paired_blur_program_animates_neutralizes_and_removes_one_attachment() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        program.resume().unwrap();
        for (time, radius) in [
            (0.5, 10.0),
            (0.75, 10.0),
            (1.5, 15.0),
            (2.25, 20.0),
            (2.5, 20.0),
            (3.0, 20.0),
            (3.75, 10.0),
            (4.5, 0.0),
        ] {
            drive(&mut program, &mut callbacks, time);
            let frame = program.session().frame();
            assert_eq!(
                frame
                    .objects
                    .iter()
                    .filter(|row| row.effects.is_some())
                    .count(),
                1
            );
            assert_eq!(
                crate::effect_authoring::only_effect(frame.objects[2].effects.as_ref().unwrap())
                    .definition
                    .as_blur()
                    .unwrap()
                    .radius(),
                Pixels(radius).into()
            );
        }
        drive(&mut program, &mut callbacks, 5.25);
        assert_eq!(
            drive(&mut program, &mut callbacks, 6.0),
            LiveProgramStatus::Finished
        );
        assert!(program
            .session()
            .frame()
            .objects
            .iter()
            .all(|row| row.effects.is_none()));
    }
}
