//! Animated painted-source shadow, paired with web/python/examples/shadow.py.
use super::image_glow::image_options;
use crate::{
    effects::{EffectOffset, EffectUpdate, Pixels, Shadow},
    AnimationOptions, Color, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
    RateFunction, Scene,
};

pub struct ImageShadow {
    image: Mobject,
    stage: u8,
}

impl LiveContinuation for ImageShadow {
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
                    "depth",
                    EffectUpdate::default()
                        .radius(Pixels(10.0))
                        .offset(EffectOffset::Pixels([-12.0, 12.0]))
                        .color(Color::rgba(0.0, 0.0, 1.0, 0.8)),
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
                    "depth",
                    EffectUpdate::default()
                        .radius(Pixels(4.0))
                        .offset(EffectOffset::Pixels([12.0, -10.0]))
                        .color(Color::rgba(0.0, 0.0, 0.0, 0.0)),
                )
                .map_err(message)?;
                live.declare_and_activate_transform_to(&self.image, &target, motion)
            }
            5 => {
                // Removing a neutral shadow preserves the same ordinary image.
                live.remove_effect(&self.image, "depth").map_err(message)?;
                live.wait_segment(0.75)
            }
            6 => return Ok(ContinuationStep::Finished),
            _ => return Err("image shadow continuation resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<ImageShadow>, String> {
    let mut scene = Scene::new();
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background
        .set_fill(224.0 / 255.0, 232.0 / 255.0, 240.0 / 255.0, 1.0)
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
            Shadow::new(
                EffectUpdate::default()
                    .radius(Pixels(4.0))
                    .offset(EffectOffset::Pixels([12.0, -10.0]))
                    .color(Color::rgba(0.0, 0.0, 0.0, 0.65)),
            )
            .map_err(message)?,
            "depth",
        )
        .map_err(message)?;
    scene.add(&image).map_err(message)?;
    scene
        .into_live_program(ImageShadow { image, stage: 0 })
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
        program: &mut LiveProgram<ImageShadow>,
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
    fn paired_shadow_program_animates_neutralizes_and_removes_one_attachment() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        program.resume().unwrap();
        for (time, radius) in [
            (0.5, 4.0),
            (0.75, 4.0),
            (1.5, 7.0),
            (2.25, 10.0),
            (2.5, 10.0),
            (3.0, 10.0),
            (3.75, 7.0),
            (4.5, 4.0),
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
                    .as_shadow()
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
