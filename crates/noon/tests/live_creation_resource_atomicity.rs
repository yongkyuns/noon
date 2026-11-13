//! Complementary R2 acceptance probes; no alternate scene or rollback state.
use noon::integration::{CallbackAdvance, HostCallbackId, SemanticMutationTransaction};
use noon::{
    AnimationOptions, ExecutionSessionPublicationError, ManimGeometryOptions, RateFunction, Scene,
    Vec2, VectorPath,
};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

fn resources(scene: &Scene) -> (usize, usize, usize) {
    let store = scene.integration_store().borrow();
    (
        store.geometry_resources().len(),
        store.text_resources().len(),
        store.font_resources().len(),
    )
}

fn path_options() -> ManimGeometryOptions {
    ManimGeometryOptions::path(
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(0.7, 0.4)),
    )
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

    let error = execution.scene_mut().geometry(path_options()).unwrap_err();
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
    let recovered = execution.scene_mut().geometry(path_options())?;
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
        "rejected callback-pending creation must not retain a geometry resource"
    );
    Ok(())
}

#[test]
fn active_segment_path_creation_is_resource_atomic_and_recovers() -> TestResult {
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

    let error = execution.scene_mut().geometry(path_options()).unwrap_err();
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
    let recovered = execution.scene_mut().geometry(path_options())?;
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
        "rejected segment-pending creation must not retain a geometry resource"
    );
    Ok(())
}

#[test]
fn stale_geometry_publication_rejects_before_import() -> TestResult {
    let mut execution = Scene::new().into_execution()?;
    let store = execution.scene().integration_store().clone();
    let mut external = SemanticMutationTransaction::new();
    external.add_node(noon_core::SemanticNodeCreation::family());
    external.apply(&mut store.borrow_mut())?;
    let before = (
        resources(execution.scene()),
        execution.scene().revision(),
        execution.session().publication_context(),
    );
    assert!(matches!(
        execution.scene_mut().geometry(path_options()),
        Err(noon::AuthoringError::ExecutionPublication(
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
    let object = execution.scene_mut().geometry(path_options())?;
    assert_eq!(resources(execution.scene()), (1, 0, 0));
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
    use noon::{MathTypst, Mobject, Text, TextAuthoringError, Typst};
    type Constructor = fn(&mut Scene) -> Result<Mobject, TextAuthoringError>;
    let constructors: [Constructor; 3] = [
        |scene| scene.text(Text::new("Noon")),
        |scene| scene.typst(Typst::new("Noon")),
        |scene| scene.math_typst(MathTypst::new("x^2")),
    ];
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
    let token = execution.session().pending_callback_token();
    let before_resources = resources(execution.scene());
    let before_revision = execution.scene().revision();
    let before_nodes = execution.scene().integration_store().borrow().len();
    let frame = execution.session().frame().clone();
    let publication = execution.session().publication_context();
    execution.session_mut().take_frame_changes();
    for create in constructors {
        assert!(matches!(
            create(execution.scene_mut()),
            Err(TextAuthoringError::Semantic(
                noon::AuthoringError::ExecutionPublication(
                    ExecutionSessionPublicationError::RequiredCallbackPending
                )
            ))
        ));
        assert_eq!(resources(execution.scene()), before_resources);
        assert_eq!(execution.scene().revision(), before_revision);
        assert_eq!(
            execution.scene().integration_store().borrow().len(),
            before_nodes
        );
        assert_eq!(execution.session().frame(), &frame);
        assert_eq!(execution.session().publication_context(), publication);
        assert_eq!(execution.session().pending_callback_token(), token);
        assert!(execution.session_mut().take_frame_changes().is_empty());
    }
    execution
        .session_mut()
        .commit_required_callback_phase(overlay.finish())?;
    for create in constructors {
        let object = create(execution.scene_mut())?;
        assert!(object.state()?.content.text().is_some());
        assert!(execution
            .session()
            .execution_object_id(object.node_id())
            .is_none());
    }
    assert_eq!(
        execution.scene().integration_store().borrow().len(),
        before_nodes + 3
    );
    assert_eq!(resources(execution.scene()).1, before_resources.1 + 3);
    Ok(())
}
