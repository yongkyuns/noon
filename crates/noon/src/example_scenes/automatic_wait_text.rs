//! Empty initial wait followed by live Text/Typst/MathTypst construction and fading.

use crate::{
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, FadeEndpoint,
    LiveContinuation, LiveProgram, LiveSession, MathTypst, Mobject, RateFunction, Scene,
    SemanticAnimationCompositionKind, SemanticFadeDirection, Text, Typst, Vec2,
};

pub struct AutomaticWaitText {
    late_objects: Option<[Mobject; 3]>,
    stage: u8,
}

impl LiveContinuation for AutomaticWaitText {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                self.stage = 1;
                live.wait_segment(0.5)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            1 => {
                let late_objects = [
                    live.create_text(
                        Text::new("LATE")
                            .with_font_size(56.0)
                            .move_to(Vec2::new(0.0, 2.0)),
                    )
                    .map_err(|error| error.to_string())?,
                    live.create_typst(
                        Typst::new("*Typst*")
                            .with_font_size(48.0)
                            .move_to(Vec2::new(0.0, 0.0)),
                    )
                    .map_err(|error| error.to_string())?,
                    live.create_math_typst(
                        MathTypst::new("x^2 + y^2")
                            .with_font_size(48.0)
                            .move_to(Vec2::new(0.0, -2.0)),
                    )
                    .map_err(|error| error.to_string())?,
                ];
                for object in &late_objects {
                    if live.contains(object).map_err(|error| error.to_string())? {
                        return Err(
                            "new live text objects must remain detached before FadeIn".into()
                        );
                    }
                }
                let segment =
                    activate_parallel_fade(live, &late_objects, SemanticFadeDirection::In, 1.0)?;
                self.late_objects = Some(late_objects);
                self.stage = 2;
                Ok(ContinuationStep::Await(segment))
            }
            2 => {
                let objects = self
                    .late_objects
                    .as_ref()
                    .ok_or("late text identities disappeared")?;
                for object in objects {
                    if !live.contains(object).map_err(|error| error.to_string())? {
                        return Err("FadeIn did not admit every late text scene root".into());
                    }
                }
                self.stage = 3;
                activate_parallel_fade(live, objects, SemanticFadeDirection::Out, 0.5)
                    .map(ContinuationStep::Await)
            }
            3 => {
                let objects = self
                    .late_objects
                    .as_ref()
                    .ok_or("late text identities disappeared")?;
                for object in objects {
                    if live.contains(object).map_err(|error| error.to_string())? {
                        return Err("FadeOut did not remove every late text scene root".into());
                    }
                }
                self.stage = 4;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            4 => {
                self.stage = 5;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("automatic-wait text continuation resumed after completion".into()),
        }
    }
}

fn activate_parallel_fade(
    live: &mut LiveSession<'_>,
    objects: &[Mobject; 3],
    direction: SemanticFadeDirection,
    run_time: f64,
) -> Result<crate::ExecutionSegment, String> {
    let options = AnimationOptions::new()
        .run_time(run_time)
        .rate_func(RateFunction::Linear);
    let requests = [
        AnimationCompositionRequest::Fade {
            target: &objects[0],
            direction,
            endpoint: FadeEndpoint::default(),
            options,
        },
        AnimationCompositionRequest::Fade {
            target: &objects[1],
            direction,
            endpoint: FadeEndpoint::default(),
            options,
        },
        AnimationCompositionRequest::Fade {
            target: &objects[2],
            direction,
            endpoint: FadeEndpoint::default(),
            options,
        },
    ];
    live.declare_and_activate_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        &requests,
        AnimationOptions::new().rate_func(RateFunction::Linear),
        AnimationOptions::new(),
    )
    .map_err(|error| error.to_string())
}

pub fn program() -> Result<LiveProgram<AutomaticWaitText>, String> {
    Scene::new()
        .into_live_program(AutomaticWaitText {
            late_objects: None,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}
