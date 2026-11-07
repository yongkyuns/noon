use std::{
    cell::{Cell, RefCell},
    future::{pending, poll_fn, Future},
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
    rc::Rc,
    task::Poll,
};

use noon_core::HostCallbackId;

use super::*;
use crate::{
    AnimationOptions, ContinuationEffects, LiveProgramError, LiveProgramStatus, RateFunction, Vec2,
};

fn admit_if_pending(program: &mut LiveProgram<SceneScript>) {
    if let LiveProgramStatus::PublicationPending(expected) = program.status() {
        let actual = program.take_renderer_publication().context();
        assert_eq!(actual, expected);
        assert_eq!(
            program.admit_publication(actual).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
    }
}

fn finish_wait(program: &mut LiveProgram<SceneScript>, time: f64) {
    program
        .drive_to(&mut RustHostCallbackTable::new(), time)
        .unwrap();
    admit_if_pending(program);
}

#[test]
fn sequential_source_resumes_only_after_exact_completion_and_publication() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let observations = Rc::new(RefCell::new(Vec::new()));
    let source_events = Rc::clone(&events);
    let source_observations = Rc::clone(&observations);
    let mut program = Scene::script(move |scene| async move {
        source_events.borrow_mut().push("setup");
        let object = scene.circle(0.5)?;
        scene.add(&object)?;
        scene
            .play_with(
                object.animate().shift(Vec2::new(2.0, 0.0)),
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .await?;
        source_events.borrow_mut().push("after play");
        source_observations.borrow_mut().push((
            scene.time()?,
            scene.effective(&object)?.transform.translation,
        ));
        scene.wait(0.5).await?;
        source_events.borrow_mut().push("after wait");
        Ok(())
    })
    .unwrap();

    // The cold setup runs exactly once. The first await has not activated yet.
    assert_eq!(&*events.borrow(), &["setup"]);
    assert_eq!(program.session().frame().time, 0.0);
    assert_eq!(program.session().frame().objects.len(), 1);
    let initial = program.take_renderer_publication().context();
    assert!(matches!(
        program.resume().unwrap(),
        LiveProgramStatus::Awaiting(_)
    ));
    assert_eq!(&*events.borrow(), &["setup"]);

    let mut callbacks = RustHostCallbackTable::new();
    assert!(matches!(
        program.drive_to(&mut callbacks, 1.0).unwrap(),
        LiveProgramStatus::Awaiting(_)
    ));
    assert_eq!(
        program.session().frame().objects[0].transform.translation.x,
        1.0
    );
    assert!(matches!(
        program.resume(),
        Err(LiveProgramError::InvalidState { .. })
    ));
    let LiveProgramStatus::PublicationPending(endpoint) =
        program.drive_to(&mut callbacks, 8.0).unwrap()
    else {
        panic!("animated endpoint must require publication admission");
    };
    assert_eq!(&*events.borrow(), &["setup"]);
    assert!(matches!(
        program.resume(),
        Err(LiveProgramError::InvalidState { .. })
    ));
    assert!(matches!(
        program.admit_publication(initial),
        Err(LiveProgramError::PublicationMismatch { .. })
    ));
    assert!(matches!(
        program.admit_publication(endpoint),
        Err(LiveProgramError::PublicationStillPending { .. })
    ));

    admit_if_pending(&mut program);
    assert!(matches!(
        program.resume().unwrap(),
        LiveProgramStatus::Awaiting(_)
    ));
    assert_eq!(&*events.borrow(), &["setup", "after play"]);
    assert_eq!(&*observations.borrow(), &[(2.0, Vec2::new(2.0, 0.0))]);
    finish_wait(&mut program, 2.5);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    assert_eq!(&*events.borrow(), &["setup", "after play", "after wait"]);
}

#[test]
fn source_can_finish_during_setup_without_being_polled_twice() {
    let calls = Rc::new(Cell::new(0));
    let source_calls = Rc::clone(&calls);
    let mut program = Scene::script(move |scene| async move {
        source_calls.set(source_calls.get() + 1);
        let object = scene.square(1.0)?;
        scene.add(&object)?;
        Ok(())
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(program.session().frame().objects.len(), 1);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    assert_eq!(calls.get(), 1);
    assert!(matches!(
        program.resume(),
        Err(LiveProgramError::InvalidState { .. })
    ));
    assert_eq!(SceneScript::EFFECTS, ContinuationEffects::Opaque);
}

#[test]
fn invalid_first_and_later_operations_return_through_the_same_await_result() {
    let caught = Rc::new(Cell::new(0));
    let source_caught = Rc::clone(&caught);
    let mut program = Scene::script(move |scene| async move {
        assert!(scene.wait(-1.0).await.is_err());
        source_caught.set(source_caught.get() + 1);
        scene.wait(0.25).await?;
        assert!(scene.wait(f64::NAN).await.is_err());
        source_caught.set(source_caught.get() + 1);
        scene.wait(0.25).await?;
        Ok(())
    })
    .unwrap();
    assert_eq!(caught.get(), 0);
    assert!(matches!(
        program.resume().unwrap(),
        LiveProgramStatus::Awaiting(_)
    ));
    assert_eq!(caught.get(), 1);
    finish_wait(&mut program, 0.25);
    assert!(matches!(
        program.resume().unwrap(),
        LiveProgramStatus::Awaiting(_)
    ));
    assert_eq!(caught.get(), 2);
    finish_wait(&mut program, 0.5);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
}

#[test]
fn unpolled_awaitables_are_inert() {
    let mut program = Scene::script(|scene| async move {
        // This would fail if constructing or dropping an awaitable ran it.
        drop(scene.wait(-1.0));
        scene.wait(0.0).await?;
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        program.resume().unwrap(),
        LiveProgramStatus::Awaiting(_)
    ));
    finish_wait(&mut program, 0.0);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
}

#[test]
fn unrelated_suspension_fails_closed_before_and_after_a_noon_boundary() {
    assert!(matches!(
        Scene::script(|_| async {
            pending::<()>().await;
            Ok(())
        }),
        Err(ScriptError::UnsupportedSuspension)
    ));

    let mut program = Scene::script(|scene| async move {
        scene.wait(0.0).await?;
        pending::<()>().await;
        Ok(())
    })
    .unwrap();
    program.resume().unwrap();
    finish_wait(&mut program, 0.0);
    assert!(matches!(
        program.resume(),
        Err(LiveProgramError::Continuation(
            ScriptError::UnsupportedSuspension
        ))
    ));
    assert_eq!(program.status(), LiveProgramStatus::Terminal);
}

#[test]
fn concurrent_and_abandoned_boundaries_are_rejected_explicitly() {
    assert!(matches!(
        Scene::script(|scene| async move {
            let mut first = scene.wait(1.0);
            let mut second = scene.wait(1.0);
            poll_fn(|context| {
                assert!(Pin::new(&mut first).poll(context).is_pending());
                Pin::new(&mut second).poll(context)
            })
            .await?;
            Ok(())
        }),
        Err(ScriptError::ConcurrentBoundary)
    ));

    assert!(matches!(
        Scene::script(|scene| async move {
            let mut wait = scene.wait(1.0);
            poll_fn(|context| {
                assert!(Pin::new(&mut wait).poll(context).is_pending());
                Poll::Ready(())
            })
            .await;
            drop(wait);
            Ok(())
        }),
        Err(ScriptError::UnawaitedBoundary)
    ));
}

#[test]
fn capabilities_are_scoped_reentrancy_is_fallible_and_clones_do_not_retain_a_source() {
    let escaped = Rc::new(RefCell::new(None));
    let source_escaped = Rc::clone(&escaped);
    let mut program = Scene::script(move |scene| async move {
        let nested = scene.clone();
        assert!(matches!(
            scene.with_scene(|_| nested.time().map(|_| ())),
            Err(ScriptError::Reentrant)
        ));
        *source_escaped.borrow_mut() = Some(scene.clone());
        scene.wait(0.0).await?;
        Ok(())
    })
    .unwrap();
    let capability = escaped.borrow().as_ref().unwrap().clone();
    assert!(matches!(capability.time(), Err(ScriptError::Inactive)));
    program.resume().unwrap();
    assert!(matches!(capability.time(), Err(ScriptError::Inactive)));
    finish_wait(&mut program, 0.0);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    assert!(matches!(capability.time(), Err(ScriptError::Inactive)));
    drop(program);
    assert!(capability.bridge.upgrade().is_none());
    assert!(matches!(capability.time(), Err(ScriptError::Inactive)));
}

#[test]
fn unwinding_restores_the_scene_and_closes_the_program() {
    let escaped = Rc::new(RefCell::new(None));
    let source_escaped = Rc::clone(&escaped);
    let mut program = Scene::script(move |scene| async move {
        let object = scene.circle(1.0)?;
        scene.add(&object)?;
        *source_escaped.borrow_mut() = Some(scene.clone());
        scene.wait(0.0).await?;
        panic!("source panic after a completed boundary");
    })
    .unwrap();
    program.resume().unwrap();
    finish_wait(&mut program, 0.0);
    assert!(catch_unwind(AssertUnwindSafe(|| program.resume())).is_err());
    assert_eq!(program.status(), LiveProgramStatus::Terminal);
    assert_eq!(program.session().frame().objects.len(), 1);
    let capability = escaped.borrow().as_ref().unwrap().clone();
    assert!(matches!(capability.time(), Err(ScriptError::Inactive)));
    let bridge = capability.bridge.upgrade().unwrap();
    assert!(bridge.borrow().scene.is_none());
    assert!(bridge.borrow().callbacks.is_none());
    assert!(matches!(
        program.resume(),
        Err(LiveProgramError::InvalidState { .. })
    ));
}

#[test]
fn owned_callbacks_drive_normally_and_reject_a_competing_host_table_before_advancing() {
    let calls = Rc::new(Cell::new(0));
    let source_calls = Rc::clone(&calls);
    let mut program = Scene::script(move |scene| async move {
        let object = scene.circle(0.5)?;
        scene.add(&object)?;
        scene.shift(&object, Vec2::new(2.0, 0.0))?;
        let updater = scene.add_updater(&object, move |target, frame| {
            source_calls.set(source_calls.get() + 1);
            target.rotate_about_origin(frame.dt())
        })?;
        scene.wait(1.0).await?;
        scene.remove_updater(&updater)?;
        scene.wait(0.5).await?;
        Ok(())
    })
    .unwrap();
    program.take_renderer_publication();
    program.resume().unwrap();

    let mut competing = RustHostCallbackTable::new();
    competing
        .insert(HostCallbackId::new(999), |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap();
    let before = program.session().publication_context();
    assert!(matches!(
        program.drive_to(&mut competing, 0.5),
        Err(LiveProgramError::CompetingCallbackTables)
    ));
    assert_eq!(program.session().publication_context(), before);
    assert_eq!(program.session().frame().time, 0.0);
    assert_eq!(calls.get(), 0);
    assert!(matches!(program.status(), LiveProgramStatus::Awaiting(_)));

    let mut empty_host_table = RustHostCallbackTable::new();
    program.drive_to(&mut empty_host_table, 1.0).unwrap();
    assert!(calls.get() > 0);
    assert!(empty_host_table.last_advance_completed_callback_phase());
    admit_if_pending(&mut program);
    program.resume().unwrap();
    let final_calls = calls.get();
    program.drive_to(&mut empty_host_table, 1.5).unwrap();
    assert_eq!(calls.get(), final_calls);
    assert!(!empty_host_table.last_advance_completed_callback_phase());
    admit_if_pending(&mut program);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
}
