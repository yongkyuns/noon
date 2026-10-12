//! Shared host continuation for static paired scenes with a bounded quiet wait.
use crate::{ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Scene};

pub struct Hold {
    duration: f64,
    started: bool,
}
impl LiveContinuation for Hold {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.started {
            return Ok(ContinuationStep::Finished);
        }
        self.started = true;
        Ok(ContinuationStep::Await(
            live.wait_segment(self.duration)
                .map_err(|e| e.to_string())?,
        ))
    }
}

pub fn program(scene: Scene, duration: f64) -> Result<LiveProgram<Hold>, String> {
    scene
        .into_live_program(Hold {
            duration,
            started: false,
        })
        .map_err(|e| e.to_string())
}
