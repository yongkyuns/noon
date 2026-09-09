//! Exercise the public Rust error contract as an external, provider-free consumer.
use std::error::Error;

use noon::{
    AnimationOptions, AuthoringError, CallbackAdvance, ExecutionSession,
    ExecutionSessionPublicationError, HostCallbackId, LiveSessionError, MobjectFamilyMember,
    RateFunction, Scene, SceneRevision, SemanticMutationTransaction, SemanticNodeId,
    SemanticSceneOperationError, SemanticStoreError, Vec2,
};

type TestResult = Result<(), Box<dyn Error>>;

/// Test-only observations, never a second scene or runtime owner.
#[derive(Debug, PartialEq)]
struct AuthoredSnapshot {
    revision: SceneRevision,
    nodes: usize,
    members: Vec<SemanticNodeId>,
    resources: (usize, usize, usize),
}

fn authored_snapshot(scene: &Scene) -> AuthoredSnapshot {
    let store = scene.store().borrow();
    AuthoredSnapshot {
        revision: store.scene_revision(),
        nodes: store.len(),
        members: store.node(scene.root()).unwrap().members().to_vec(),
        resources: (
            store.geometry_resources().len(),
            store.text_resources().len(),
            store.font_resources().len(),
        ),
    }
}

fn assert_live_unchanged(
    scene: &Scene,
    session: &mut ExecutionSession,
    authored: AuthoredSnapshot,
    frame: noon::FrameState,
    publication: noon::PublicationContext,
) {
    assert_eq!(authored_snapshot(scene), authored);
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), publication);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn foreign_handle_in_authored_batch_is_typed_and_atomic() -> TestResult {
    let mut scene = Scene::new();
    let local = scene.circle(1.0)?;
    let foreign = Scene::new().circle(1.0)?;
    // Same slot/generation is not the same identity in a different store.
    assert_eq!(local.node_id(), foreign.node_id());
    let before = authored_snapshot(&scene);
    let error = scene
        .add_many(&[
            MobjectFamilyMember::Mobject(&local),
            MobjectFamilyMember::Mobject(&foreign),
        ])
        .unwrap_err();
    assert_eq!(error, AuthoringError::ForeignStore);
    assert!(error.source().is_none());
    assert_eq!(authored_snapshot(&scene), before);
    scene.add(&local)?;
    assert_eq!(scene.execution_session()?.frame().objects.len(), 1);
    Ok(())
}

#[test]
fn stale_object_and_family_generations_retain_the_rejected_identity() -> TestResult {
    let mut scene = Scene::new();
    let stale = scene.circle(1.0)?;
    scene.store().borrow_mut().remove_node(stale.node_id())?;
    let replacement = scene.square(1.0)?;
    assert_eq!(stale.node_id().slot(), replacement.node_id().slot());
    assert_ne!(
        stale.node_id().generation(),
        replacement.node_id().generation()
    );
    let expected =
        AuthoringError::Semantic(SemanticSceneOperationError::UnknownNode(stale.node_id()));
    assert_eq!(stale.validate(), Err(expected.clone()));
    let before = authored_snapshot(&scene);
    assert_eq!(scene.add(&stale), Err(expected));
    assert_eq!(authored_snapshot(&scene), before);
    scene.add(&replacement)?;

    let stale_family = scene.family(&[])?;
    scene
        .store()
        .borrow_mut()
        .remove_node(stale_family.node_id())?;
    let new_family = scene.family(&[])?;
    assert_eq!(stale_family.node_id().slot(), new_family.node_id().slot());
    assert_ne!(
        stale_family.node_id().generation(),
        new_family.node_id().generation()
    );
    let expected = AuthoringError::Semantic(SemanticSceneOperationError::UnknownNode(
        stale_family.node_id(),
    ));
    assert_eq!(stale_family.validate(), Err(expected.clone()));
    let before = authored_snapshot(&scene);
    assert_eq!(
        scene
            .add_many(&[MobjectFamilyMember::Family(&stale_family)])
            .unwrap_err(),
        expected
    );
    assert_eq!(authored_snapshot(&scene), before);
    Ok(())
}

