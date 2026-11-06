use super::super::ExecutionSession;
use crate::{RateFunction, Scene, SemanticVec3};
use noon_core::{
    GeometryRef, NativeEventSource, NativePointerId, NativeStateSource, ObjectContentRef,
    PlaybackControl, SemanticObjectProperty, SemanticObjectState, SemanticStore, StoredGeometry,
    Transform2D, Vec2,
};

fn scalar_scene() -> (Scene, crate::Mobject) {
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
    (scene, point)
}

#[test]
fn immutable_playback_admits_scalar_tracks_and_viewer_pointer_policy() {
    let (mut scene, _) = scalar_scene();
    let button = scene.rectangle(1.0, 0.5).unwrap();
    scene.add(&button).unwrap();
    scene
        .on_click_playback(&button, PlaybackControl::TogglePlayback)
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session.enable_pointer_fill_selection(4.0).unwrap();
    session
        .configure_native_pointer_input(
            NativePointerId {
                source: 1,
                pointer: 0,
            },
            1,
        )
        .unwrap();
    assert!(session.has_native_pointer_subscribers());
    assert!(session.has_playback_controls());
    assert!(session.supports_immutable_playback());
    assert_eq!(session.finite_playback_end_time(), Some(2.0));
    let revision = session.publication_context().scene_revision();
    session.take_frame_changes();
    let view = session
        .inspection_pointer_view(1, Vec2::new(800.0, 400.0))
        .unwrap();
    let displayed = session.capture_pointer_frame(view).unwrap();
    session
        .zoom_inspection_view(&displayed, view, Vec2::new(400.0, 200.0), 1.25)
        .unwrap();
    assert!(session.supports_immutable_playback());
    session.seek(1.5).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 3.0);
    session.seek(0.5).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 1.0);
    assert_eq!(session.publication_context().scene_revision(), revision);
    assert!(session.supports_immutable_playback());
}

#[test]
fn prepared_reader_replays_deterministic_scalar_timeline() {
    let (scene, _) = scalar_scene();
    let mut session = scene.execution_session().unwrap();
    let mut expected = scene.execution_session().unwrap();
    assert!(session.supports_immutable_playback());
    session.begin_replay_retention(Default::default()).unwrap();
    assert_eq!(session.replay_stats().payloads_retained, 1);
    let boundary = session.wait_segment(1.0).unwrap();
    session.advance_to(boundary.end_time()).unwrap();

    let (_, mut reader) = session.create_prepared_prefix(boundary).unwrap();
    reader.seek(0.5).unwrap();
    expected.seek(0.5).unwrap();
    assert_eq!(reader.frame(), expected.frame());
    assert_eq!(reader.frame().objects[0].transform.translation.x, 1.0);
    reader.advance_to(0.75).unwrap();
    expected.advance_to(0.75).unwrap();
    assert_eq!(reader.frame(), expected.frame());
}

#[test]
fn initial_scalar_schedule_counts_toward_retention_limit() {
    let (scene, _) = scalar_scene();
    let mut session = scene.execution_session().unwrap();
    assert_eq!(
        session.begin_replay_retention(noon_runtime::ReplayLimits {
            revisions: 1,
            payloads: 0,
        }),
        Err(noon_runtime::ReplayError::RetentionLimit)
    );
    assert!(!session.replay_scope_active());
}

