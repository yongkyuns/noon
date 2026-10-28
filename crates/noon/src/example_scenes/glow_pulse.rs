//! Persistent and temporary glow pulses use ordinary timing and composition.
//! Paired with `web/python/examples/glow_pulse.py`.

use crate::{
    effects::{Blur, EffectUpdate, Pixels},
    AnimationCompositionRequest as Request, AnimationOptions, Color, ContinuationStep,
    LiveContinuation, LiveProgram, LiveSession, Mobject, RateFunction, Scene,
    SemanticAnimationCompositionKind, TransformToRequest,
};

pub struct GlowPulseScene {
    persistent: Mobject,
    temporary: Mobject,
    stage: u8,
}

fn linear(seconds: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(seconds)
        .rate_func(RateFunction::Linear)
}

impl LiveContinuation for GlowPulseScene {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 => live.wait_segment(0.5),
            1 => {
                let target = live.target_editor(&self.persistent).map_err(message)?;
                live.set_glow(&target, EffectUpdate::default().intensity(1.2))
                    .map_err(message)?;
                live.declare_and_activate_transform_to(&self.persistent, &target, linear(0.5))
            }
            2 => {
                let target = live.target_editor(&self.persistent).map_err(message)?;
                live.shift(&target, 0.0, 0.4).map_err(message)?;
                live.set_glow(&target, EffectUpdate::default().radius(Pixels(12.0)))
                    .map_err(message)?;
                live.declare_and_activate_animation_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &[
                        Request::GlowPulse {
                            target: &self.persistent,
                            intensity: 2.0,
                            options: AnimationOptions::new().run_time(0.6),
                        },
                        Request::GlowPulse {
                            target: &self.temporary,
                            intensity: 2.0,
                            options: AnimationOptions::new().run_time(0.6),
                        },
                        Request::TransformTo(
                            TransformToRequest::new(&self.persistent, &target, linear(0.6))
                                .method_target(),
                        ),
                    ],
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    AnimationOptions::new(),
                )
            }
            // The left restores its activation-time 1.2, keeping motion/radius.
            // The right's temporary binding retires at this play's completion.
            3 => live.wait_segment(0.4),
            4 => live.declare_and_activate_glow_pulse(&self.temporary, 1.5, linear(0.5)),
            5 | 7 => live.wait_segment(0.25),
            6 => live.declare_and_activate_glow_pulse(
                &self.temporary,
                1.5,
                linear(0.5).reverse_rate_function(true),
            ),
            8 => return Ok(ContinuationStep::Finished),
            _ => return Err("glow pulse continuation resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<GlowPulseScene>, String> {
    let mut scene = Scene::new();
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background
        .set_fill(8.0 / 255.0, 16.0 / 255.0, 31.0 / 255.0, 1.0)
        .map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;

    let mut persistent = white_dot(&mut scene, -2.0)?;
    let mut temporary = white_dot(&mut scene, 2.0)?;
    persistent
        .set_glow(
            EffectUpdate::default()
                .color(Color::BLUE)
                .radius(Pixels(10.0))
                .intensity(0.35),
        )
        .map_err(message)?;
    // A neutral authored treatment must survive temporary glow enrollment and
    // cleanup. It preserves the ordinary pixels while exercising a nonempty stack.
    temporary
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(0.0))).map_err(message)?,
            "soften",
        )
        .map_err(message)?;
    scene.add(&persistent).map_err(message)?;
    scene.add(&temporary).map_err(message)?;
    scene
        .into_live_program(GlowPulseScene {
            persistent,
            temporary,
            stage: 0,
        })
        .map_err(message)
}

fn white_dot(scene: &mut Scene, x: f64) -> Result<Mobject, String> {
    let mut dot = scene.circle(0.4).map_err(message)?;
    dot.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
    dot.disable_stroke().map_err(message)?;
    dot.set_translation(x, 0.0).map_err(message)?;
    Ok(dot)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn paired_program_preserves_disjoint_motion_radius_and_restores_both_pulse_modes() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        for time in [
            0.5, 0.75, 1.0, 1.3, 1.6, 2.0, 2.125, 2.375, 2.5, 2.75, 2.875, 3.125, 3.25, 3.5,
        ] {
            let status = program.drive_to(&mut callbacks, time).unwrap();
            let frame = program.session().frame();
            assert_eq!(frame.objects.len(), 3);
            let left =
                crate::effect_authoring::only_effect(frame.objects[1].effects.as_ref().unwrap())
                    .definition;
            let right = frame.objects[2].effects.as_ref().unwrap();
            assert_eq!(
                right.definitions()[0].as_blur().unwrap().radius(),
                Pixels(0.0).into()
            );
            assert!(right.len() == 1 || right.len() == 2);
            let right_glow = right
                .definitions()
                .get(1)
                .map(|value| value.as_glow().unwrap());
            if time == 1.3 {
                assert!((left.as_glow().unwrap().intensity() - 2.0).abs() < 1e-12);
                assert_eq!(left.radius(), Some(noon_core::EffectRadius::Pixels(11.0)));
                assert!((frame.render_transform(1).translation.y - 0.2).abs() < 1e-6);
                assert!((right_glow.unwrap().intensity() - 2.0).abs() < 1e-12);
            }
            if time >= 1.6 {
                assert!((left.as_glow().unwrap().intensity() - 1.2).abs() < 1e-12);
                assert_eq!(left.radius(), Some(noon_core::EffectRadius::Pixels(12.0)));
                assert!((frame.render_transform(1).translation.y - 0.4).abs() < 1e-6);
            }
            if [1.6, 2.5, 3.25, 3.5].contains(&time) {
                assert_eq!(
                    right.len(),
                    1,
                    "cleanup must retire only the temporary glow"
                );
                assert!(right_glow.is_none());
            }
            let expected = if [2.125, 3.125].contains(&time) {
                Some(0.375)
            } else if [2.375, 2.875].contains(&time) {
                Some(1.125)
            } else {
                None
            };
            if let Some(expected) = expected {
                assert_eq!(right_glow.unwrap().intensity(), expected);
            }
            match status {
                LiveProgramStatus::PublicationPending(expected) => {
                    let context = program.take_renderer_publication().context();
                    assert_eq!(context, expected);
                    program.admit_publication(context).unwrap();
                    assert_eq!(
                        program.resume().unwrap() == LiveProgramStatus::Finished,
                        time == 3.5
                    );
                }
                LiveProgramStatus::ReadyToResume => {
                    assert_eq!(
                        program.resume().unwrap() == LiveProgramStatus::Finished,
                        time == 3.5
                    );
                }
                LiveProgramStatus::Awaiting(_) => {}
                other => panic!("unexpected program state at {time}: {other:?}"),
            }
        }
        assert_eq!(program.status(), LiveProgramStatus::Finished);
        assert_eq!(program.session().frame().time, 3.5);
    }
}
