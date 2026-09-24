use super::*;
use crate::{
    ContinuationStep, LiveProgramStatus, LiveSession, LiveSessionError, RateFunction,
    RustHostCallbackTable, Scene,
};
use noon_runtime::TimelineWakeState;

struct MoveThenFinish {
    source: Mobject,
    target: Mobject,
    resumes: usize,
}
impl LiveContinuation for MoveThenFinish {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        self.resumes += 1;
        if self.resumes == 1 {
            Ok(ContinuationStep::Await(
                live.declare_and_activate_transform_to(
                    &self.source,
                    &self.target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )?,
            ))
        } else {
            Ok(ContinuationStep::Finished)
        }
    }
}

fn fixture() -> (LiveProgram<MoveThenFinish>, Mobject) {
    let mut scene = Scene::new();
    let source = scene.circle(0.3).unwrap();
    let indicated = scene.circle(0.4).unwrap();
    scene.add(&source).unwrap();
    scene.add(&indicated).unwrap();
    let mut target = source.target_editor().unwrap();
    target.set_translation(2.0, 0.0).unwrap();
    let program = scene
        .into_live_program(MoveThenFinish {
            source,
            target,
            resumes: 0,
        })
        .unwrap();
    (program, indicated)
}

#[test]
fn awaited_source_and_effect_keep_independent_progress_and_wake_demand() {
    let (mut program, indicated) = fixture();
    program.take_renderer_publication();
    program.resume().unwrap();
    let token = program
        .start_indicate_effect(
            &indicated,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(2.0),
        )
        .unwrap()
        .unwrap();
    program.advance_property_animations_by(0.5).unwrap();
    assert_eq!(program.session().frame().time, 0.0);
    assert_eq!(
        program.session().property_animation_elapsed(token),
        Some(0.5)
    );
    assert_eq!(program.continuation.resumes, 1);
    assert!(program.wake_state().property_animation_pending());
    program
        .drive_to(&mut RustHostCallbackTable::new(), 1.0)
        .unwrap();
    assert_eq!(
        program.session().property_animation_elapsed(token),
        Some(0.5)
    );
    assert_eq!(program.continuation.resumes, 1);
    let publication = program.take_renderer_publication().context();
    program.admit_publication(publication).unwrap();
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    assert_eq!(program.continuation.resumes, 2);
    assert_eq!(
        program.wake_state().timeline(),
        TimelineWakeState::Quiescent
    );
    assert!(program.wake_state().property_animation_pending());
    program.advance_property_animations_by(1.5).unwrap();
    assert_eq!(program.session().frame().time, 1.0);
    assert_eq!(program.continuation.resumes, 2);
    assert_eq!(program.status(), LiveProgramStatus::Finished);
    assert!(!program.wake_state().property_animation_pending());
    program.take_renderer_publication();
    assert!(program.wake_state().is_quiescent());
}

#[test]
fn pending_endpoint_tracks_effect_activation_ticks_and_release_not_an_old_receipt() {
    let (mut program, indicated) = fixture();
    program.resume().unwrap();
    program
        .drive_to(&mut RustHostCallbackTable::new(), 1.0)
        .unwrap();
    let endpoint = program.take_renderer_publication().context();
    assert_eq!(
        program.status(),
        LiveProgramStatus::PublicationPending(endpoint)
    );
    let token = program
        .start_indicate_effect(
            &indicated,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(1.0),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        program.admit_publication(endpoint),
        Err(LiveProgramError::PublicationMismatch { .. })
    ));
    program.advance_property_animations_by(0.5).unwrap();
    let midpoint = program.take_renderer_publication().context();
    assert_eq!(
        program.status(),
        LiveProgramStatus::PublicationPending(midpoint)
    );
    program.cancel_property_animation(token).unwrap();
    assert!(matches!(
        program.admit_publication(midpoint),
        Err(LiveProgramError::PublicationMismatch { .. })
    ));
    let restored = program.take_renderer_publication().context();
    assert_eq!(
        program.admit_publication(restored).unwrap(),
        LiveProgramStatus::ReadyToResume
    );
    assert_eq!(program.continuation.resumes, 1);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
}

#[test]
fn terminal_program_cannot_start_drive_or_cancel_effects() {
    let (mut program, indicated) = fixture();
    let token = program
        .start_indicate_effect(
            &indicated,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(1.0),
        )
        .unwrap()
        .unwrap();
    program.phase = super::super::LiveProgramPhase::Terminal;
    let before = program.session().publication_context();
    assert!(matches!(
        program.advance_property_animations_by(0.5),
        Err(LiveProgramError::InvalidState { .. })
    ));
    assert!(matches!(
        program.cancel_property_animation(token),
        Err(LiveProgramError::InvalidState { .. })
    ));
    assert!(matches!(
        program.start_indicate_effect(
            &indicated,
            IndicateOptions::default(),
            AnimationOptions::new()
        ),
        Err(LiveProgramError::InvalidState { .. })
    ));
    assert_eq!(program.session().publication_context(), before);
    assert_eq!(
        program.session().property_animation_elapsed(token),
        Some(0.0)
    );
    program.take_renderer_publication();
    assert!(program.wake_state().is_quiescent());
}

#[test]
fn foreign_target_and_invalid_delta_preserve_program_phase_and_pending_receipt() {
    let (mut program, _) = fixture();
    program.resume().unwrap();
    program
        .drive_to(&mut RustHostCallbackTable::new(), 1.0)
        .unwrap();
    let before = program.status();
    let mut foreign = Scene::new();
    let object = foreign.circle(1.0).unwrap();
    assert!(program
        .start_indicate_effect(&object, IndicateOptions::default(), AnimationOptions::new())
        .is_err());
    assert!(program.advance_property_animations_by(-1.0).is_err());
    assert_eq!(program.status(), before);
    assert_eq!(program.continuation.resumes, 1);
}
