//! Paired constructor-default evidence on the ordinary typed engine path.
use crate::{
    ContinuationStep, LiveContinuation, LiveProgram, LiveSession, ManimNumberLineOptions, Scene,
};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    for (range, y, include_tip) in [
        (None, 2.0, false),
        (Some(&[-2.0, 2.0][..]), 0.0, true),
        (Some(&[-2.0, 2.0, 0.5][..]), -2.0, false),
    ] {
        let mut options = ManimNumberLineOptions::from_range(range)?;
        options.include_tip = include_tip;
        let line = scene.number_line(&options)?;
        let family = line.family().clone();
        family.shift(0.0, y)?;
        scene.add_many(&[(&family).into()])?;
    }
    Ok(scene)
}

pub struct Hold(bool);

impl LiveContinuation for Hold {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.0 {
            return Ok(ContinuationStep::Finished);
        }
        self.0 = true;
        Ok(ContinuationStep::Await(
            live.wait_segment(0.2).map_err(|e| e.to_string())?,
        ))
    }
}

pub fn program() -> Result<LiveProgram<Hold>, Box<dyn std::error::Error>> {
    Ok(scene()?.into_live_program(Hold(false))?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn ordinary_defaults_use_one_typed_scene_without_host_callbacks() {
        let scene = super::scene().unwrap();
        let session = scene.execution_session().unwrap();
        assert_eq!(session.frame().objects.len(), 32);
        assert!(!session.has_required_callbacks());
    }
}
