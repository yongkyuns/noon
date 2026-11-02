use super::*;
use noon::{ContinuationStep, LiveSession, LiveSessionError, RateFunction, Scene, SemanticVec3};
use noon_core::{
    HostCallbackId, NativeInputModifiers, NativePointerInputKind, NativePointerPosition,
    PlaybackControl, SemanticMutationTransaction,
};

fn moving_source() -> StaticExecutionSource {
    let mut scene = Scene::new();
    let point = scene.circle(0.25).unwrap();
    scene.add(&point).unwrap();
    let tracker = scene.value_tracker(0.0).unwrap();
    let position = scene
        .position_from_tracker(
            &tracker,
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(0.0, 0.0, 0.0),
        )
        .unwrap();
    scene.bind_position(&point, &position).unwrap();
    scene
        .play_value(&tracker, 4.0)
        .rate_func(RateFunction::Linear)
        .run_time(2.0)
        .unwrap();
    StaticExecutionSource::new(
        scene.into_execution().unwrap(),
        RustHostCallbackTable::new(),
    )
}

#[test]
fn paired_native_audio_examples_admit_the_shared_finite_control_profile() {
    let (scene, animation) = noon::example_scenes::background_audio::scene().unwrap();
    let mut source = StaticExecutionSource::new(
        scene
            .into_execution_with_animation_root(&animation)
            .unwrap(),
        RustHostCallbackTable::new(),
    );
    assert!(source.session().has_playback_controls());
    assert_eq!(source.finite_transport_end(), Some(30.0));
    source.seek_to(20.0).unwrap();
    source.seek_to(10.0).unwrap();
    assert_eq!(source.frame_time(), 10.0);

    let lesson = noon::example_scenes::background_music_lesson::scene().unwrap();
    let mut source = StaticExecutionSource::new(
        lesson.into_execution().unwrap(),
        RustHostCallbackTable::new(),
    );
    assert_eq!(source.finite_transport_end(), Some(30.0));
    source.seek_to(25.0).unwrap();
    source.seek_to(5.0).unwrap();
    assert_eq!(source.frame_time(), 5.0);
}

#[test]
fn finite_native_seek_matches_forward_and_rewind_without_authored_changes() {
    let mut forward = moving_source();
    let mut direct = moving_source();
    assert_eq!(direct.finite_transport_end(), Some(2.0));
    let revision = direct.session().publication_context().scene_revision();
    for time in [0.0, 0.125, 0.75, 1.25, 2.0] {
        forward.advance_to(time).unwrap();
        direct.seek_to(time).unwrap();
        assert_eq!(direct.frame_time(), time);
        assert_eq!(direct.session().frame(), forward.session().frame());
    }
    direct.seek_to(0.125).unwrap();
    let mut reference = moving_source();
    reference.advance_to(0.125).unwrap();
    assert_eq!(direct.session().frame(), reference.session().frame());
    assert_eq!(
        direct.session().publication_context().scene_revision(),
        revision
    );
}

#[test]
fn invalid_native_seek_preserves_committed_frame_and_publication() {
    let mut source = moving_source();
    source.seek_to(0.75).unwrap();
    let frame = source.session().frame().clone();
    let publication = source.session().publication_context();
    for target in [-0.001, 2.0001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(source.seek_to(target).is_err());
        assert_eq!(source.session().frame(), &frame);
        assert_eq!(source.session().publication_context(), publication);
    }
}

#[test]
fn native_transport_adopts_a_completed_static_source_at_the_exact_zero_range() {
    struct Finished;
    impl LiveContinuation for Finished {
        type Error = LiveSessionError;
        fn resume(&mut self, _live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            Ok(ContinuationStep::Finished)
        }
    }
    let mut source = PlaybackExecutionSource::new(
        Scene::new().into_live_program(Finished).unwrap(),
        RustHostCallbackTable::new(),
    )
    .unwrap();
    assert!(!source.source_active());
    assert_eq!(source.session().finite_timeline_end_time(), Some(0.0));
    assert_eq!(source.finite_transport_end(), Some(0.0));
    source.seek_to(0.0).unwrap();
    assert_eq!(source.frame_time(), 0.0);
    assert!(!source.can_seek_to(0.001));
}

