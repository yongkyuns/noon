//! Shared declaration publication, not the unfinished foreground ordering facade.
use noon::{ExecutionSessionPublicationError, Scene};
use noon_core::{
    SemanticMutationImpact, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticObjectProperty, SemanticSignalValue, SemanticVec3,
};

#[test]
fn metadata_only_live_publication_does_not_lower_or_dirty_any_object() {
    let mut scene = Scene::new();
    let target = scene.square(0.5).unwrap();
    for _ in 0..1_000 {
        let unrelated = scene.square(0.25).unwrap();
        scene.add(&unrelated).unwrap();
    }
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let objects = session.frame().objects.clone();
    let painter_order = session.painter_order().to_vec();
    let time = session.frame().time;
    let before = session.publication_context();
    let members = scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_foreground_members(scene.root(), [target.node_id()]);
    let result = scene.live(&mut session).apply(tx).unwrap();
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::ForegroundMembers {
            scope: scene.root()
        }]
    );
    assert_eq!(
        scene.revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(
        session.publication_context().scene_revision(),
        scene.revision()
    );
    assert_eq!(
        session.publication_context().execution_revision(),
        before.execution_revision()
    );
    assert_eq!(session.frame().objects, objects);
    assert_eq!(session.frame().time, time);
    assert_eq!(session.painter_order(), painter_order);
    assert!(session.take_frame_changes().is_empty());
    let stats = session.last_structural_publication_stats();
    assert_eq!(stats.preparation.object_states_lowered, 0);
    assert_eq!(stats.preparation.possible_entries, 0);
    assert_eq!(stats.preparation.possible_exits, 0);
    assert_eq!(stats.entered_objects, 0);
    assert_eq!(stats.exited_objects, 0);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .last_mutation_stats()
            .slots_written,
        1
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members(),
        members
    );
    assert!(target.state().is_ok());
    // The same declaration publishes no new semantic or runtime revision.
    let unchanged = session.publication_context();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_foreground_members(scene.root(), [target.node_id()]);
    assert!(scene
        .live(&mut session)
        .apply(tx)
        .unwrap()
        .impacts()
        .is_empty());
    assert_eq!(session.publication_context(), unchanged);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn failed_runtime_preflight_keeps_metadata_properties_and_publication_unchanged() {
    let mut scene = Scene::new();
    let target = scene.square(1.0).unwrap();
    scene.add(&target).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let frame = session.frame().clone();
    let authored = target.state().unwrap();
    let context = session.publication_context();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_foreground_members(scene.root(), [target.node_id()]);
    tx.set_property(
        target.node_id(),
        SemanticObjectProperty::Translation,
        SemanticSignalValue::Vec3(SemanticVec3::new(f64::MAX, 0.0, 0.0)),
    );
    assert!(scene.live(&mut session).apply(tx).is_err());
    assert_eq!(scene.revision(), context.scene_revision());
    assert_eq!(session.publication_context(), context);
    assert_eq!(target.state().unwrap(), authored);
    assert_eq!(session.frame(), &frame);
    assert!(session.take_frame_changes().is_empty());
    assert!(scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .foreground_members()
        .is_empty());
}

#[test]
fn deleting_detached_foreground_target_cleans_metadata_without_frame_work() {
    let mut scene = Scene::new();
    let visible = scene.square(1.0).unwrap();
    let detached = scene.square(2.0).unwrap();
    scene.add(&visible).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_foreground_members(scene.root(), [detached.node_id()]);
    scene.live(&mut session).apply(tx).unwrap();
    session.take_frame_changes();
    let objects = session.frame().objects.clone();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(detached.node_id());
    scene.live(&mut session).apply(tx).unwrap();
    assert!(scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .foreground_members()
        .is_empty());
    assert!(detached.validate().is_err());
    assert_eq!(session.frame().objects, objects);
    assert!(session.take_frame_changes().is_empty());
    assert_eq!(
        session.publication_context().scene_revision(),
        scene.revision()
    );
}

#[test]
fn provisional_metadata_target_and_display_entry_share_one_transaction() {
    let scene = Scene::new();
    let mut session = scene.execution_session().unwrap();
    let before = scene.revision();
    let mut tx = SemanticMutationTransaction::new();
    let target = tx.create_node(SemanticNodeCreation::object(
        noon_core::SemanticObjectState::new(noon_core::StoredGeometry::Circle { radius: 1.0 }),
    ));
    tx.add_member(scene.root(), target)
        .set_foreground_members(scene.root(), [target]);
    let result = scene.live(&mut session).apply(tx).unwrap();
    let target = result.resolve(target).unwrap();
    let store = scene.integration_store().borrow();
    assert_eq!(
        store.node(scene.root()).unwrap().foreground_members(),
        &[target]
    );
    assert_eq!(store.node(scene.root()).unwrap().members(), &[target]);
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert_eq!(session.frame().objects.len(), 1);
    assert_eq!(
        session.last_structural_publication_stats().entered_objects,
        1
    );
}

#[test]
fn foreign_store_and_stale_publication_reject_foreground_mutation() {
    let mut scene = Scene::new();
    let target = scene.square(1.0).unwrap();
    scene.add(&target).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let before = session.publication_context();
    let mut foreign = noon_core::SemanticStore::new();
    let root = foreign.insert_family();
    let child = foreign.insert_family();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_foreground_members(root, [child]);
    assert!(matches!(
        session.apply_semantic_transaction(&mut foreign, tx),
        Err(ExecutionSessionPublicationError::ForeignSemanticStore)
    ));
    assert!(foreign.node(root).unwrap().foreground_members().is_empty());
    let mut tx = SemanticMutationTransaction::new();
    tx.set_foreground_members(scene.root(), [target.node_id()]);
    tx.apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_foreground_members(
        scene.root(),
        std::iter::empty::<noon_core::SemanticNodeId>(),
    );
    assert!(scene.live(&mut session).apply(tx).is_err());
    assert_eq!(session.publication_context(), before);
    assert!(session.take_frame_changes().is_empty());
}
