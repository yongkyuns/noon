//! Public, provider-free object/value/layout/state error and recovery contracts.
use std::{error::Error, rc::Rc};

use noon::integration::{
    CallbackAdvance, GeometryResourceArena, GeometryResourceError, HostCallbackId,
    SemanticMutationTransaction, SemanticMutationTransactionError, SemanticSignalBindingError,
    SemanticSignalError,
};
use noon::{
    AnimationOptions, AuthoringError, ExecutionSession, ExecutionSessionCallbackError, LayoutAnchor,
    LiveSessionError, Mobject, MobjectFamilyMember, RateFunction, Scene, SceneRevision,
    SemanticNodeId, SemanticObjectProperty, SemanticObjectState, SemanticVec3, StoredGeometry,
    UnsupportedAuthoringOperation, Vec2, VectorPath,
};

type TestResult = Result<(), Box<dyn Error>>;

/// Observations for assertions only; no second scene or runtime is constructed.
#[derive(Debug, PartialEq)]
struct AuthoredSnapshot {
    revision: SceneRevision,
    nodes: usize,
    members: Vec<SemanticNodeId>,
    resources: (usize, usize, usize),
    objects: Vec<SemanticObjectState>,
}

fn snapshot(scene: &Scene, objects: &[&Mobject]) -> AuthoredSnapshot {
    let store = scene.integration_store().borrow();
    AuthoredSnapshot {
        revision: store.scene_revision(),
        nodes: store.len(),
        members: store.node(scene.root()).unwrap().members().to_vec(),
        resources: (store.geometry_resources().len(), store.text_resources().len(), store.font_resources().len()),
        objects: objects.iter().map(|object| object.state().unwrap()).collect(),
    }
}

fn assert_live_unchanged(
    scene: &Scene,
    session: &mut ExecutionSession,
    objects: &[&Mobject],
    before: AuthoredSnapshot,
    frame: noon::integration::FrameState,
    publication: noon::PublicationContext,
) {
    assert_eq!(snapshot(scene, objects), before);
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), publication);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn constructor_and_resource_rejections_do_not_allocate_and_recover() -> TestResult {
    let scene = Scene::new();
    let before = snapshot(&scene, &[]);
    assert!(matches!(scene.circle(0.0), Err(AuthoringError::NonPositiveNumber { value: 0.0, .. })));
    assert!(matches!(scene.rectangle(1.0, f64::INFINITY), Err(AuthoringError::InvalidRenderNumber { .. })));
    let invalid = VectorPath::new().move_to(Vec2::new(f32::INFINITY, 0.0));
    assert_eq!(scene.path(invalid.clone()).unwrap_err(), AuthoringError::NonFiniteGeometry);
    assert_eq!(scene.integration_store().borrow_mut().insert_geometry_path(invalid).unwrap_err(), GeometryResourceError::NonFinitePath);
    assert_eq!(snapshot(&scene, &[]), before);

    let path = VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::new(1.0, 1.0));
    let mut other_resources = GeometryResourceArena::new();
    let missing = other_resources.insert_path(path.clone());
    let error = Mobject::new(Rc::clone(scene.integration_store()), SemanticObjectState::new(StoredGeometry::Resource(missing))).unwrap_err();
    assert_eq!(error, AuthoringError::MissingGeometryResource(missing));
    assert_eq!(snapshot(&scene, &[]), before);
    scene.path(path)?;
    scene.circle(1.0)?;
    assert_eq!(scene.integration_store().borrow().geometry_resources().len(), 1);
    Ok(())
}

#[test]
fn style_transform_and_query_categories_are_typed_and_atomic() -> TestResult {
    let scene = Scene::new();
    let mut object = scene.circle(1.0)?;
    let before = snapshot(&scene, &[&object]);
    assert!(matches!(object.set_fill_opacity(1.5), Err(AuthoringError::InvalidOpacity { value: 1.5, .. })));
    assert_eq!(object.set_stroke_width_mode("invalid"), Err(AuthoringError::InvalidStrokeWidthMode("invalid".into())));
    assert!(matches!(object.set_rotation(f64::INFINITY), Err(AuthoringError::InvalidRenderNumber { .. })));
    let error = object.manim_line_endpoints().unwrap_err();
    assert_eq!(error, AuthoringError::Unsupported(UnsupportedAuthoringOperation::LineEndpointContent));
    assert!(error.source().unwrap().is::<UnsupportedAuthoringOperation>());
    assert_eq!(snapshot(&scene, &[&object]), before);
    object.set_fill_opacity(0.5)?;
    object.set_rotation(0.25)?;
    assert_eq!(object.fill_opacity()?, 0.5);
    Ok(())
}

