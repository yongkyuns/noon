//! Complementary R2 acceptance probes; no alternate scene or rollback state.
use std::error::Error;
use noon::integration::{CallbackAdvance, HostCallbackId, SemanticMutationTransaction};
use noon::{
    AnimationOptions, ExecutionSessionPublicationError, LiveSessionError,
    ManimGeometryOptions, RateFunction, Scene, Vec2, VectorPath,
};

type TestResult = Result<(), Box<dyn Error>>;

fn resources(scene: &Scene) -> (usize, usize, usize) {
    let store = scene.integration_store().borrow();
    (store.geometry_resources().len(), store.text_resources().len(), store.font_resources().len())
}

fn path_options() -> ManimGeometryOptions {
    ManimGeometryOptions::path(VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(0.7, 0.4)))
        .expect("finite nonempty path")
}

#[test]
fn pending_callback_path_creation_is_resource_atomic_and_recovers() -> TestResult {
    let mut scene = Scene::new();
    let object = scene.circle(1.0)?;
    scene.add(&object)?;
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(object.node_id(), HostCallbackId::new(9), 0.0, None);
    callbacks.apply(&mut scene.integration_store().borrow_mut())?;
    let mut session = scene.execution_session()?;
    let overlay = match session.advance_to_callback_barrier(0.0)? {
        CallbackAdvance::HostRequired { overlay, .. } => overlay,
        CallbackAdvance::Ready(_) => panic!("expected callback barrier"),
    };
    let token = session.pending_callback_token().expect("pending callback");
    session.take_frame_changes();
    let before_resources = resources(&scene);
    let before_nodes = scene.integration_store().borrow().len();
    let before_revision = scene.revision();
    let before_state = object.state()?;
    let frame = session.frame().clone();
    let publication = session.publication_context();
    let execution_id = session.execution_object_id(object.node_id());

    let error = scene.live(&mut session).create_manim_geometry(path_options()).unwrap_err();
    assert!(matches!(&error, LiveSessionError::Publication(
        ExecutionSessionPublicationError::RequiredCallbackPending
    )), "unexpected typed error: {error:?}");
    let after_rejection_resources = resources(&scene);
    assert_eq!(scene.integration_store().borrow().len(), before_nodes);
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(object.state()?, before_state);
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.pending_callback_token(), Some(token));
    assert!(session.take_frame_changes().is_empty());

    session.commit_required_callback_phase(overlay.finish())?;
    let recovered = scene.live(&mut session).create_manim_geometry(path_options())?;
    assert_eq!(session.execution_object_id(object.node_id()), execution_id);
    assert_eq!(scene.integration_store().borrow().len(), before_nodes + 1);
    assert!(session.execution_object_id(recovered.node_id()).is_none());
    eprintln!("CALLBACK before={before_resources:?} rejected={after_rejection_resources:?} recovered={:?}; typed rejection, frame/token/revision preservation and recovery reached", resources(&scene));
    assert_eq!(after_rejection_resources, before_resources,
        "rejected callback-pending creation must not retain a geometry resource");
    Ok(())
}

#[test]
fn active_segment_path_creation_is_resource_atomic_and_recovers() -> TestResult {
    let mut scene = Scene::new();
    let object = scene.circle(1.0)?;
    let mut target = object.target_editor()?;
    target.set_translation(4.0, 0.0)?;
    scene.add(&object)?;
    let animation = scene.declare_transform_to(&object, &target,
        AnimationOptions::new().run_time(2.0).rate_func(RateFunction::Linear))?;
    let mut session = scene.execution_session()?;
    let segment = scene.live(&mut session).play_animation(&animation)?;
    scene.live(&mut session).advance_segment_to(segment, 1.0)?;
    session.take_frame_changes();
    let before_resources = resources(&scene);
    let before_nodes = scene.integration_store().borrow().len();
    let before_revision = scene.revision();
    let before_state = object.state()?;
    let frame = session.frame().clone();
    let publication = session.publication_context();
    let execution_id = session.execution_object_id(object.node_id());

    let error = scene.live(&mut session).create_manim_geometry(path_options()).unwrap_err();
    assert!(matches!(&error, LiveSessionError::Publication(
        ExecutionSessionPublicationError::SegmentCompletionPending
    )), "unexpected typed error: {error:?}");
    let after_rejection_resources = resources(&scene);
    assert_eq!(scene.integration_store().borrow().len(), before_nodes);
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(object.state()?, before_state);
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), publication);
    assert!(session.take_frame_changes().is_empty());

    {
        let mut live = scene.live(&mut session);
        live.advance_segment_to(segment, segment.end_time())?;
        live.complete_segment(segment)?;
    }
    let recovered = scene.live(&mut session).create_manim_geometry(path_options())?;
    assert_eq!(session.execution_object_id(object.node_id()), execution_id);
    assert_eq!(scene.integration_store().borrow().len(), before_nodes + 1);
    assert!(session.execution_object_id(recovered.node_id()).is_none());
    eprintln!("SEGMENT before={before_resources:?} rejected={after_rejection_resources:?} recovered={:?}; typed rejection, frame/revision preservation and recovery reached", resources(&scene));
    assert_eq!(after_rejection_resources, before_resources,
        "rejected segment-pending creation must not retain a geometry resource");
    Ok(())
}
