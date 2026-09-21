use super::*;
use crate::{ContinuationStep, LiveContinuation, LiveProgramError, LiveSession, Scene};
use noon_core::{
    AnimationOptions, NativeInputModifiers, NativePointerCancellation, NativePointerId,
    NativePointerInput, NativePointerInputKind, RateFunction, SemanticNodeId, SemanticObjectState,
    SemanticStore, SemanticVec3, StoredGeometry,
};

const POINTER: NativePointerId = NativePointerId {
    source: 9,
    pointer: 7,
};
const VIEW: u64 = 3;
const SIZE: Vec2 = Vec2::new(800.0, 400.0);
const CENTER: Vec2 = Vec2::new(400.0, 200.0);

fn view() -> PointerFrameView {
    PointerFrameView::new(VIEW, SIZE, Camera2DState::default()).unwrap()
}

fn fixture() -> (Scene, SemanticNodeId, ExecutionSession) {
    let mut scene = Scene::new();
    let mut circle = scene.circle(1.0).unwrap();
    circle.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .configure_native_pointer_input(POINTER, VIEW)
        .unwrap();
    session.enable_pointer_fill_selection(4.0).unwrap();
    (scene, circle.node_id(), session)
}

fn input(
    token: &NativePointerInputToken,
    n: u64,
    kind: NativePointerInputKind,
) -> NativePointerInput {
    NativePointerInput::new(
        n,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        kind,
    )
}

fn press(frame: &PointerFrameSnapshot) -> NativePointerInputKind {
    NativePointerInputKind::Press {
        position: frame.position(CENTER).unwrap(),
        button: 0,
    }
}

fn moving_fixture() -> ExecutionSession {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    let source = store.insert_semantic_object(state.clone());
    store.add_semantic_family_member(root, source).unwrap();
    let mut end = state;
    end.transform.translation = SemanticVec3::new(4.0, 0.0, 0.0);
    let target = store.insert_semantic_object(end);
    let animation = store
        .insert_semantic_transform_animation(source, target, AnimationOptions::new())
        .unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session
        .configure_native_pointer_input(POINTER, VIEW)
        .unwrap();
    session.enable_pointer_fill_selection(4.0).unwrap();
    session
        .activate_animation_segment(
            &store,
            animation,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    session
}

#[test]
fn invalid_view_is_rejected_before_capture() {
    for size in [
        Vec2::ZERO,
        Vec2::new(-1.0, 2.0),
        Vec2::new(2.0, 0.0),
        Vec2::new(f32::NAN, 2.0),
        Vec2::new(2.0, f32::INFINITY),
    ] {
        assert_eq!(
            PointerFrameView::new(VIEW, size, Camera2DState::default()),
            Err(PointerFrameError::InvalidView)
        );
    }
    for camera in [
        Camera2DState {
            center: Vec2::new(f32::NAN, 0.0),
            height: 8.0,
        },
        Camera2DState {
            center: Vec2::ZERO,
            height: 0.0,
        },
        Camera2DState {
            center: Vec2::ZERO,
            height: f32::INFINITY,
        },
    ] {
        assert_eq!(
            PointerFrameView::new(VIEW, SIZE, camera),
            Err(PointerFrameError::InvalidView)
        );
    }
}

#[test]
fn capture_does_not_acknowledge_present_consume_dirtiness_or_advance_time() {
    let (_, _, mut session) = fixture();
    let state = session.frame().clone();
    let context = session.publication_context();
    let wake = session.wake_state();
    for _ in 0..100 {
        let snapshot = session.capture_pointer_frame(view()).unwrap();
        assert_eq!(snapshot.publication(), context);
        assert_eq!(snapshot.view(), view());
        assert_eq!(snapshot.view().revision(), VIEW);
        assert_eq!(snapshot.view().viewport(), SIZE);
        assert_eq!(snapshot.view().camera(), Camera2DState::default());
        assert_eq!(
            snapshot.input_token(&session, view()).unwrap(),
            session.native_pointer_input_token().unwrap()
        );
    }
    assert_eq!(session.frame(), &state);
    assert_eq!(session.wake_state(), wake);
    assert_eq!(session.last_native_event_sequence, None);
    assert!(!session.take_renderer_publication().changes().is_empty());
}

#[test]
fn snapshot_can_be_captured_before_any_pointer_is_configured() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut session = scene.execution_session().unwrap();
    let frame = session.capture_pointer_frame(view()).unwrap();
    assert_eq!(
        frame.input_token(&session, view()),
        Err(PointerFrameError::Input(
            ExecutionSessionInputError::PointerNotConfigured
        ))
    );
    session
        .configure_native_pointer_input(POINTER, VIEW)
        .unwrap();
    assert!(frame.input_token(&session, view()).is_ok());
}

