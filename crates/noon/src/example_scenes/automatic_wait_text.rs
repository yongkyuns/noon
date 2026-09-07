//! Empty initial wait followed by live Text construction and a fade lifecycle.

use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
    RateFunction, Scene, SemanticFadeDirection, Text,
};

pub struct AutomaticWaitText {
    label: Option<Mobject>,
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
                let label = live
                    .create_text(Text::new("LATE").with_font_size(64.0))
                    .map_err(|error| error.to_string())?;
                if live.contains(&label).map_err(|error| error.to_string())? {
                    return Err("new live Text must remain detached before FadeIn".into());
                }
                let segment = live
                    .declare_and_activate_fade(
                        &label,
                        SemanticFadeDirection::In,
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    )
                    .map_err(|error| error.to_string())?;
                self.label = Some(label);
                self.stage = 2;
                Ok(ContinuationStep::Await(segment))
            }
            2 => {
                let label = self
                    .label
                    .as_ref()
                    .ok_or("late Text identity disappeared")?;
                if !live.contains(label).map_err(|error| error.to_string())? {
                    return Err("FadeIn did not admit the late Text scene root".into());
                }
                self.stage = 3;
                live.declare_and_activate_fade(
                    label,
                    SemanticFadeDirection::Out,
                    AnimationOptions::new()
                        .run_time(0.5)
                        .rate_func(RateFunction::Linear),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            3 => {
                let label = self
                    .label
                    .as_ref()
                    .ok_or("late Text identity disappeared")?;
                if live.contains(label).map_err(|error| error.to_string())? {
                    return Err("FadeOut did not remove the late Text scene root".into());
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
            _ => Err("automatic-wait Text continuation resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<AutomaticWaitText>, String> {
    Scene::new()
        .into_live_program(AutomaticWaitText {
            label: None,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}