#[test]
fn prepared_reader_adopts_scalar_timeline_suffix_while_paused() {
    let mut scene = Scene::new();
    let circle = scene.circle(0.25).unwrap();
    scene.add(&circle).unwrap();
    let tracker = scene.value_tracker(0.0).unwrap();
    let position = scene
        .position_from_tracker(
            &tracker,
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(-2.0, 0.0, 0.0),
        )
        .unwrap();
    scene.bind_position(&circle, &position).unwrap();

    let mut source = scene.execution_session().unwrap();
    source.begin_replay_retention(Default::default()).unwrap();
    let first = source.wait_segment(1.0).unwrap();
    source.advance_to(first.end_time()).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(first).unwrap();
    let object = reader.frame().objects[0].id;
    reader.seek(0.5).unwrap();
    let paused = reader.frame().clone();
    let publication = reader.publication_context();

    let segment = scene
        .live(&mut source)
        .declare_and_activate_value_tracker(&tracker, 2.0, 2.0, RateFunction::Linear)
        .unwrap();
    {
        let mut live = scene.live(&mut source);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    let batch = source
        .publish_prepared_prefix(segment, &mut publisher)
        .unwrap();
    reader.preflight_append(&batch).unwrap();
    reader.append(batch).unwrap();
    assert_eq!(
        reader.frame(),
        &paused,
        "appending must not move the reader"
    );
    assert_eq!(reader.publication_context(), publication);

    reader.seek(2.0).unwrap();
    assert_eq!(reader.frame().objects[0].id, object);
    assert_eq!(
        reader.frame().objects[0].transform.translation,
        Vec2::new(-1.0, 0.0)
    );
    reader.seek(0.5).unwrap();
    assert_eq!(
        reader.frame().objects[0].transform.translation,
        Vec2::new(-2.0, 0.0)
    );
    reader.seek(2.0).unwrap();
    assert_eq!(
        reader.frame().objects[0].transform.translation,
        Vec2::new(-1.0, 0.0)
    );
    let query = reader.query_viewport(crate::Rect::new(
        Vec2::new(-1.5, -0.5),
        Vec2::new(-0.5, 0.5),
    ));
    assert_eq!(query.object_indices(), &[0]);

    source.seal_replay().unwrap();
    let terminal = source
        .finish_prepared_prefix(segment, &mut publisher)
        .unwrap();
    assert!(terminal.is_terminal());
    reader.append(terminal).unwrap();
    reader.seek(3.0).unwrap();
    assert_eq!(reader.frame().objects[0].id, object);
    assert_eq!(reader.frame().objects[0].transform.translation, Vec2::ZERO);
}

#[test]
fn finite_timeline_alone_does_not_admit_native_state_or_event_playback() {
    for event in [false, true] {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
        if event {
            store
                .bind_semantic_native_event_input(
                    signal,
                    NativeEventSource::KeyPress {
                        code: "Space".into(),
                    },
                )
                .unwrap();
        } else {
            store
                .bind_semantic_native_state_input(
                    signal,
                    NativeStateSource::Control {
                        name: "level".into(),
                    },
                )
                .unwrap();
        }
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.5,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let session = ExecutionSession::from_semantic_store(&store).unwrap();
        assert_eq!(session.finite_timeline_end_time(), Some(0.0));
        assert!(!session.supports_immutable_playback());
        assert_eq!(session.finite_playback_end_time(), None);
        assert_eq!(
            session.source_execution_requirement(),
            Some(super::SourceExecutionRequirement::NativeInput)
        );
    }
}

#[test]
fn finite_playback_denies_external_effective_content_until_its_owner_releases() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let mut session = scene.execution_session().unwrap();
    assert!(session.supports_immutable_playback());
    let prepared = session
        .prepare_effective_content_replacement(
            target.node_id(),
            ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
            None,
            None,
        )
        .unwrap();
    let lease = session
        .commit_effective_content_replacement(prepared)
        .unwrap();
    assert!(!session.supports_immutable_playback());
    assert_eq!(session.finite_playback_end_time(), None);
    session.release_effective_content(lease).unwrap();
    assert_eq!(session.finite_playback_end_time(), Some(0.0));
}

#[test]
fn finite_playback_denies_unrecorded_effective_property_drivers() {
    let (scene, _) = scalar_scene();
    let mut session = scene.execution_session().unwrap();
    let object = session.frame().objects[0].id;
    let prepared = session.runtime.prepare_advance_to(0.0).unwrap();
    let effective = session
        .runtime
        .prepare_effective_property_batch(&[noon_runtime::EffectivePropertyWrite::Transform {
            object,
            transform: Transform2D {
                translation: Vec2::new(9.0, 0.0),
                ..Transform2D::IDENTITY
            },
        }])
        .unwrap();
    session
        .runtime
        .commit_prepared_frame(prepared, effective)
        .unwrap();
    assert!(!session.supports_immutable_playback());
    assert_eq!(session.finite_playback_end_time(), None);
}

#[test]
fn finite_playback_denies_configured_drag_and_retained_history() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .set_translation_drag_targets([target.node_id()])
        .unwrap();
    assert!(!session.translation_drag_active());
    assert_eq!(session.finite_playback_end_time(), None);
    session.set_translation_drag_targets([]).unwrap();
    assert_eq!(session.finite_playback_end_time(), Some(0.0));
    session.begin_replay_retention(Default::default()).unwrap();
    assert_eq!(session.finite_playback_end_time(), None);
    session.seal_replay().unwrap();
    assert_eq!(session.finite_playback_end_time(), None);
}