#[test]
fn mapping_preserves_surface_pixels_and_supports_outside_capture_positions() {
    let (_, _, session) = fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    for (surface, world) in [
        (CENTER, Vec2::ZERO),
        (Vec2::ZERO, Vec2::new(-8.0, 4.0)),
        (SIZE, Vec2::new(8.0, -4.0)),
        (Vec2::new(900.0, -100.0), Vec2::new(10.0, 6.0)),
    ] {
        let position = frame.position(surface).unwrap();
        assert_eq!(position.surface(), surface);
        assert_eq!(position.scene(), world);
    }
}

#[test]
fn mapping_uses_the_captured_translated_zoomed_camera() {
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame().unwrap();
    camera.set_translation(5.0, -3.0).unwrap();
    camera.set_scale(0.5, 0.5).unwrap();
    let session = scene.execution_session().unwrap();
    let camera = session.camera().unwrap();
    let frame = session
        .capture_pointer_frame(PointerFrameView::new(VIEW, SIZE, camera).unwrap())
        .unwrap();
    assert_eq!(
        frame.position(CENTER).unwrap().scene(),
        Vec2::new(5.0, -3.0)
    );
    assert_eq!(
        frame.position(Vec2::ZERO).unwrap().scene(),
        Vec2::new(1.0, -1.0)
    );
}

#[test]
fn manual_render_camera_cannot_be_mislabelled_as_the_execution_camera() {
    let (_, _, session) = fixture();
    let camera = Camera2DState {
        center: Vec2::new(1.0, 0.0),
        height: 8.0,
    };
    assert_eq!(
        session.capture_pointer_frame(PointerFrameView::new(VIEW, SIZE, camera).unwrap()),
        Err(PointerFrameError::CameraMismatch)
    );
}

#[test]
fn projection_rejects_nonfinite_input_and_unrepresentable_output() {
    let (_, _, session) = fixture();
    let mut frame = session.capture_pointer_frame(view()).unwrap();
    for point in [Vec2::new(f32::NAN, 0.0), Vec2::new(0.0, f32::INFINITY)] {
        assert_eq!(
            frame.position(point),
            Err(PointerFrameError::PositionOutOfRange)
        );
    }
    // Private test-only construction isolates arithmetic limits from camera authoring.
    frame.view = PointerFrameView::new(
        VIEW,
        Vec2::ONE,
        Camera2DState {
            center: Vec2::ZERO,
            height: f32::MAX,
        },
    )
    .unwrap();
    assert_eq!(
        frame.position(Vec2::new(f32::MAX, 0.0)),
        Err(PointerFrameError::PositionOutOfRange)
    );
}

#[test]
fn widened_projection_avoids_intermediate_overflow_for_a_finite_result() {
    let (_, _, session) = fixture();
    let mut frame = session.capture_pointer_frame(view()).unwrap();
    frame.view = PointerFrameView::new(
        VIEW,
        Vec2::new(f32::MAX, f32::MAX),
        Camera2DState {
            center: Vec2::ZERO,
            height: 1.0,
        },
    )
    .unwrap();
    let point = frame.position(Vec2::new(-f32::MAX, 0.0)).unwrap();
    assert_eq!(point.scene(), Vec2::new(-1.5, 0.5));
}

#[test]
fn a_stale_display_cannot_issue_a_token_for_the_newer_execution() {
    let mut session = moving_fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    session.advance_to(1.0).unwrap();
    let before = session.frame().clone();
    assert_eq!(
        frame.input_token(&session, view()),
        Err(PointerFrameError::Input(
            ExecutionSessionInputError::StalePointerPublication {
                expected: session.publication_context(),
                actual: frame.publication()
            }
        ))
    );
    assert_eq!(session.frame(), &before);
    assert_eq!(session.last_native_event_sequence, None);
}

#[test]
fn advancing_after_token_capture_is_rejected_by_both_picking_and_admission() {
    let mut session = moving_fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    let token = frame.input_token(&session, view()).unwrap();
    let occurrence = input(&token, 8, press(&frame));
    session.advance_to(1.0).unwrap();
    let before = session.frame().clone();
    assert!(matches!(
        session.pick_native_pointer_fill(&token, occurrence, |_| true),
        Err(ExecutionSessionInputError::StalePointerPublication { .. })
    ));
    assert!(matches!(
        session.submit_native_pointer_input(&token, occurrence),
        Err(ExecutionSessionInputError::StalePointerPublication { .. })
    ));
    assert_eq!(session.frame(), &before);
    assert_eq!(session.last_native_event_sequence, None);
    // A genuinely new frame can authorize a new sample, reusing the unacknowledged serial.
    let fresh = session.capture_pointer_frame(view()).unwrap();
    let token = fresh.input_token(&session, view()).unwrap();
    session
        .submit_native_pointer_input(&token, input(&token, 8, press(&fresh)))
        .unwrap();
    assert_eq!(session.last_native_event_sequence, Some(8));
}

