//! Own points, drawing intent and children stay on stable family roots.
use crate::{
    Color, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, LiveSessionError,
    MobjectFamily, Scene,
};

pub struct FamilyRootPoints {
    group: MobjectFamily,
    vector: MobjectFamily,
    left_points: Vec<(f64, f64)>,
    right_points: Vec<(f64, f64)>,
    stage: u8,
}

impl LiveContinuation for FamilyRootPoints {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.stage == 3 {
            return Ok(ContinuationStep::Finished);
        }
        let step = (|| -> Result<_, LiveSessionError> {
            match self.stage {
                0 => {}
                1 => {
                    live.clear_family_points(&self.group)?;
                    live.clear_family_points(&self.vector)?;
                }
                2 => {
                    live.set_family_points_f64(&self.group, &self.left_points)?;
                    live.set_family_points_f64(&self.vector, &self.right_points)?;
                }
                _ => unreachable!("three held phases"),
            }
            let expected = if self.stage == 1 { 0 } else { 16 };
            assert_eq!(self.group.get_num_points()?, expected);
            assert_eq!(self.vector.get_num_points()?, expected);
            assert_eq!(self.group.member_count()?, 1);
            assert_eq!(self.vector.member_count()?, 1);
            self.stage += 1;
            live.wait_segment(0.25).map(ContinuationStep::Await)
        })();
        step.map_err(|error| error.to_string())
    }
}

/// The same compiled Rust continuation feeds native capture and direct WASM.
pub fn program() -> Result<LiveProgram<FamilyRootPoints>, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut left = scene.circle(0.2)?;
        let mut right = scene.circle(0.2)?;
        left.shift(-2.0, 0.0)?;
        right.shift(2.0, 0.0)?;
        for child in [&mut left, &mut right] {
            child.set_fill(1.0, 1.0, 0.0, 1.0)?;
            child.set_stroke_width(0.0)?;
        }
        let mut left_square = scene.square(1.2)?;
        let mut right_square = scene.square(1.2)?;
        left_square.shift(-2.0, 0.0)?;
        right_square.shift(2.0, 0.0)?;
        let left_points = left_square.path_query()?.points()?;
        let right_points = right_square.path_query()?.points()?;
        let group = scene.family_with_presentation(&[(&left).into()], 0.0, Color::WHITE, false)?;
        let vector = scene.family_with_presentation(&[(&right).into()], 0.0, Color::WHITE, true)?;
        group.set_points_f64(&left_points)?;
        vector.set_points_f64(&right_points)?;
        let mut vector_root = vector
            .own_object()?
            .ok_or_else(|| std::io::Error::other("materialized family has no own payload"))?;
        vector_root.set_fill(0.0, 0.0, 1.0, 1.0)?;
        vector_root.set_stroke_width(0.0)?;
        scene.add_many(&[(&group).into(), (&vector).into()])?;
        Ok(scene.into_live_program(FamilyRootPoints {
            group,
            vector,
            left_points,
            right_points,
            stage: 0,
        })?)
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn shared_root_points_continue_through_held_clear_and_restore_phases() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for end in [0.25, 0.5, 0.75] {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            match program.drive_to(&mut callbacks, end).unwrap() {
                LiveProgramStatus::PublicationPending(expected) => {
                    let context = program.take_renderer_publication().context();
                    assert_eq!(context, expected);
                    program.admit_publication(context).unwrap();
                }
                LiveProgramStatus::ReadyToResume => {}
                status => panic!("root-point completion: {status:?}"),
            }
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }

    #[test]
    fn public_live_family_points_reject_foreign_identity_without_publication() {
        let mut scene = Scene::new();
        let local = scene.family(&[]).unwrap();
        scene.add_many(&[(&local).into()]).unwrap();
        let foreign = Scene::new().family(&[]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let context = session.publication_context();
        let revision = scene.revision();
        let mut live = scene.live(&mut session);
        assert!(live.set_family_points_f64(&foreign, &[(0.0, 0.0)]).is_err());
        assert!(live.clear_family_points(&foreign).is_err());
        assert_eq!(session.publication_context(), context);
        assert_eq!(scene.revision(), revision);
        assert!(local.own_object().unwrap().is_none());
    }
}