#[test]
fn authored_membership_preserves_duplicate_missing_and_ambiguous_causes() -> TestResult {
    let mut scene = Scene::new();
    let leaf = scene.circle(1.0)?;
    let missing = scene.square(1.0)?;
    let replacement = scene.rectangle(1.0, 2.0)?;
    let before = authored_snapshot(&scene);
    let error = scene
        .add_many(&[
            MobjectFamilyMember::Mobject(&leaf),
            MobjectFamilyMember::Mobject(&leaf),
        ])
        .unwrap_err();
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<SemanticSceneOperationError>(),
        Some(&SemanticSceneOperationError::DuplicateMembershipTarget(
            leaf.node_id()
        ))
    );
    assert_eq!(authored_snapshot(&scene), before);

    let left = scene.family(&[MobjectFamilyMember::Mobject(&leaf)])?;
    let right = scene.family(&[MobjectFamilyMember::Mobject(&leaf)])?;
    scene.add_many(&[
        MobjectFamilyMember::Family(&left),
        MobjectFamilyMember::Family(&right),
    ])?;
    let before = authored_snapshot(&scene);
    assert_eq!(
        scene
            .replace(
                MobjectFamilyMember::Mobject(&missing),
                MobjectFamilyMember::Mobject(&replacement)
            )
            .unwrap_err(),
        AuthoringError::Semantic(SemanticSceneOperationError::MissingMembershipTarget(
            missing.node_id()
        ))
    );
    assert_eq!(
        scene
            .replace(
                MobjectFamilyMember::Mobject(&leaf),
                MobjectFamilyMember::Mobject(&replacement)
            )
            .unwrap_err(),
        AuthoringError::Semantic(SemanticSceneOperationError::AmbiguousMembershipTarget(
            leaf.node_id()
        ))
    );
    assert_eq!(authored_snapshot(&scene), before);
    scene.clear()?;
    scene.add(&replacement)?;
    assert_eq!(scene.execution_session()?.frame().objects.len(), 1);
    Ok(())
}

#[test]
fn live_membership_retains_semantic_cause_and_recovers_without_partial_work() -> TestResult {
    let mut scene = Scene::new();
    let anchor = scene.circle(1.0)?;
    let next = scene.square(1.0)?;
    scene.add(&anchor)?;
    let mut session = scene.execution_session()?;
    session.take_frame_changes();
    let before = authored_snapshot(&scene);
    let frame = session.frame().clone();
    let publication = session.publication_context();
    let anchor_execution = session.execution_object_id(anchor.node_id());
    let error = scene
        .live(&mut session)
        .add_many(&[
            MobjectFamilyMember::Mobject(&next),
            MobjectFamilyMember::Mobject(&next),
        ])
        .unwrap_err();
    assert!(
        matches!(&error, LiveSessionError::Authoring(AuthoringError::Semantic(
        SemanticSceneOperationError::DuplicateMembershipTarget(id))) if *id == next.node_id())
    );
    assert!(error.source().unwrap().is::<AuthoringError>());
    assert_eq!(
        error
            .source()
            .unwrap()
            .source()
            .unwrap()
            .downcast_ref::<SemanticSceneOperationError>(),
        Some(&SemanticSceneOperationError::DuplicateMembershipTarget(
            next.node_id()
        ))
    );
    assert_live_unchanged(&scene, &mut session, before, frame, publication);
    scene.live(&mut session).add(&next)?;
    assert_eq!(
        session.execution_object_id(anchor.node_id()),
        anchor_execution
    );
    assert_eq!(session.frame().objects.len(), 2);
    Ok(())
}