#[test]
fn native_transport_rejects_a_completed_live_projection_even_after_seek_to_zero() {
    let mut scene = Scene::new();
    let marker = scene.circle(0.5).unwrap();
    scene.add(&marker).unwrap();
    let mut target = marker.target_editor().unwrap();
    target.set_translation(4.0, 0.0).unwrap();
    let mut session = scene.into_execution().unwrap();
    assert_eq!(session.session().painter_order().len(), 1);
    let initial_revision = session.session().publication_context().execution_revision();
    let segment = session
        .live()
        .declare_and_activate_transform_to(
            &marker,
            &target,
            noon::AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    session.session_mut().advance_to(1.0).unwrap();
    session.live().complete_segment(segment).unwrap();
    session.live().remove(&marker).unwrap();
    session.seek(0.0).unwrap();
    // The completed source removed its marker without retaining history; the
    // current finite plan cannot reconstruct the former time-zero membership.
    assert_eq!(session.session().frame().time, 0.0);
    assert!(session.session().painter_order().is_empty());
    assert!(
        session
            .session()
            .publication_context()
            .execution_revision()
            .get()
            > initial_revision.get()
    );
    assert!(session.session().finite_timeline_end_time().is_some());
    assert!(!session.session().replay_scope_active());
    let mut source = StaticExecutionSource::new(session, RustHostCallbackTable::new());
    assert_eq!(source.finite_transport_end(), None);
    assert!(matches!(
        source.seek_to(0.0),
        Err(NativeHostError::PlaybackControlsUnavailable)
    ));
}

#[test]
fn native_transport_does_not_admit_required_callbacks() {
    let mut scene = Scene::new();
    let marker = scene.circle(0.5).unwrap();
    scene.add(&marker).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(marker.node_id(), HostCallbackId::new(1), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut source = StaticExecutionSource::new(
        scene.into_execution().unwrap(),
        RustHostCallbackTable::new(),
    );
    assert!(source.session().has_required_callbacks());
    assert_eq!(source.finite_transport_end(), None);
    assert!(matches!(
        source.seek_to(0.0),
        Err(NativeHostError::PlaybackControlsUnavailable)
    ));
}

#[test]
fn native_seek_rechecks_external_owner_after_admission() {
    let mut source = moving_source();
    assert_eq!(source.finite_transport_end(), Some(2.0));
    source
        .session
        .session_mut()
        .begin_replay_retention(Default::default())
        .unwrap();
    let publication = source.session().publication_context();
    assert_eq!(source.finite_transport_end(), None);
    assert!(matches!(
        source.seek_to(0.5),
        Err(NativeHostError::PlaybackControlsUnavailable)
    ));
    assert_eq!(source.session().publication_context(), publication);
}

#[test]
fn native_source_forwards_each_committed_scene_control_receipt_once() {
    for control in [
        PlaybackControl::TogglePlayback,
        PlaybackControl::PreviousSection,
        PlaybackControl::NextSection,
        PlaybackControl::Restart,
        PlaybackControl::ToggleAudioMuted,
    ] {
        let mut scene = Scene::new();
        let mut button = scene.rectangle(2.0, 1.0).unwrap();
        button.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
        scene.add(&button).unwrap();
        scene.on_click_playback(&button, control).unwrap();
        let mut source = StaticExecutionSource::new(
            scene.into_execution().unwrap(),
            RustHostCallbackTable::new(),
        );
        assert_eq!(source.finite_transport_end(), Some(0.0));
        source
            .configure_native_pointer_input(
                NativePointerId {
                    source: 3,
                    pointer: 0,
                },
                1,
            )
            .unwrap();
        let revision = source.session().publication_context().scene_revision();
        let position = NativePointerPosition::new(Vec2::ZERO, Vec2::new(100.0, 100.0)).unwrap();
        for (sequence, kind, expected) in [
            (
                1,
                NativePointerInputKind::Press {
                    position,
                    button: 0,
                },
                None,
            ),
            (
                2,
                NativePointerInputKind::Release {
                    position,
                    button: 0,
                },
                Some(control),
            ),
            (
                3,
                NativePointerInputKind::Release {
                    position,
                    button: 0,
                },
                None,
            ),
        ] {
            let token = source.native_pointer_input_token().unwrap();
            let input = NativePointerInput::new(
                sequence,
                token.pointer(),
                token.context(),
                NativeInputModifiers::default(),
                kind,
            );
            let receipt = source.submit_native_pointer_input(&token, input).unwrap();
            assert_eq!(receipt.playback_control(), expected);
            assert_eq!(source.frame_time(), 0.0);
            assert_eq!(
                source.session().publication_context().scene_revision(),
                revision
            );
            assert_eq!(source.finite_transport_end(), Some(0.0));
        }
    }
}
