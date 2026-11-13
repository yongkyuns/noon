//! Complementary R2 acceptance probes; no alternate scene or rollback state.
use noon::integration::{CallbackAdvance, HostCallbackId, SemanticMutationTransaction};
use noon::{
    AnimationOptions, ExecutionSessionPublicationError, ImageMobjectOptions, RateFunction, Scene,
};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

fn resources(scene: &Scene) -> usize {
    scene
        .integration_store()
        .borrow()
        .raster_image_resources()
        .len()
}
fn image_options() -> ImageMobjectOptions {
    ImageMobjectOptions::rgba8(1, 1, vec![25, 50, 75, 128]).unwrap()
}

#[test]
fn pending_callback_image_creation_is_resource_atomic_and_recovers() -> TestResult {
    let mut scene = Scene::new();
    let object = scene.circle(1.0)?;
    scene.add(&object)?;
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(object.node_id(), HostCallbackId::new(9), 0.0, None);
    callbacks.apply(&mut scene.integration_store().borrow_mut())?;
    let mut execution = scene.into_execution()?;
    let overlay = match execution.session_mut().advance_to_callback_barrier(0.0)? {
        CallbackAdvance::HostRequired { overlay, .. } => overlay,
        CallbackAdvance::Ready(_) => panic!("expected callback barrier"),
    };
    let token = execution
        .session()
        .pending_callback_token()
        .expect("pending callback");
    execution.session_mut().take_frame_changes();
    let before_resources = resources(execution.scene());
    let before_nodes = execution.scene().integration_store().borrow().len();
    let before_revision = execution.scene().revision();
    let before_state = object.state()?;
    let frame = execution.session().frame().clone();
    let publication = execution.session().publication_context();
    let execution_id = execution.session().execution_object_id(object.node_id());

    let error = execution.scene_mut().image(image_options()).unwrap_err();
    assert!(
        matches!(
            &error,
            noon::AuthoringError::ExecutionPublication(
                ExecutionSessionPublicationError::RequiredCallbackPending
            )
        ),
        "unexpected typed error: {error:?}"
    );
    let after_rejection_resources = resources(execution.scene());
    assert_eq!(
        execution.scene().integration_store().borrow().len(),
        before_nodes
    );
    assert_eq!(execution.scene().revision(), before_revision);
    assert_eq!(object.state()?, before_state);
    assert_eq!(execution.session().frame(), &frame);
    assert_eq!(execution.session().publication_context(), publication);
    assert_eq!(execution.session().pending_callback_token(), Some(token));
    assert!(execution.session_mut().take_frame_changes().is_empty());

    execution
        .session_mut()
        .commit_required_callback_phase(overlay.finish())?;
    let recovered = execution.scene_mut().image(image_options())?;
    assert_eq!(
        execution.session().execution_object_id(object.node_id()),
        execution_id
    );
    assert_eq!(
        execution.scene().integration_store().borrow().len(),
        before_nodes + 1
    );
    assert!(execution
        .session()
        .execution_object_id(recovered.node_id())
        .is_none());
    eprintln!("CALLBACK before={before_resources:?} rejected={after_rejection_resources:?} recovered={:?}; typed rejection, frame/token/revision preservation and recovery reached", resources(execution.scene()));
    assert_eq!(
        after_rejection_resources, before_resources,
        "rejected callback-pending creation must not retain a image resource"
    );
    Ok(())
}

