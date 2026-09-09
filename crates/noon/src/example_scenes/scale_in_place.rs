//! Play-begin target capture for the paired Python ScaleInPlace example.
use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
    RateFunction, Scene,
};

pub struct ScaleInPlace {
    square: Mobject,
    stage: u8,
}

impl LiveContinuation for ScaleInPlace {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.stage == 2 {
            let state = live.effective(&self.square).map_err(|e| e.to_string())?;
            if state.transform.translation != noon_core::Vec2::new(1.0, 0.0)
                || state.transform.scale != noon_core::Vec2::new(2.0, 2.0)
            {
                return Err("scaled target did not capture the completed movement".into());
            }
            self.stage = 3;
            return Ok(ContinuationStep::Finished);
        }
        if self.stage > 2 {
            return Err("scale continuation resumed after completion".into());
        }
        let target = live
            .target_editor(&self.square)
            .map_err(|e| e.to_string())?;
        let duration = if self.stage == 0 {
            live.set_translation(&target, 1.0, 0.0)
                .map_err(|e| e.to_string())?;
            0.25
        } else {
            live.scale(&target, 2.0, 2.0).map_err(|e| e.to_string())?;
            0.75
        };
        self.stage += 1;
        live.declare_and_activate_transform_to(
            &self.square,
            &target,
            AnimationOptions::new()
                .run_time(duration)
                .rate_func(RateFunction::Linear),
        )
        .map(ContinuationStep::Await)
        .map_err(|e| e.to_string())
    }
}

pub fn program() -> Result<LiveProgram<ScaleInPlace>, String> {
    let mut scene = Scene::new();
    let mut square = scene.square(1.0).map_err(|error| error.to_string())?;
    square
        .set_translation(-0.5, 0.25)
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
        .into_live_program(ScaleInPlace { square, stage: 0 })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn scale_target_captures_the_previous_segment_endpoint() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for end in [0.25, 1.0] {
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
