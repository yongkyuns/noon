//! Sequential counterpart of the exact Python RotationUpdater gallery source.
use crate::{
    ContinuationStep, HostCallbackId, LiveContinuation, LiveProgram, LiveSession, Mobject,
    RustHostCallbackTable, Scene, SemanticMutationTransaction, Vec2,
};

const FORTH: HostCallbackId = HostCallbackId::new(1);
const BACK: HostCallbackId = HostCallbackId::new(2);

pub struct RotationUpdater {
    moving: Mobject,
    stage: u8,
}

impl LiveContinuation for RotationUpdater {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {}
            1 => {
                let mut tx = SemanticMutationTransaction::new();
                tx.remove_updater(self.moving.node_id(), FORTH, 2.0);
                tx.add_updater(self.moving.node_id(), BACK, 2.0, None);
                live.apply(tx).map_err(|e| e.to_string())?;
            }
            2 => {
                let mut tx = SemanticMutationTransaction::new();
                tx.remove_updater(self.moving.node_id(), BACK, 4.0);
                live.apply(tx).map_err(|e| e.to_string())?;
            }
            3 => return Ok(ContinuationStep::Finished),
            _ => unreachable!(),
        }
        let duration = if self.stage == 2 { 0.5 } else { 2.0 };
        self.stage += 1;
        live.wait_segment(duration)
            .map(ContinuationStep::Await)
            .map_err(|e| e.to_string())
    }
}

pub fn program() -> Result<(LiveProgram<RotationUpdater>, RustHostCallbackTable), String> {
    let mut scene = Scene::new();
    let mut reference = scene.line((0.0, 0.0), (-1.0, 0.0))?;
    reference.set_color(1.0, 1.0, 1.0, 1.0)?;
    let mut moving = scene.line((0.0, 0.0), (-1.0, 0.0))?;
    moving.set_color(1.0, 1.0, 0.0, 1.0)?;
    scene.add(&reference).map_err(|error| error.to_string())?;
    scene.add(&moving).map_err(|error| error.to_string())?;
    let mut callbacks = RustHostCallbackTable::new();
    for (id, sign) in [(FORTH, 1.0), (BACK, -1.0)] {
        callbacks
            .insert(id, move |context| {
                let transform = context
                    .target_transform_rotated_about_point(sign * context.delta_time(), Vec2::ZERO)
                    .map_err(std::io::Error::other)?;
                context
                    .set_target_transform(transform)
                    .map_err(std::io::Error::other)
            })
            .map_err(|e| e.to_string())?;
    }
    callbacks
        .add_updater(
            &mut scene.store().borrow_mut(),
            moving.node_id(),
            FORTH,
            0.0,
            None,
        )
        .map_err(|e| e.to_string())?;
    Ok((
        scene
            .into_live_program(RotationUpdater { moving, stage: 0 })
            .map_err(|e| e.to_string())?,
        callbacks,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiveProgramStatus;

    #[test]
    fn live_rotation_updater_reverses_then_stops_without_replaying_source() {
        let (mut program, mut callbacks) = program().unwrap();
        let identity = program.session().runtime_identity();
        for end in [2.0, 4.0, 4.5] {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            let middle = if end == 2.0 {
                1.0
            } else if end == 4.0 {
                3.0
            } else {
                4.25
            };
            program.drive_to(&mut callbacks, middle).unwrap();
            let angle = program.session().frame().objects[1].transform.rotation;
            assert!((angle - if end == 4.5 { 0.0 } else { 1.0 }).abs() < 1e-5);
            if let LiveProgramStatus::PublicationPending(expected) =
                program.drive_to(&mut callbacks, end).unwrap()
            {
                let publication = program.take_renderer_publication().context();
                assert_eq!(expected, publication);
                program.admit_publication(publication).unwrap();
            }
            assert_eq!(program.session().runtime_identity(), identity);
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        let frame = program.session().frame();
        assert_eq!(frame.time, 4.5);
        assert!((frame.objects[1].transform.rotation).abs() < 1e-5);
        assert_eq!(frame.objects.len(), 2);
    }
}
