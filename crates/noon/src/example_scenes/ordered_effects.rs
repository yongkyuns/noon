//! Ordinary ordered appearance, paired with web/python/examples/ordered_effects.py.
use crate::{
    effects::{Blur, EffectOffset, EffectUpdate, Pixels, Shadow},
    AnimationCompositionRequest, AnimationOptions, Color, ContinuationStep, LiveContinuation,
    LiveProgram, LiveSession, Mobject, RateFunction, Scene, SemanticAnimationCompositionKind,
    TransformToRequest,
};

pub struct OrderedEffects {
    reference: Mobject,
    source: Mobject,
    stage: u8,
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl LiveContinuation for OrderedEffects {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let segment = match self.stage {
            0 | 2 | 4 => live.wait_segment(0.5),
            1 => {
                let first = live.target_editor(&self.source).map_err(message)?;
                let finish = live.target_editor(&self.source).map_err(message)?;
                let reference = live.target_editor(&self.reference).map_err(message)?;
                live.shift(&first, 0.0, 0.4).map_err(message)?;
                live.shift(&reference, 0.0, 0.4).map_err(message)?;
                live.set_effect(
                    &first,
                    "soften",
                    EffectUpdate::default().radius(Pixels(4.0)),
                )
                .map_err(message)?;
                live.set_effect(
                    &finish,
                    "finish",
                    EffectUpdate::default().radius(Pixels(3.0)),
                )
                .map_err(message)?;
                live.declare_and_activate_animation_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &[
                        AnimationCompositionRequest::TransformTo(
                            TransformToRequest::new(&self.source, &first, options).method_target(),
                        ),
                        AnimationCompositionRequest::TransformTo(
                            TransformToRequest::new(&self.source, &finish, options).method_target(),
                        ),
                        AnimationCompositionRequest::TransformTo(
                            TransformToRequest::new(&self.reference, &reference, options)
                                .method_target(),
                        ),
                    ],
                    options,
                    AnimationOptions::new(),
                )
            }
            3 => {
                let target = live.target_editor(&self.source).map_err(message)?;
                let reference = live.target_editor(&self.reference).map_err(message)?;
                live.shift(&target, 0.0, -0.4).map_err(message)?;
                live.shift(&reference, 0.0, -0.4).map_err(message)?;
                for name in ["soften", "finish"] {
                    live.set_effect(&target, name, EffectUpdate::default().radius(Pixels(0.0)))
                        .map_err(message)?;
                }
                live.set_effect(&target, "glow", EffectUpdate::default().intensity(0.0))
                    .map_err(message)?;
                live.set_effect(
                    &target,
                    "depth",
                    EffectUpdate::default().color(Color::rgba(0.0, 0.0, 1.0, 0.0)),
                )
                .map_err(message)?;
                live.declare_and_activate_animation_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &[
                        AnimationCompositionRequest::TransformTo(
                            TransformToRequest::new(&self.source, &target, options).method_target(),
                        ),
                        AnimationCompositionRequest::TransformTo(
                            TransformToRequest::new(&self.reference, &reference, options)
                                .method_target(),
                        ),
                    ],
                    options,
                    AnimationOptions::new(),
                )
            }
            5 => {
                for name in ["soften", "glow", "depth", "finish"] {
                    live.remove_effect(&self.source, name).map_err(message)?;
                }
                live.wait_segment(0.5)
            }
            6 => return Ok(ContinuationStep::Finished),
            _ => return Err("ordered appearance resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<OrderedEffects>, String> {
    let mut scene = Scene::new();
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background.set_fill(0.0, 0.0, 0.0, 1.0).map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;
    let mut reference = scene.rectangle(1.2, 0.8).map_err(message)?;
    reference.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
    reference.disable_stroke().map_err(message)?;
    reference.set_translation(-2.0, 0.0).map_err(message)?;
    scene.add(&reference).map_err(message)?;
    let mut source = scene.rectangle(1.2, 0.8).map_err(message)?;
    source.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
    source.disable_stroke().map_err(message)?;
    source.set_translation(2.0, 0.0).map_err(message)?;
    source.set_object_opacity(0.5).map_err(message)?;
    source
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).map_err(message)?,
            "soften",
        )
        .map_err(message)?;
    source
        .set_glow(
            EffectUpdate::default()
                .radius(Pixels(3.25))
                .intensity(1.25)
                .color(Color::rgba(1.0, 0.0, 0.0, 1.0)),
        )
        .map_err(message)?;
    source
        .add_effect(
            Shadow::new(
                EffectUpdate::default()
                    .radius(Pixels(2.0))
                    .offset(EffectOffset::Pixels([8.0, -4.0]))
                    .color(Color::rgba(0.0, 0.0, 1.0, 0.7)),
            )
            .map_err(message)?,
            "depth",
        )
        .map_err(message)?;
    source
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(1.0))).map_err(message)?,
            "finish",
        )
        .map_err(message)?;
    scene.add(&source).map_err(message)?;
    scene
        .into_live_program(OrderedEffects {
            reference,
            source,
            stage: 0,
        })
        .map_err(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn paired_ordered_program_drives_two_blurs_and_retires_all_bindings() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        program.resume().unwrap();
        for (time, first, last, neutral) in [
            (0.5, 2.0, 1.0, false),
            (1.0, 3.0, 2.0, false),
            (1.5, 4.0, 3.0, false),
            (2.0, 4.0, 3.0, false),
            (2.5, 2.0, 1.5, false),
            (3.0, 0.0, 0.0, true),
            (3.25, 0.0, 0.0, true),
        ] {
            let status = program.drive_to(&mut callbacks, time).unwrap();
            if let LiveProgramStatus::PublicationPending(expected) = status {
                let context = program.take_renderer_publication().context();
                assert_eq!(context, expected);
                program.admit_publication(context).unwrap();
                program.resume().unwrap();
            } else if status == LiveProgramStatus::ReadyToResume {
                program.resume().unwrap();
            }
            let frame = program.session().frame();
            assert_eq!(frame.objects.len(), 3);
            let stack = frame.objects[2].effects.as_ref().unwrap();
            assert_eq!(stack.len(), 4);
            assert_eq!(stack.definitions()[0].radius(), Some(Pixels(first).into()));
            assert_eq!(stack.definitions()[3].radius(), Some(Pixels(last).into()));
            assert_eq!(
                stack.definitions().iter().all(|effect| effect.is_neutral()),
                neutral
            );
            assert_eq!(
                frame.objects[1].transform.translation.y,
                frame.objects[2].transform.translation.y
            );
            assert_eq!(frame.objects[2].style.opacity, 0.5);
        }
        let status = program.drive_to(&mut callbacks, 3.5).unwrap();
        if let LiveProgramStatus::PublicationPending(context) = status {
            assert_eq!(program.take_renderer_publication().context(), context);
            program.admit_publication(context).unwrap();
            program.resume().unwrap();
        } else if status == LiveProgramStatus::ReadyToResume {
            program.resume().unwrap();
        }
        assert!(program.session().frame().objects[2].effects.is_none());
        let status = program.drive_to(&mut callbacks, 4.0).unwrap();
        let finished = if let LiveProgramStatus::PublicationPending(context) = status {
            assert_eq!(program.take_renderer_publication().context(), context);
            program.admit_publication(context).unwrap();
            program.resume().unwrap()
        } else if status == LiveProgramStatus::ReadyToResume {
            program.resume().unwrap()
        } else {
            status
        };
        assert_eq!(finished, LiveProgramStatus::Finished);
    }
}