#[test]
fn active_segment_image_creation_is_resource_atomic_and_recovers() -> TestResult {
    let mut scene = Scene::new();
    let object = scene.circle(1.0)?;
    let mut target = object.target_editor()?;
    target.set_translation(4.0, 0.0)?;
    scene.add(&object)?;
    let animation = scene.declare_transform_to(
        &object,
        &target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;
    let mut execution = scene.into_execution()?;
    let segment = execution.live().play_animation(&animation)?;
    execution.live().advance_segment_to(segment, 1.0)?;
    execution.session_mut().take_frame_changes();
    let before_resources = resources(execution.scene());
    let before_nodes = execution.scene().integration_store().borrow().len();
    let before_revision = execution.scene().revision();
    let before_state = object.state()?;
    let frame = execution.session().frame().clone();
    let publication = execution.session().publication_context();
    let execution_id = execution.session().execution_object_id(object.node_id());

    let error = execution.scene_mut().image(image_options()).unwrap_err();
    assert!(
        matches!(
            &error,
            noon::AuthoringError::ExecutionPublication(
                ExecutionSessionPublicationError::SegmentCompletionPending
            )
        ),
        "unexpected typed error: {error:?}"
    );
    let after_rejection_resources = resources(execution.scene());
    assert_eq!(
        execution.scene().integration_store().borrow().len(),
        before_nodes
    );
    assert_eq!(execution.scene().revision(), before_revision);
    assert_eq!(object.state()?, before_state);
    assert_eq!(execution.session().frame(), &frame);
    assert_eq!(execution.session().publication_context(), publication);
    assert!(execution.session_mut().take_frame_changes().is_empty());

    {
        let mut live = execution.live();
        live.advance_segment_to(segment, segment.end_time())?;
        live.complete_segment(segment)?;
    }
    let recovered = execution.scene_mut().image(image_options())?;
    assert_eq!(
        execution.session().execution_object_id(object.node_id()),
        execution_id
    );
    assert_eq!(
        execution.scene().integration_store().borrow().len(),
        before_nodes + 1
    );
    assert!(execution
        .session()
        .execution_object_id(recovered.node_id())
        .is_none());
    eprintln!("SEGMENT before={before_resources:?} rejected={after_rejection_resources:?} recovered={:?}; typed rejection, frame/revision preservation and recovery reached", resources(execution.scene()));
    assert_eq!(
        after_rejection_resources, before_resources,
        "rejected segment-pending creation must not retain a image resource"
    );
    Ok(())
}

#[test]
fn invalid_publication_context_rejects_before_resource_import() -> TestResult {
    use noon::{AuthoringError, SceneExecutionError};
    use noon_core::{SemanticNodeCreation, SemanticNodeId};

    let mut scene = Scene::new();
    let object = scene.circle(1.0)?;
    scene.add(&object)?;
    let other = scene.family(&[(&object).into()])?;
    let store = scene.integration_store().clone();
    for invalid_root in [object.node_id(), SemanticNodeId::new(u32::MAX, 1)] {
        assert!(Scene::with_integration_root(store.clone(), invalid_root).is_err());
    }
    for mut wrong in [
        Scene::new(),
        Scene::with_integration_root(store.clone(), other.node_id())?,
    ] {
        let before = (resources(&wrong), wrong.revision());
        assert!(matches!(
            noon::integration::scene_execution::attach(&mut wrong, scene.execution_session()?),
            Err(SceneExecutionError::Publication(_))
        ));
        assert_eq!((resources(&wrong), wrong.revision()), before);
    }

    let mut execution = scene.into_execution()?;
    let mut edit = SemanticMutationTransaction::new();
    edit.add_node(SemanticNodeCreation::family());
    edit.apply(&mut store.borrow_mut())?;
    let before = (
        resources(execution.scene()),
        execution.scene().revision(),
        execution.session().publication_context(),
    );
    assert!(matches!(
        execution.scene_mut().image(image_options()),
        Err(AuthoringError::ExecutionPublication(
            ExecutionSessionPublicationError::StaleSceneRevision { .. }
        ))
    ));
    assert_eq!(
        (
            resources(execution.scene()),
            execution.scene().revision(),
            execution.session().publication_context()
        ),
        before
    );
    Ok(())
}

#[test]
fn ordinary_wait_allows_resource_construction_without_frame_work() -> TestResult {
    let scene = Scene::new();
    let mut execution = scene.into_execution()?;
    let segment = execution.live().wait_segment(1.0)?;
    execution.session_mut().take_frame_changes();
    let object = execution.scene_mut().image(image_options())?;
    assert_eq!(resources(execution.scene()), 1);
    assert!(execution
        .session()
        .execution_object_id(object.node_id())
        .is_none());
    assert!(execution.session_mut().take_frame_changes().is_empty());
    let mut live = execution.live();
    live.advance_segment_to(segment, segment.end_time())?;
    live.complete_segment(segment)?;
    live.add(&object)?;
    Ok(())
}

#[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
#[test]
fn pending_callback_rejects_text_providers_before_import_and_recovers() -> TestResult {
    use noon::{LiveSession, LiveSessionError, MathTypst, Mobject, Text, Typst};
    type Constructor = fn(&mut LiveSession<'_>) -> Result<Mobject, LiveSessionError>;
    let constructors: [Constructor; 3] = [
        |live| live.create_text(Text::new("Noon")),
        |live| live.create_typst(Typst::new("Noon")),
        |live| live.create_math_typst(MathTypst::new("x^2")),
    ];
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
    let token = session.pending_callback_token();
    let before_resources = resources(&scene);
    let before_text = scene.integration_store().borrow().text_resources().len();
    let before_revision = scene.revision();
    let before_nodes = scene.integration_store().borrow().len();
    let frame = session.frame().clone();
    let publication = session.publication_context();
    session.take_frame_changes();
    for create in constructors {
        assert!(matches!(
            create(&mut scene.live(&mut session)),
            Err(LiveSessionError::Publication(
                ExecutionSessionPublicationError::RequiredCallbackPending
            ))
        ));
        assert_eq!(resources(&scene), before_resources);
        assert_eq!(scene.revision(), before_revision);
        assert_eq!(scene.integration_store().borrow().len(), before_nodes);
        assert_eq!(session.frame(), &frame);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.pending_callback_token(), token);
        assert!(session.take_frame_changes().is_empty());
    }
    session.commit_required_callback_phase(overlay.finish())?;
    for create in constructors {
        let object = create(&mut scene.live(&mut session))?;
        assert!(object.state()?.content.text().is_some());
        assert!(session.execution_object_id(object.node_id()).is_none());
    }
    assert_eq!(scene.integration_store().borrow().len(), before_nodes + 3);
    assert_eq!(resources(&scene), before_resources);
    assert_eq!(
        scene.integration_store().borrow().text_resources().len(),
        before_text + 3
    );
    Ok(())
}