#[test]
fn view_revision_size_and_camera_changes_are_all_rejected() {
    let (_, _, session) = fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    for changed in [
        PointerFrameView::new(VIEW + 1, SIZE, Camera2DState::default()).unwrap(),
        PointerFrameView::new(VIEW, Vec2::new(400.0, 400.0), Camera2DState::default()).unwrap(),
        PointerFrameView::new(
            VIEW,
            SIZE,
            Camera2DState {
                center: Vec2::new(1.0, 0.0),
                height: 8.0,
            },
        )
        .unwrap(),
    ] {
        assert_eq!(
            frame.input_token(&session, changed),
            Err(PointerFrameError::ViewChanged)
        );
    }
    assert_eq!(session.last_native_event_sequence, None);
}

#[test]
fn a_reconfigured_view_invalidates_the_old_receipt_even_with_unchanged_frame() {
    let (_, _, mut session) = fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    session
        .configure_native_pointer_input(POINTER, VIEW + 1)
        .unwrap();
    assert_eq!(
        frame.input_token(&session, view()),
        Err(PointerFrameError::ViewChanged)
    );
}

#[test]
fn same_view_source_rebinding_does_not_reanimate_an_old_contact_token() {
    let (_, _, mut session) = fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    let old = frame.input_token(&session, view()).unwrap();
    session
        .configure_native_pointer_input(POINTER, VIEW)
        .unwrap();
    assert_eq!(
        session.submit_native_pointer_input(&old, input(&old, 0, press(&frame))),
        Err(ExecutionSessionInputError::StalePointerBinding)
    );
    // The unchanged displayed image may serve a new contact, but never its old token.
    assert_ne!(frame.input_token(&session, view()).unwrap(), old);
}

#[test]
fn moving_execution_preserves_receipts_but_cloning_never_does() {
    let (_, _, session) = fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    let moved = session;
    assert!(frame.input_token(&moved, view()).is_ok());
    let cloned = moved.clone();
    assert_eq!(cloned.publication_context(), moved.publication_context());
    assert_eq!(
        frame.input_token(&cloned, view()),
        Err(PointerFrameError::Input(
            ExecutionSessionInputError::ForeignPointerRuntime
        ))
    );
}

#[test]
fn cancellation_can_retire_a_pressed_contact_after_execution_outpaces_the_display() {
    let mut session = moving_fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    let token = frame.input_token(&session, view()).unwrap();
    session
        .submit_native_pointer_input(&token, input(&token, 0, press(&frame)))
        .unwrap();
    session.advance_to(1.0).unwrap();
    let receipt = session
        .submit_native_pointer_input(
            &token,
            input(
                &token,
                1,
                NativePointerInputKind::Cancel(NativePointerCancellation::CaptureLost),
            ),
        )
        .unwrap();
    assert_eq!(receipt.selection_click(), None);
    let fresh = session.capture_pointer_frame(view()).unwrap();
    let token = fresh.input_token(&session, view()).unwrap();
    let release = NativePointerInputKind::Release {
        position: fresh.position(CENTER).unwrap(),
        button: 0,
    };
    assert_eq!(
        session
            .submit_native_pointer_input(&token, input(&token, 2, release))
            .unwrap()
            .selection_click(),
        None
    );
}

#[test]
fn frame_qualified_click_uses_the_existing_session_selection_and_local_picker() {
    let (_, target, mut session) = fixture();
    session.take_frame_changes();
    let frame = session.capture_pointer_frame(view()).unwrap();
    let before = session.frame().clone();
    let token = frame.input_token(&session, view()).unwrap();
    let down = session
        .submit_native_pointer_input(&token, input(&token, 0, press(&frame)))
        .unwrap();
    let up = session
        .submit_native_pointer_input(
            &token,
            input(
                &token,
                1,
                NativePointerInputKind::Release {
                    position: frame.position(CENTER).unwrap(),
                    button: 0,
                },
            ),
        )
        .unwrap();
    assert_eq!(up.selection_click().unwrap().target(), Some(target));
    assert_eq!(down.selection_query().unwrap().precise_tests(), 1);
    assert_eq!(
        up.selection_query()
            .unwrap()
            .spatial_stats()
            .candidates_tested,
        1
    );
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), frame.publication());
    assert!(session.take_frame_changes().is_empty());
    assert!(
        frame.input_token(&session, view()).is_ok(),
        "selection-only redraw does not invent an execution epoch"
    );
}

