//! Compile and exercise the public facade as an independent consumer of only noon.
//! Provider-feature CI runs these tests natively and compiles them for WASM.
use noon::{
    AnimationOptions, ExecutionSessionPublicationError, LiveSessionError, ManimGeometryOptions,
    MobjectTarget, RateFunction, Scene, Vec2,
};

#[test]
fn public_authoring_live_queries_and_completion() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let before = scene.revision();
    let circle = scene.geometry(ManimGeometryOptions::circle(1.0)?)?;
    assert!(scene.revision().get() > before.get());
    scene.add(&circle)?;
    let mut running = scene.into_execution()?;
    let target = running.scene_mut().target_editor(&circle)?;
    running.live().shift(&target, 4.0, -2.0)?;
    let segment = running.live().declare_and_activate_transform_to(
        &circle,
        &target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;
    running.live().advance_segment_to(segment, 1.0)?;
    let halfway = running.scene().effective(&circle)?;
    assert_eq!(halfway.transform.translation, Vec2::new(2.0, -1.0));
    assert_eq!(
        running.scene().authored(&circle)?.transform.translation.x,
        0.0
    );
    running
        .live()
        .advance_segment_to(segment, segment.end_time())?;
    assert!(!running.live().segment_state(segment).is_complete());
    running.live().complete_segment(segment)?;
    assert!(running.live().segment_state(segment).is_complete());
    assert_eq!(
        running.scene().authored(&circle)?.transform.translation.x,
        4.0
    );
    running.live().shift(&circle, 1.0, 0.0)?;
    let after = running.scene().effective(&circle)?;
    assert_eq!(after.transform.translation, Vec2::new(5.0, -2.0));
    assert_eq!(
        after.publication.scene_revision(),
        running.scene().revision()
    );
    assert_eq!(halfway.transform.translation, Vec2::new(2.0, -1.0));
    let added = running
        .scene_mut()
        .geometry(ManimGeometryOptions::square(0.5)?)?;
    running.scene_mut().add(&added)?;
    assert!(running.live().contains(&added)?);
    let wait = running.live().wait_segment(0.25)?;
    running.live().advance_segment_to(wait, wait.end_time())?;
    running.live().complete_segment(wait)?;
    assert!(running.live().segment_state(wait).is_complete());
    Ok(())
}

#[test]
fn integration_access_keeps_one_arena_and_stale_publication_protection(
) -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{SemanticMutationTransaction, SemanticStore, TextResource};
    use std::{cell::RefCell, rc::Rc};
    // Shared semantic text/resource contracts remain available without providers.
    let _: Option<TextResource> = None;
    let arena = Rc::new(RefCell::new(SemanticStore::new()));
    let mut scene = Scene::with_integration_store(Rc::clone(&arena));
    let circle = scene.circle(1.0)?;
    let family = scene.family(&[MobjectTarget::Object(&circle)])?;
    assert!(Rc::ptr_eq(scene.integration_store(), &arena));
    assert!(Rc::ptr_eq(circle.integration_store(), &arena));
    assert!(Rc::ptr_eq(family.integration_store(), &arena));
    scene.add(&circle)?;
    assert!(arena.borrow().text_resources().is_empty());
    let mut session = scene.execution_session()?;
    session.take_frame_changes();
    let published = session.publication_context();
    let original_transform = session.frame().objects[0].transform;
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(
        circle.node_id(),
        noon::SemanticObjectProperty::Translation,
        noon::SemanticVec3::new(9.0, 0.0, 0.0),
    );
    transaction.apply(&mut scene.integration_store().borrow_mut())?;
    let raw_revision = scene.revision();
    assert_ne!(raw_revision, published.scene_revision());
    {
        let mut live = scene.live(&mut session);
        let error = live.shift(&circle, 1.0, 0.0).unwrap_err();
        assert!(matches!(error, LiveSessionError::Publication(
            ExecutionSessionPublicationError::StaleSceneRevision { expected, actual }
        ) if expected == published.scene_revision() && actual == raw_revision));
        assert!(matches!(
            live.effective(&circle),
            Err(LiveSessionError::Publication(
                ExecutionSessionPublicationError::StaleSceneRevision { .. }
            ))
        ));
    }
    assert_eq!(circle.state()?.transform.translation.x, 9.0);
    assert_eq!(scene.revision(), raw_revision);
    assert_eq!(session.publication_context(), published);
    assert_eq!(session.frame().objects[0].transform, original_transform);
    assert!(session.take_frame_changes().is_empty());
    Ok(())
}
