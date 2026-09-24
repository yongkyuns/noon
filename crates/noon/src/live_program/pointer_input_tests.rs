use super::*;
use crate::integration::{NativeInputModifiers, NativePointerInputKind, NativePointerPosition};
use noon_core::{
    AnimationOptions, NativePointerId, NativePointerInput, RateFunction, ReactiveValue, Vec2,
};

const POINTER: NativePointerId = NativePointerId {
    source: 1,
    pointer: 0,
};

fn press(token: &NativePointerInputToken, sequence: u64) -> NativePointerInput {
    NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        NativePointerInputKind::Press {
            position: NativePointerPosition::new(Vec2::new(2.0, 1.0), Vec2::new(400.0, 200.0))
                .unwrap(),
            button: 0,
        },
    )
}

struct Finish;
impl LiveContinuation for Finish {
    type Error = std::convert::Infallible;
    fn resume(&mut self, _: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        Ok(ContinuationStep::Finished)
    }
}

#[test]
fn finished_program_accepts_native_pointer_input_without_resuming_or_advancing_time() {
    let mut scene = Scene::new();
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    let position = scene.pointer_position_signal().unwrap();
    let down = scene.pointer_down_events(0).unwrap();
    scene.bind_native_translation(&circle, &position).unwrap();
    scene.bind_rotation(&circle, &down).unwrap();
    let mut program = scene.into_live_program(Finish).unwrap();
    program.resume().unwrap();
    let token = program.configure_native_pointer_input(POINTER, 2).unwrap();
    program.take_renderer_publication();
    let input = press(&token, 1);
    let receipt = program.submit_native_pointer_input(&token, input).unwrap();
    assert_eq!(receipt.input(), input);
    assert_eq!(program.status(), LiveProgramStatus::Finished);
    assert_eq!(program.session().frame().time, 0.0);
    assert_eq!(
        program.session().effective_signal_value(down.node_id()),
        Some(&ReactiveValue::Scalar(1.0))
    );
    assert_eq!(
        program.session().frame().objects[0].transform.translation,
        Vec2::new(2.0, 1.0)
    );
    assert_eq!(
        program.take_renderer_publication().context(),
        receipt.publication()
    );
}

struct Animate {
    source: crate::Mobject,
    target: crate::Mobject,
}
impl LiveContinuation for Animate {
    type Error = crate::LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        Ok(ContinuationStep::Await(
            live.declare_and_activate_transform_to(
                &self.source,
                &self.target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )?,
        ))
    }
}

#[test]
fn pointer_publication_refreshes_the_live_endpoint_fence_without_bypassing_it() {
    let mut scene = Scene::new();
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    let down = scene.pointer_down_events(0).unwrap();
    scene.bind_rotation(&circle, &down).unwrap();
    let mut target = circle.target_editor().unwrap();
    target.set_translation(3.0, 0.0).unwrap();
    let mut program = scene
        .into_live_program(Animate {
            source: circle,
            target,
        })
        .unwrap();
    program.take_renderer_publication();
    program.resume().unwrap();
    let LiveProgramStatus::PublicationPending(endpoint) = program
        .drive_to(&mut RustHostCallbackTable::new(), 1.0)
        .unwrap()
    else {
        panic!("endpoint must require presentation");
    };
    let token = program.configure_native_pointer_input(POINTER, 0).unwrap();
    let receipt = program
        .submit_native_pointer_input(&token, press(&token, 0))
        .unwrap();
    assert_ne!(receipt.publication(), endpoint);
    assert_eq!(
        program.status(),
        LiveProgramStatus::PublicationPending(receipt.publication())
    );
    assert!(matches!(
        program.admit_publication(endpoint),
        Err(LiveProgramError::PublicationMismatch { .. })
    ));
    assert!(matches!(
        program.admit_publication(receipt.publication()),
        Err(LiveProgramError::PublicationStillPending { .. })
    ));
    let current = program.take_renderer_publication().context();
    assert_eq!(
        program.admit_publication(current).unwrap(),
        LiveProgramStatus::ReadyToResume
    );
    assert_eq!(program.session().frame().time, 1.0);
}

struct Fail;
impl LiveContinuation for Fail {
    type Error = &'static str;
    fn resume(&mut self, _: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        Err("intentional terminal continuation")
    }
}

#[test]
fn terminal_program_rejects_contextual_input_configuration_observation_and_delivery() {
    let scene = Scene::new();
    let mut program = scene.into_live_program(Fail).unwrap();
    let token = program.configure_native_pointer_input(POINTER, 0).unwrap();
    assert!(program.resume().is_err());
    let before = program.session().publication_context();
    assert!(matches!(
        program.configure_native_pointer_input(POINTER, 1),
        Err(LiveProgramError::InvalidState { .. })
    ));
    assert!(matches!(
        program.native_pointer_input_token(),
        Err(LiveProgramError::InvalidState { .. })
    ));
    assert!(matches!(
        program.submit_native_pointer_input(&token, press(&token, 1)),
        Err(LiveProgramError::InvalidState { .. })
    ));
    assert_eq!(program.session().publication_context(), before);
}

#[test]
fn selection_configuration_preserves_program_phase_and_frame_contracts() {
    let scene = Scene::new();
    let mut program = scene.into_live_program(Finish).unwrap();
    // Like the existing host-input methods, configuration is allowed before
    // the first continuation resumes; it must not drive that continuation.
    program.set_pointer_fill_selection(Some(4.0)).unwrap();
    assert_eq!(program.status(), LiveProgramStatus::ReadyToResume);
    program.set_pointer_fill_selection(None).unwrap();
    assert!(!program.session().has_native_pointer_subscribers());
    program.resume().unwrap();
    let frame = program.session().frame().clone();
    program.set_pointer_fill_selection(Some(4.0)).unwrap();
    assert!(program.session().has_native_pointer_subscribers());
    assert!(program.set_pointer_fill_selection(Some(f32::NAN)).is_err());
    assert!(program.session().has_native_pointer_subscribers());
    program.set_pointer_fill_selection(None).unwrap();
    assert!(!program.session().has_native_pointer_subscribers());
    assert_eq!(program.session().frame(), &frame);
    assert_eq!(program.status(), LiveProgramStatus::Finished);
}

#[test]
fn terminal_program_cannot_reconfigure_transient_selection() {
    let mut program = Scene::new().into_live_program(Fail).unwrap();
    program.set_pointer_fill_selection(Some(4.0)).unwrap();
    assert!(program.resume().is_err());
    let frame = program.session().frame().clone();
    for setting in [None, Some(2.0)] {
        assert!(matches!(
            program.set_pointer_fill_selection(setting),
            Err(LiveProgramError::InvalidState { .. })
        ));
        assert!(program.session().has_native_pointer_subscribers());
        assert_eq!(program.session().frame(), &frame);
        assert_eq!(program.status(), LiveProgramStatus::Terminal);
    }
}