#[test]
fn live_foreign_handle_and_foreign_runtime_are_distinct_and_atomic() -> TestResult {
    let mut scene = Scene::new();
    let local = scene.circle(1.0)?;
    scene.add(&local)?;
    let mut foreign_scene = Scene::new();
    let foreign = foreign_scene.circle(1.0)?;
    foreign_scene.add(&foreign)?;
    let mut session = scene.execution_session()?;
    session.take_frame_changes();
    let before = authored_snapshot(&scene);
    let frame = session.frame().clone();
    let publication = session.publication_context();
    assert!(matches!(
        scene.live(&mut session).add(&foreign),
        Err(LiveSessionError::ForeignMobjectStore)
    ));
    assert_live_unchanged(&scene, &mut session, before, frame, publication);

    let mut wrong_session = foreign_scene.execution_session()?;
    wrong_session.take_frame_changes();
    let before = authored_snapshot(&scene);
    let frame = wrong_session.frame().clone();
    let publication = wrong_session.publication_context();
    assert!(matches!(
        scene.live(&mut wrong_session).remove(&local),
        Err(LiveSessionError::Publication(
            ExecutionSessionPublicationError::ForeignSemanticStore
        ))
    ));
    assert_live_unchanged(&scene, &mut wrong_session, before, frame, publication);
    Ok(())
}

#[test]
fn live_stale_handle_is_not_reduced_to_a_mobject_string() -> TestResult {
    let mut scene = Scene::new();
    let stale = scene.circle(1.0)?;
    scene.store().borrow_mut().remove_node(stale.node_id())?;
    let valid = scene.square(1.0)?;
    scene.add(&valid)?;
    // Lower the valid revision so this is a handle failure, not a stale-publication failure.
    let mut session = scene.execution_session()?;
    session.take_frame_changes();
    let before = authored_snapshot(&scene);
    let frame = session.frame().clone();
    let publication = session.publication_context();
    for error in [
        scene.live(&mut session).add(&stale).unwrap_err(),
        scene.live(&mut session).effective(&stale).unwrap_err(),
    ] {
        assert!(
            matches!(error, LiveSessionError::Authoring(AuthoringError::Semantic(
            SemanticSceneOperationError::UnknownNode(id))) if id == stale.node_id())
        );
    }
    assert_live_unchanged(&scene, &mut session, before, frame, publication);
    scene.live(&mut session).set_translation(&valid, 2.0, 0.0)?;
    assert_eq!(
        scene
            .live(&mut session)
            .effective(&valid)?
            .transform
            .translation,
        Vec2::new(2.0, 0.0)
    );
    Ok(())
}

#[test]
fn external_authored_edit_keeps_the_stale_publication_category() -> TestResult {
    let mut scene = Scene::new();
    let mut local = scene.circle(1.0)?;
    let detached = scene.square(1.0)?;
    scene.add(&local)?;
    let mut session = scene.execution_session()?;
    session.take_frame_changes();
    let frame = session.frame().clone();
    let publication = session.publication_context();
    let expected = scene.store().borrow().scene_revision();
    local.set_translation(3.0, 0.0)?;
    let actual = scene.store().borrow().scene_revision();
    let before = authored_snapshot(&scene);
    let error = scene.live(&mut session).add(&detached).unwrap_err();
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<ExecutionSessionPublicationError>(),
        Some(&ExecutionSessionPublicationError::StaleSceneRevision { expected, actual })
    );
    assert_live_unchanged(&scene, &mut session, before, frame, publication);
    // Explicitly lower the new authored revision; do not bypass the stale check.
    let mut replacement_session = scene.execution_session()?;
    scene.live(&mut replacement_session).add(&detached)?;
    assert_eq!(
        scene
            .live(&mut replacement_session)
            .effective(&local)?
            .transform
            .translation
            .x,
        3.0
    );
    Ok(())
}

