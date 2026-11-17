//! Shared foreground persistence and family restructuring through ordinary membership edits.
use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, Mobject, MobjectFamily,
    MobjectTarget::{Family, Object as Leaf},
    RateFunction, Scene, SemanticNodeId,
};
pub struct ForegroundMembership {
    red: Mobject,
    blue: Mobject,
    green: Mobject,
    later: Mobject,
    family: MobjectFamily,
    stage: u8,
}

impl ForegroundMembership {
    fn assert_lists(
        &self,
        scene: &Scene,
        display: &[SemanticNodeId],
        foreground: &[SemanticNodeId],
    ) -> Result<(), String> {
        let members = scene.root_member_ids().map_err(|error| error.to_string())?;
        let foreground_members = scene
            .foreground_member_ids()
            .map_err(|error| error.to_string())?;
        if members != display || foreground_members != foreground {
            return Err(format!(
                "foreground stage {}: display {members:?} foreground {foreground_members:?}, expected {display:?} / {foreground:?}",
                self.stage
            ));
        }
        Ok(())
    }
}

impl LiveContinuation for ForegroundMembership {
    type Error = String;

    fn resume(&mut self, scene: &mut crate::ContinuationScene) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                scene
                    .add_foreground_many(&[Family(&self.family)])
                    .map_err(|error| error.to_string())?;
                self.assert_lists(scene, &[self.family.node_id()], &[self.family.node_id()])?;
            }
            1 => {
                let segment = scene
                    .live()
                    .declare_and_activate_fade(
                        &self.green,
                        noon_core::SemanticFadeDirection::In,
                        AnimationOptions::new()
                            .run_time(0.35)
                            .rate_func(RateFunction::Linear),
                    )
                    .map_err(|error| error.to_string())?;
                self.assert_lists(
                    scene,
                    &[self.green.node_id(), self.family.node_id()],
                    &[self.family.node_id()],
                )?;
                self.stage += 1;
                return Ok(ContinuationStep::Await(segment));
            }
            2 => {
                scene
                    .remove_foreground_many(&[Leaf(&self.red)])
                    .map_err(|error| error.to_string())?;
                self.assert_lists(
                    scene,
                    &[self.green.node_id(), self.family.node_id()],
                    &[self.blue.node_id()],
                )?;
            }
            3 => {
                let segment = scene
                    .live()
                    .declare_and_activate_create(
                        &self.later,
                        AnimationOptions::new()
                            .run_time(0.35)
                            .rate_func(RateFunction::Linear),
                    )
                    .map_err(|error| error.to_string())?;
                self.assert_lists(
                    scene,
                    &[
                        self.green.node_id(),
                        self.red.node_id(),
                        self.later.node_id(),
                        self.blue.node_id(),
                    ],
                    &[self.blue.node_id()],
                )?;
                self.stage += 1;
                return Ok(ContinuationStep::Await(segment));
            }
            4 => {
                scene
                    .remove_foreground_many(&[Leaf(&self.blue)])
                    .map_err(|error| error.to_string())?;
                self.assert_lists(
                    scene,
                    &[
                        self.green.node_id(),
                        self.red.node_id(),
                        self.later.node_id(),
                        self.blue.node_id(),
                    ],
                    &[],
                )?;
            }
            5 => return Ok(ContinuationStep::Finished),
            _ => return Err("foreground continuation resumed after completion".into()),
        }
        self.stage += 1;
        scene
            .live()
            .wait_segment(0.35)
            .map(ContinuationStep::Await)
            .map_err(|error| error.to_string())
    }
}

pub fn program() -> Result<LiveProgram<ForegroundMembership>, String> {
    let mut scene = Scene::new();
    let mut red = scene.square(1.8).map_err(|error| error.to_string())?;
    red.set_fill(1.0, 0.1, 0.1, 1.0)
        .map_err(|error| error.to_string())?;
    red.set_translation(-0.7, 0.0)
        .map_err(|error| error.to_string())?;
    let mut blue = scene.square(1.8).map_err(|error| error.to_string())?;
    blue.set_fill(0.1, 0.2, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    blue.set_translation(0.7, 0.0)
        .map_err(|error| error.to_string())?;
    let mut green = scene.square(2.1).map_err(|error| error.to_string())?;
    green
        .set_fill(0.1, 0.8, 0.2, 1.0)
        .map_err(|error| error.to_string())?;
    let mut later = scene.circle(0.8).map_err(|error| error.to_string())?;
    later
        .set_fill(1.0, 0.8, 0.1, 1.0)
        .map_err(|error| error.to_string())?;
    let family = scene
        .family(&[(&red).into(), (&blue).into()])
        .map_err(|error| error.to_string())?;
    let continuation = ForegroundMembership {
        red,
        blue,
        green,
        later,
        family,
        stage: 0,
    };
    scene
        .into_live_program(continuation)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn foreground_example_runs_through_shared_live_membership() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for stage in 0..5 {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            match program
                .drive_to(&mut callbacks, f64::from(stage + 1) * 0.35)
                .unwrap()
            {
                LiveProgramStatus::PublicationPending(expected) => {
                    let context = program.take_renderer_publication().context();
                    assert_eq!(context, expected);
                    program.admit_publication(context).unwrap();
                }
                LiveProgramStatus::ReadyToResume => {}
                status => panic!("foreground completion: {status:?}"),
            }
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
