use std::{cell::RefCell, rc::Rc};

use noon::{
    LiveProgram, LiveProgramStatus, RustHostCallbackTable, Scene, SceneScript, ScriptError, Vec2,
};

fn finish_segment(program: &mut LiveProgram<SceneScript>, time: f64) {
    let mut callbacks = RustHostCallbackTable::new();
    if let LiveProgramStatus::PublicationPending(expected) =
        program.drive_to(&mut callbacks, time).unwrap()
    {
        let publication = program.take_renderer_publication().context();
        assert_eq!(publication, expected);
        program.admit_publication(publication).unwrap();
    }
}

#[test]
fn managed_updaters_read_ordered_effective_writes_without_exposing_callback_ids() {
    let mut program = Scene::script(|scene| async move {
        let circle = scene.circle(0.5)?;
        scene.shift(&circle, Vec2::new(1.0, 0.0))?;
        scene.add(&circle)?;
        scene.add_updater(&circle, |target, frame| {
            target.rotate_about_origin(std::f64::consts::FRAC_PI_2 * frame.dt())
        })?;
        scene.add_updater(&circle, |target, frame| {
            target.rotate_about_point(
                std::f64::consts::FRAC_PI_2 * frame.dt(),
                Vec2::new(1.0, 0.0),
            )
        })?;
        scene.wait(2.0).await?;
        Ok(())
    })
    .unwrap();
    program.resume().unwrap();
    program
        .drive_to(&mut RustHostCallbackTable::new(), 1.0)
        .unwrap();
    let transform = program.session().frame().objects[0].transform;
    assert!(transform.translation.x.abs() < 1e-5);
    assert!((transform.translation.y + 1.0).abs() < 1e-5);
    assert!((transform.rotation - std::f32::consts::PI).abs() < 1e-5);
}

#[test]
fn managed_updater_error_discards_the_whole_effective_phase() {
    let mut program = Scene::script(|scene| async move {
        let circle = scene.circle(0.5)?;
        scene.add(&circle)?;
        scene.add_updater(&circle, |target, frame| {
            target.rotate_about_origin(frame.dt())
        })?;
        scene.add_updater(&circle, |target, frame| {
            if frame.time() > 0.0 {
                target.set_fill_opacity(f64::NAN)?;
            }
            Ok(())
        })?;
        scene.wait(2.0).await?;
        Ok(())
    })
    .unwrap();
    program.resume().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    program.drive_to(&mut callbacks, 0.0).unwrap();
    let before = program.session().frame().objects[0].transform;
    let identity = program.session().runtime_identity();
    assert!(program.drive_to(&mut callbacks, 1.0).is_err());
    assert_eq!(program.status(), LiveProgramStatus::Terminal);
    assert_eq!(program.session().runtime_identity(), identity);
    assert_eq!(program.session().frame().objects[0].transform, before);
}

#[test]
fn foreign_registration_cannot_remove_a_local_updater_with_the_same_lookup_id() {
    let registration = Rc::new(RefCell::new(None));
    let foreign = Rc::clone(&registration);
    let first = Scene::script(move |scene| async move {
        let circle = scene.circle(0.5)?;
        scene.add(&circle)?;
        *foreign.borrow_mut() = Some(scene.add_updater(&circle, |target, frame| {
            target.rotate_about_origin(frame.dt())
        })?);
        scene.wait(2.0).await?;
        Ok(())
    })
    .unwrap();
    let mut second = Scene::script(move |scene| async move {
        let circle = scene.circle(0.5)?;
        scene.add(&circle)?;
        scene.add_updater(&circle, |target, frame| {
            target.rotate_about_origin(frame.dt())
        })?;
        scene.wait(1.0).await?;
        assert!(matches!(
            scene.remove_updater(registration.borrow().as_ref().unwrap()),
            Err(ScriptError::ForeignUpdaterRegistration)
        ));
        scene.wait(1.0).await?;
        Ok(())
    })
    .unwrap();
    second.resume().unwrap();
    finish_segment(&mut second, 1.0);
    second.resume().unwrap();
    finish_segment(&mut second, 2.0);
    assert!((second.session().frame().objects[0].transform.rotation - 2.0).abs() < 1e-5);
    assert_eq!(second.resume().unwrap(), LiveProgramStatus::Finished);
    assert_eq!(first.session().frame().time, 0.0);
}

#[test]
fn dropping_a_program_releases_updater_captures_after_registration_is_removed() {
    let captured = Rc::new(());
    let observer = Rc::downgrade(&captured);
    let mut program = Scene::script(move |scene| async move {
        let circle = scene.circle(0.5)?;
        scene.add(&circle)?;
        let registration = scene.add_updater(&circle, move |target, frame| {
            let _keep_alive = &captured;
            target.rotate_about_origin(frame.dt())
        })?;
        scene.wait(1.0).await?;
        scene.remove_updater(&registration)?;
        Ok(())
    })
    .unwrap();
    program.resume().unwrap();
    finish_segment(&mut program, 1.0);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    assert!(observer.upgrade().is_some());
    drop(program);
    assert!(observer.upgrade().is_none());
}