#[test]
fn pending_segment_rejects_membership_then_accepts_it_after_logical_completion() -> TestResult {
    let mut scene = Scene::new();
    let local = scene.circle(1.0)?;
    let detached = scene.square(1.0)?;
    let mut target = local.target_editor()?;
    target.set_translation(4.0, 0.0)?;
    scene.add(&local)?;
    let animation = scene.declare_transform_to(
        &local,
        &target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;
    let mut session = scene.execution_session()?;
    let segment = scene.live(&mut session).play_animation(&animation)?;
    scene.live(&mut session).advance_segment_to(segment, 1.0)?;
    session.take_frame_changes();
    let before = authored_snapshot(&scene);
    let frame = session.frame().clone();
    let publication = session.publication_context();
    assert!(matches!(
        scene.live(&mut session).add(&detached),
        Err(LiveSessionError::Publication(
            ExecutionSessionPublicationError::SegmentCompletionPending
        ))
    ));
    assert_live_unchanged(&scene, &mut session, before, frame, publication);
    assert_eq!(
        scene
            .live(&mut session)
            .authored(&local)?
            .transform
            .translation
            .x,
        0.0
    );
    assert_eq!(
        scene
            .live(&mut session)
            .effective(&local)?
            .transform
            .translation
            .x,
        2.0
    );
    let mut live = scene.live(&mut session);
    live.advance_segment_to(segment, segment.end_time())?;
    live.complete_segment(segment)?;
    live.add(&detached)?;
    assert_eq!(live.authored(&local)?.transform.translation.x, 4.0);
    assert_eq!(live.effective(&local)?.transform.translation.x, 4.0);
    Ok(())
}

#[test]
fn pending_callback_preserves_publication_and_can_resume_after_rejection() -> TestResult {
    let mut scene = Scene::new();
    let local = scene.circle(1.0)?;
    let detached = scene.square(1.0)?;
    scene.add(&local)?;
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(local.node_id(), HostCallbackId::new(9), 0.0, None);
    callbacks.apply(&mut scene.store().borrow_mut())?;
    let mut session = scene.execution_session()?;
    let overlay = match session.advance_to_callback_barrier(0.0)? {
        CallbackAdvance::HostRequired { overlay, .. } => overlay,
        CallbackAdvance::Ready(_) => panic!("expected ordered callback barrier"),
    };
    session.take_frame_changes();
    let before = authored_snapshot(&scene);
    let frame = session.frame().clone();
    let publication = session.publication_context();
    assert!(matches!(
        scene.live(&mut session).add(&detached),
        Err(LiveSessionError::Publication(
            ExecutionSessionPublicationError::RequiredCallbackPending
        ))
    ));
    assert_live_unchanged(&scene, &mut session, before, frame, publication);
    session.commit_required_callback_phase(overlay.finish())?;
    scene.live(&mut session).add(&detached)?;
    assert_eq!(session.frame().objects.len(), 2);
    Ok(())
}

#[test]
fn semantic_store_error_remains_an_inspectable_cause() {
    let root = Scene::new().root();
    let cause = SemanticStoreError::NotFamily(root);
    let error = AuthoringError::from(SemanticSceneOperationError::Store(cause.clone()));
    assert_eq!(
        error
            .source()
            .unwrap()
            .source()
            .unwrap()
            .downcast_ref::<SemanticStoreError>(),
        Some(&cause)
    );
}

#[test]
fn invalid_live_batch_preserves_transaction_cause_and_rolls_back_prior_writes() -> TestResult {
    use noon::{SemanticMutationTransactionError, SemanticObjectProperty, SemanticVec3};
    let mut scene = Scene::new();
    let local = scene.circle(1.0)?;
    scene.add(&local)?;
    let mut session = scene.execution_session()?;
    session.take_frame_changes();
    let before = authored_snapshot(&scene);
    let state = local.state()?;
    let frame = session.frame().clone();
    let publication = session.publication_context();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(
        local.node_id(),
        SemanticObjectProperty::Translation,
        SemanticVec3::new(5.0, 0.0, 0.0),
    );
    transaction.set_property(
        local.node_id(),
        SemanticObjectProperty::StrokeWidth,
        f64::NAN,
    );
    let error = scene.live(&mut session).apply(transaction).unwrap_err();
    let cause = SemanticMutationTransactionError::NonFinitePropertyValue {
        index: 1,
        object: local.node_id(),
        property: SemanticObjectProperty::StrokeWidth,
    };
    assert_eq!(
        error
            .source()
            .unwrap()
            .source()
            .unwrap()
            .downcast_ref::<SemanticMutationTransactionError>(),
        Some(&cause)
    );
    assert!(matches!(
        error,
        LiveSessionError::Publication(ExecutionSessionPublicationError::Semantic(_))
    ));
    assert_eq!(local.state()?, state);
    assert_live_unchanged(&scene, &mut session, before, frame, publication);
    scene.live(&mut session).set_translation(&local, 1.0, 0.0)?;
    Ok(())
}
