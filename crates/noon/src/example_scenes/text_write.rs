//! Text movement and glyph-by-glyph Write in one shared execution composition.

use crate::{
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
    LiveSession, Mobject, RateFunction, Scene, SemanticAnimationCompositionKind,
    TransformToRequest,
};

pub struct TextWrite {
    moving: Mobject,
    target: Mobject,
    writing: Mobject,
    stage: u8,
}

impl LiveContinuation for TextWrite {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                let options = AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear);
                let segment = live
                    .declare_and_activate_animation_composition(
                        SemanticAnimationCompositionKind::Parallel,
                        &[
                            AnimationCompositionRequest::TransformTo(
                                TransformToRequest::point_correspondence(
                                    &self.moving,
                                    &self.target,
                                    options,
                                ),
                            ),
                            AnimationCompositionRequest::TextWrite {
                                target: &self.writing,
                                reverse_member_order: false,
                                options,
                            },
                        ],
                        options,
                        AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                self.stage = 1;
                Ok(ContinuationStep::Await(segment))
            }
            1 => {
                let moved = live
                    .effective(&self.moving)
                    .map_err(|error| error.to_string())?;
                if moved.transform.translation.x.abs() > 1e-6 {
                    return Err(
                        "Text Write composition did not complete its sibling transform".into(),
                    );
                }
                self.stage = 2;
                live.declare_and_activate_text_write(
                    &self.writing,
                    true,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear)
                        .introducer(false)
                        .remover(true)
                        .reverse_rate_function(true),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                self.stage = 3;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            3 => {
                self.stage = 4;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("Text Write continuation resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<TextWrite>, String> {
    let scene = Scene::new();
    let mut moving = scene.text("MOVE").map_err(|error| error.to_string())?;
    moving.set_translation(-2.0, -1.0)?;
    let mut writing = scene.text("WRITE").map_err(|error| error.to_string())?;
    writing.set_translation(-1.0, 1.0)?;
    let mut target = moving.target_editor()?;
    target.shift(2.0, 0.0)?;
    scene
        .into_live_program(TextWrite {
            moving,
            target,
            writing,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    fn complete(
        program: &mut LiveProgram<TextWrite>,
        callbacks: &mut RustHostCallbackTable,
        time: f64,
    ) {
        match program.drive_to(callbacks, time).unwrap() {
            LiveProgramStatus::PublicationPending(expected) => {
                let context = program.take_renderer_publication().context();
                assert_eq!(context, expected);
                program.admit_publication(context).unwrap();
            }
            LiveProgramStatus::ReadyToResume => {}
            status => panic!("expected completion at {time}, got {status:?}"),
        }
    }

    #[test]
    fn native_text_write_and_unwrite_share_the_ordinary_execution_session() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        program.drive_to(&mut callbacks, 0.5).unwrap();
        let frame = program.session().planned_family_frame();
        assert_eq!(frame.retained.objects.len(), 2);
        let states: Vec<_> = frame.family_animations.iter().flatten().collect();
        assert_eq!(
            states.len(),
            1,
            "only the writing Text owns glyph animation state"
        );
        assert!((states[0].overall_progress - 0.25).abs() < 1e-6);
        assert_eq!(program.session().family_animation_plans().len(), 1);
        complete(&mut program, &mut callbacks, 2.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        program.drive_to(&mut callbacks, 2.5).unwrap();
        let frame = program.session().planned_family_frame();
        let state = frame.family_animations.iter().flatten().next().unwrap();
        assert!(state.reverse_member_order && state.reverse_rate_function);
        assert!((state.overall_progress - 0.5).abs() < 1e-6);
        complete(&mut program, &mut callbacks, 3.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        complete(&mut program, &mut callbacks, 3.25);
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        let visible = program.query_viewport(crate::Rect::new(
            crate::Vec2::new(-4.0, -3.0),
            crate::Vec2::new(4.0, 3.0),
        ));
        assert_eq!(visible.object_indices().len(), 1);
        assert!(program
            .session()
            .planned_family_frame()
            .family_animations
            .iter()
            .all(Option::is_none));
    }
}
