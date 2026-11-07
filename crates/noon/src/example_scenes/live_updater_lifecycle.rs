//! Sequential counterpart of the exact Python RotationUpdater gallery source.
use crate::{Color, LiveProgram, Scene, SceneScript, ScriptError, Vec2};

pub fn program() -> Result<LiveProgram<SceneScript>, ScriptError> {
    Scene::script(|scene| async move {
        let reference = scene.line(Vec2::ZERO, Vec2::new(-1.0, 0.0))?;
        scene.set_color(&reference, Color::WHITE)?;
        let moving = scene.line(Vec2::ZERO, Vec2::new(-1.0, 0.0))?;
        scene.set_color(&moving, Color::YELLOW)?;

        let forth = scene.add_updater(&moving, |target, frame| {
            target.rotate_about_origin(frame.dt())
        })?;
        scene.add(&reference)?;
        scene.add(&moving)?;
        scene.wait(2.0).await?;
        scene.remove_updater(&forth)?;

        let back = scene.add_updater(&moving, |target, frame| {
            target.rotate_about_origin(-frame.dt())
        })?;
        scene.wait(2.0).await?;
        scene.remove_updater(&back)?;
        scene.wait(0.5).await?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn live_rotation_updater_reverses_then_stops_without_replaying_source() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        let identity = program.session().runtime_identity();
        let mut object_order = None;
        for (start, middle, end, middle_angle, end_angle) in [
            (0.0, 1.0, 2.0, 1.0, 2.0),
            (2.0, 3.0, 4.0, 1.0, 0.0),
            (4.0, 4.25, 4.5, 0.0, 0.0),
        ] {
            let LiveProgramStatus::Awaiting(segment) = program.resume().unwrap() else {
                panic!("updater source must await each authored wait");
            };
            assert!(!segment.is_complete());
            assert_eq!(program.session().frame().time, start);
            program.drive_to(&mut callbacks, middle).unwrap();
            let frame = program.session().frame();
            assert_eq!(frame.time, middle);
            assert_eq!(frame.objects.len(), 2);
            let current_order = [frame.objects[0].id, frame.objects[1].id];
            assert_eq!(*object_order.get_or_insert(current_order), current_order);
            assert_eq!(frame.objects[0].style.stroke, Some(Color::WHITE));
            assert_eq!(frame.objects[1].style.stroke, Some(Color::YELLOW));
            assert!(frame.objects[0].transform.rotation.abs() < 1e-5);
            assert!((frame.objects[1].transform.rotation - middle_angle).abs() < 1e-5);
            if let LiveProgramStatus::PublicationPending(expected) =
                program.drive_to(&mut callbacks, end).unwrap()
            {
                let publication = program.take_renderer_publication().context();
                assert_eq!(expected, publication);
                program.admit_publication(publication).unwrap();
            }
            assert_eq!(program.session().frame().time, end);
            assert!(
                (program.session().frame().objects[1].transform.rotation - end_angle).abs() < 1e-5
            );
            assert_eq!(program.session().runtime_identity(), identity);
            assert!(callbacks.is_empty());
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        let frame = program.session().frame();
        assert_eq!(frame.time, 4.5);
        assert!(frame.objects[1].transform.rotation.abs() < 1e-5);
        assert_eq!(frame.objects.len(), 2);
    }
}