#[test]
fn snapshot_is_neither_a_pending_callback_bypass_nor_an_input_acknowledgement() {
    let (_, target, mut session) = fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    let token = frame.input_token(&session, view()).unwrap();
    let overlay = session
        .begin_required_callback_phase(0.0, [target])
        .unwrap();
    assert_eq!(
        frame.input_token(&session, view()),
        Err(PointerFrameError::Input(
            ExecutionSessionInputError::RequiredCallbackPending
        ))
    );
    assert_eq!(
        session.submit_native_pointer_input(
            &token,
            input(
                &token,
                0,
                NativePointerInputKind::Cancel(NativePointerCancellation::Cancelled)
            )
        ),
        Err(ExecutionSessionInputError::RequiredCallbackPending)
    );
    assert_eq!(session.last_native_event_sequence, None);
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
}

struct Finish;
impl LiveContinuation for Finish {
    type Error = std::convert::Infallible;
    fn resume(&mut self, _: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        Ok(ContinuationStep::Finished)
    }
}

#[test]
fn finished_program_accepts_frame_qualified_input_without_advancing_time() {
    let (scene, _, _) = fixture();
    let mut program = scene.into_live_program(Finish).unwrap();
    let frame = program.session().capture_pointer_frame(view()).unwrap();
    program
        .configure_native_pointer_input(POINTER, VIEW)
        .unwrap();
    assert_eq!(program.status(), crate::LiveProgramStatus::ReadyToResume);
    program.resume().unwrap();
    program
        .configure_native_pointer_input(POINTER, VIEW)
        .unwrap();
    let token = frame.input_token(program.session(), view()).unwrap();
    let receipt = program
        .submit_native_pointer_input(&token, input(&token, 0, press(&frame)))
        .unwrap();
    assert_eq!(receipt.input().context().publication, frame.publication());
    assert_eq!(program.session().frame().time, 0.0);
    assert!(matches!(
        program.resume(),
        Err(LiveProgramError::InvalidState { .. })
    ));
}

#[test]
fn returning_to_the_same_image_does_not_revive_a_historical_frame() {
    let mut session = moving_fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    session.advance_to(1.0).unwrap();
    session.seek(0.0).unwrap();
    assert_eq!(session.frame().time, 0.0);
    assert!(matches!(
        frame.input_token(&session, view()),
        Err(PointerFrameError::Input(
            ExecutionSessionInputError::StalePointerPublication { .. }
        ))
    ));
}

#[test]
fn cancellation_cannot_use_a_frame_token_from_a_retired_binding() {
    let (_, _, mut session) = fixture();
    let frame = session.capture_pointer_frame(view()).unwrap();
    let token = frame.input_token(&session, view()).unwrap();
    session
        .configure_native_pointer_input(
            NativePointerId {
                source: 10,
                pointer: 7,
            },
            VIEW,
        )
        .unwrap();
    assert_eq!(
        session.submit_native_pointer_input(
            &token,
            input(
                &token,
                0,
                NativePointerInputKind::Cancel(NativePointerCancellation::Cancelled)
            )
        ),
        Err(ExecutionSessionInputError::StalePointerBinding)
    );
    assert_eq!(session.last_native_event_sequence, None);
}

struct Fail;
impl LiveContinuation for Fail {
    type Error = &'static str;
    fn resume(&mut self, _: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        Err("intentional terminal continuation")
    }
}

#[test]
fn a_frame_snapshot_does_not_bypass_the_live_program_terminal_barrier() {
    let (scene, _, _) = fixture();
    let mut program = scene.into_live_program(Fail).unwrap();
    program
        .configure_native_pointer_input(POINTER, VIEW)
        .unwrap();
    let frame = program.session().capture_pointer_frame(view()).unwrap();
    let token = frame.input_token(program.session(), view()).unwrap();
    assert!(program.resume().is_err());
    let before = program.session().frame().clone();
    // A frame is read-only metadata, not permission to resume a terminated program.
    assert!(frame.input_token(program.session(), view()).is_ok());
    for kind in [
        press(&frame),
        NativePointerInputKind::Cancel(NativePointerCancellation::Cancelled),
    ] {
        assert!(matches!(
            program.submit_native_pointer_input(&token, input(&token, 0, kind)),
            Err(LiveProgramError::InvalidState { .. })
        ));
    }
    assert_eq!(program.session().frame(), &before);
    assert_eq!(program.session().last_native_event_sequence, None);
}