#[test]
fn scalar_failures_preserve_domain_and_transaction_causes() -> TestResult {
    let scene = Scene::new();
    let before = snapshot(&scene, &[]);
    let error = scene.value_tracker(f64::NAN).unwrap_err();
    assert_eq!(error, AuthoringError::Signal(SemanticSignalError::NonFiniteValue));
    assert_eq!(error.source().unwrap().downcast_ref::<SemanticSignalError>(), Some(&SemanticSignalError::NonFiniteValue));
    assert_eq!(snapshot(&scene, &[]), before);
    let tracker = scene.value_tracker(1.0)?;
    let before = snapshot(&scene, &[]);
    let error = scene.set_value(&tracker, f64::NAN).unwrap_err();
    assert!(matches!(error, AuthoringError::Transaction(_)));
    assert!(error.source().unwrap().is::<SemanticMutationTransactionError>());
    assert_eq!(error.source().unwrap().source().unwrap().downcast_ref::<SemanticSignalError>(), Some(&SemanticSignalError::NonFiniteValue));
    assert_eq!(snapshot(&scene, &[]), before);
    scene.set_value(&tracker, 2.0)?;
    assert_eq!(scene.value_tracker_value(&tracker)?, 2.0);
    assert_eq!(tracker.set_detached_value(3.0), Err(AuthoringError::AlreadyScopedTracker(tracker.node_id())));
    let before = snapshot(&scene, &[]);
    assert_eq!(scene.key_state_signal("  ", false).unwrap_err(), AuthoringError::EmptyInputName { kind: "key code".into() });
    assert_eq!(snapshot(&scene, &[]), before);
    scene.key_state_signal("Space", false)?;
    Ok(())
}

#[test]
fn duplicate_binding_preserves_existing_signal_and_same_binding_is_a_noop() -> TestResult {
    let scene = Scene::new();
    let object = scene.circle(1.0)?;
    let tracker = scene.value_tracker(1.0)?;
    let first = scene.position_from_tracker(&tracker, SemanticVec3::new(1.0, 0.0, 0.0), SemanticVec3::ZERO)?;
    let second = scene.position_from_tracker(&tracker, SemanticVec3::new(0.0, 1.0, 0.0), SemanticVec3::ZERO)?;
    scene.bind_position(&object, &first)?;
    let before = snapshot(&scene, &[&object]);
    let cause = SemanticSignalBindingError::PropertyAlreadyBound {
        target: object.node_id(), property: SemanticObjectProperty::Translation, existing_signal: first.node_id(),
    };
    let error = scene.bind_position(&object, &second).unwrap_err();
    assert_eq!(error, AuthoringError::SignalBinding(cause.clone()));
    assert_eq!(error.source().unwrap().downcast_ref::<SemanticSignalBindingError>(), Some(&cause));
    assert_eq!(snapshot(&scene, &[&object]), before);
    scene.bind_position(&object, &first)?;
    assert_eq!(snapshot(&scene, &[&object]), before);
    scene.set_value(&tracker, 2.0)?;
    Ok(())
}

#[test]
fn layout_and_copy_failures_leave_all_family_leaves_unchanged() -> TestResult {
    let scene = Scene::new();
    let left = scene.circle(1.0)?;
    let right = scene.square(1.0)?;
    let family = scene.family(&[MobjectFamilyMember::Mobject(&left), MobjectFamilyMember::Mobject(&right)])?;
    let before = snapshot(&scene, &[&left, &right]);
    assert_eq!(LayoutAnchor::from(&family).member(-3).layout().unwrap_err(), AuthoringError::InvalidSubmobjectIndex { family: family.node_id(), index: -3 });
    assert_eq!(family.arrange(0.0, 0.0, 0.1, true).unwrap_err(), AuthoringError::ZeroDirection);
    assert!(matches!(family.arrange_in_grid(Some(1), Some(1), 0.1, 0.1), Err(AuthoringError::InsufficientGridCapacity { .. })));
    let foreign = Scene::new().circle(1.0)?;
    assert_eq!(family.copy_with_references(&[MobjectFamilyMember::Mobject(&foreign)]).unwrap_err(), AuthoringError::ForeignStore);
    assert_eq!(snapshot(&scene, &[&left, &right]), before);
    family.arrange(1.0, 0.0, 0.1, true)?;
    let copy = family.copy_family()?;
    assert_ne!(copy.mobject(&left)?.node_id(), left.node_id());
    assert_eq!(copy.mobject(&left)?.state()?, left.state()?);
    Ok(())
}

#[test]
fn pending_callback_capture_keeps_token_and_recovers_without_rebuilding() -> TestResult {
    let mut scene = Scene::new();
    let object = scene.circle(1.0)?;
    let family = scene.family(&[MobjectFamilyMember::Mobject(&object)])?;
    scene.add_many(&[MobjectFamilyMember::Family(&family)])?;
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(object.node_id(), HostCallbackId::new(9), 0.0, None);
    callbacks.apply(&mut scene.integration_store().borrow_mut())?;
    let mut session = scene.execution_session()?;
    let overlay = match session.advance_to_callback_barrier(0.0)? {
        CallbackAdvance::HostRequired { overlay, .. } => overlay,
        CallbackAdvance::Ready(_) => panic!("expected callback barrier"),
    };
    let token = session.pending_callback_token().unwrap();
    session.take_frame_changes();
    let before = snapshot(&scene, &[&object]);
    let frame = session.frame().clone();
    let publication = session.publication_context();
    let execution_id = session.execution_object_id(object.node_id());
    for error in [scene.live(&mut session).target_editor(&object).unwrap_err(), scene.live(&mut session).copy_family(&family).unwrap_err()] {
        assert!(matches!(&error, LiveSessionError::Callback(ExecutionSessionCallbackError::Pending(found)) if *found == token));
        assert!(error.source().unwrap().is::<ExecutionSessionCallbackError>());
    }
    assert_live_unchanged(&scene, &mut session, &[&object], before, frame, publication);
    session.commit_required_callback_phase(overlay.finish())?;
    scene.live(&mut session).target_editor(&object)?;
    scene.live(&mut session).copy_family(&family)?;
    assert_eq!(session.execution_object_id(object.node_id()), execution_id);
    Ok(())
}

#[test]
fn live_family_callback_failure_preserves_effective_and_authored_separation() -> TestResult {
    let mut scene = Scene::new();
    let object = scene.circle(1.0)?;
    let unaffected = scene.square(1.0)?;
    let family = scene.family(&[MobjectFamilyMember::Mobject(&object)])?;
    let mut target = object.target_editor()?;
    target.set_translation(4.0, 0.0)?;
    scene.add_many(&[MobjectFamilyMember::Family(&family)])?;
    scene.add(&unaffected)?;
    let animation = scene.declare_transform_to(&object, &target, AnimationOptions::new().run_time(2.0).rate_func(RateFunction::Linear))?;
    let mut session = scene.execution_session()?;
    let segment = scene.live(&mut session).play_animation(&animation)?;
    scene.live(&mut session).advance_segment_to(segment, 1.0)?;
    session.take_frame_changes();
    let before = snapshot(&scene, &[&object, &unaffected]);
    let frame = session.frame().clone();
    let publication = session.publication_context();
    let unaffected_id = session.execution_object_id(unaffected.node_id());
    let error = scene.live(&mut session).arrange_family(&family, 1.0, 0.0, 0.1, true).unwrap_err();
    assert!(matches!(error, LiveSessionError::Authoring(AuthoringError::Unsupported(UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver))));
    assert!(error.source().unwrap().source().unwrap().is::<UnsupportedAuthoringOperation>());
    assert_live_unchanged(&scene, &mut session, &[&object, &unaffected], before, frame, publication);
    assert_eq!(scene.live(&mut session).authored(&object)?.transform.translation.x, 0.0);
    assert_eq!(scene.live(&mut session).effective(&object)?.transform.translation.x, 2.0);
    let mut live = scene.live(&mut session);
    live.advance_segment_to(segment, segment.end_time())?;
    live.complete_segment(segment)?;
    live.arrange_family(&family, 1.0, 0.0, 0.1, true)?;
    assert_eq!(session.execution_object_id(unaffected.node_id()), unaffected_id);
    Ok(())
}

#[test]
fn camera_rejection_does_not_change_the_root_and_recovers_after_clear() -> TestResult {
    let mut scene = Scene::new();
    let object = scene.circle(1.0)?;
    scene.add(&object)?;
    let before = snapshot(&scene, &[&object]);
    assert_eq!(scene.camera_frame().unwrap_err(), AuthoringError::CameraRequiresEmptyScene(scene.root()));
    assert_eq!(snapshot(&scene, &[&object]), before);
    scene.clear()?;
    scene.camera_frame()?;
    scene.add(&object)?;
    scene.execution_session()?;
    Ok(())
}
