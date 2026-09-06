use super::*;
use noon_core::{
    AnimationOptions, RateFunction, SemanticAnimationIntent, SemanticAnimationState,
    SemanticMutationImpact, SemanticNodeCreation, SemanticObjectProperty, SemanticObjectState,
    SemanticStyle, SemanticVec3, StoredGeometry,
};

fn fixture(count: usize) -> (SemanticStore, ExecutionSession, Vec<SemanticNodeId>) {
    let mut store = SemanticStore::new();
    let nodes = (0..count)
        .map(|_| {
            let node =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }));
            store.attach_to_scene(node).unwrap();
            node
        })
        .collect();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    (store, session, nodes)
}

fn translation(node: SemanticNodeId, x: f64) -> SemanticMutationTransaction {
    let mut tx = SemanticMutationTransaction::new();
    tx.set_property(
        node,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(x, 0.0, 0.0),
    );
    tx
}

fn rooted_family_fixture() -> (SemanticStore, ExecutionSession, SemanticNodeId) {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    store.attach_to_scene(root).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    (store, session, root)
}

#[test]
fn one_local_batch_publishes_one_coherent_context_and_only_affected_row() {
    let (mut store, mut session, nodes) = fixture(100_000);
    let node = nodes[123];
    let before = session.publication_context();
    let mut tx = translation(node, 4.0);
    tx.set_property(node, SemanticObjectProperty::RotationZ, 0.5);
    tx.replace_style(
        node,
        SemanticStyle {
            object_opacity: 0.25,
            ..Default::default()
        },
    );
    session.apply_semantic_transaction(&mut store, tx).unwrap();
    let view = session.effective_semantic_object(&store, node).unwrap();
    assert_eq!(view.object.transform.translation.x, 4.0);
    assert_eq!(view.object.transform.rotation, 0.5);
    assert_eq!(view.object.style.opacity, 0.25);
    assert_eq!(
        view.publication.scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(
        view.publication.execution_revision(),
        before.execution_revision().checked_next().unwrap()
    );
    assert_eq!(
        view.publication.frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(store.scene_revision(), view.publication.scene_revision());
    assert_eq!(session.take_frame_changes().object_indices(), &[123]);
    assert_eq!(session.runtime.last_patch_stats().objects_recomputed, 0);
    assert_eq!(session.runtime.last_patch_stats().full_seeks, 0);
    assert_eq!(session.runtime.last_patch_stats().full_group_rebuilds, 0);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn exact_noop_and_sub_f32_edit_have_distinct_publication_rules() {
    let (mut store, mut session, nodes) = fixture(1);
    session
        .apply_semantic_transaction(&mut store, translation(nodes[0], 1.0))
        .unwrap();
    session.take_frame_changes();
    let before = session.publication_context();
    session
        .apply_semantic_transaction(&mut store, translation(nodes[0], 1.0))
        .unwrap();
    assert_eq!(session.publication_context(), before);
    session
        .apply_semantic_transaction(&mut store, translation(nodes[0], 1.0 + f64::EPSILON))
        .unwrap();
    let after = session.publication_context();
    assert_eq!(
        after.scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(after.execution_revision(), before.execution_revision());
    assert_eq!(
        after.frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert!(session.take_frame_changes().is_empty());
    assert_eq!(
        store
            .semantic_object_state_checked(nodes[0])
            .unwrap()
            .transform
            .translation
            .x,
        1.0 + f64::EPSILON
    );
}

#[test]
fn late_lowering_failure_preserves_both_authorities_and_dirty_state() {
    let (mut store, mut session, nodes) = fixture(2);
    let context = session.publication_context();
    let frame = session.frame().clone();
    let authored = store
        .semantic_object_state_checked(nodes[0])
        .unwrap()
        .clone();
    let mut tx = translation(nodes[0], 2.0);
    tx.set_property(nodes[1], SemanticObjectProperty::RotationZ, f64::MAX);
    assert!(matches!(
        session.apply_semantic_transaction(&mut store, tx),
        Err(ExecutionSessionPublicationError::Lowering(_))
    ));
    assert_eq!(session.publication_context(), context);
    assert_eq!(store.scene_revision(), context.scene_revision());
    assert_eq!(
        store.semantic_object_state_checked(nodes[0]).unwrap(),
        &authored
    );
    assert_eq!(session.frame(), &frame);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn independent_and_cloned_stores_cannot_alias_publication_queries_or_animation() {
    let (store, mut session, nodes) = fixture(1);
    let (independent, _, _) = fixture(1);
    for mut foreign in [independent, store.clone()] {
        assert_eq!(foreign.scene_revision(), store.scene_revision());
        assert_eq!(
            session.apply_semantic_transaction(&mut foreign, translation(nodes[0], 1.0)),
            Err(ExecutionSessionPublicationError::ForeignSemanticStore)
        );
        assert!(matches!(
            session.effective_semantic_object(&foreign, nodes[0]),
            Err(ExecutionSessionPublicationError::ForeignSemanticStore)
        ));
        assert!(matches!(
            session.activate_animation_segment(&foreign, nodes[0], AnimationOptions::new()),
            Err(super::super::super::ExecutionSessionAnimationError::ForeignSemanticStore)
        ));
    }
    assert_eq!(store.identity(), store.identity());
    assert_ne!(store.identity(), store.clone().identity());
}

#[test]
fn out_of_band_edits_fail_closed() {
    let (mut store, mut session, nodes) = fixture(1);
    translation(nodes[0], 2.0).apply(&mut store).unwrap();
    assert!(matches!(
        session.apply_semantic_transaction(&mut store, translation(nodes[0], 3.0)),
        Err(ExecutionSessionPublicationError::StaleSceneRevision { .. })
    ));
    assert!(matches!(
        session.effective_semantic_object(&store, nodes[0]),
        Err(ExecutionSessionPublicationError::StaleSceneRevision { .. })
    ));
    assert_eq!(session.frame().objects[0].transform.translation.x, 0.0);
}

#[test]
fn effective_queries_reject_removed_generations_even_after_low_level_store_edits() {
    let (mut store, session, nodes) = fixture(1);
    store.remove_node(nodes[0]).unwrap();
    assert!(
        matches!(session.effective_semantic_object(&store, nodes[0]),
        Err(ExecutionSessionPublicationError::UnknownObject(node)) if node == nodes[0])
    );
}

fn add_target(
    store: &mut SemanticStore,
    session: &mut ExecutionSession,
    source: SemanticNodeId,
    x: f64,
) -> SemanticNodeId {
    let mut state = store.semantic_object_state_checked(source).unwrap().clone();
    state.transform.translation.x = x;
    let mut tx = SemanticMutationTransaction::new();
    tx.add_node(SemanticNodeCreation::object(state));
    let result = session.apply_semantic_transaction(store, tx).unwrap();
    let [SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
        panic!("target must be allocated once")
    };
    *node
}

#[test]
fn completed_effective_query_can_author_and_activate_the_next_segment() {
    let (mut store, mut session, nodes) = fixture(1);
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    for end in [4.0, 8.0] {
        let before = session.publication_context();
        let target = add_target(&mut store, &mut session, nodes[0], end);
        assert_eq!(
            session.publication_context().execution_revision(),
            before.execution_revision()
        );
        assert!(session.take_frame_changes().is_empty());
        // Detached target edits are also published explicitly, without touching a live row.
        session
            .apply_semantic_transaction(&mut store, translation(target, end))
            .unwrap();
        let mut tx = SemanticMutationTransaction::new();
        tx.add_animation(SemanticAnimationState::new(
            SemanticAnimationIntent::TransformTo {
                target: nodes[0],
                target_state: target,
            },
            options,
        ));
        let result = session.apply_semantic_transaction(&mut store, tx).unwrap();
        let [SemanticMutationImpact::AnimationAdded { animation }] = result.impacts() else {
            panic!("animation expected")
        };
        let segment = session
            .activate_animation_segment(&store, *animation, options)
            .unwrap();
        session
            .advance_segment_to(segment, segment.start_time() + 0.5)
            .unwrap();
        assert_eq!(
            session
                .effective_semantic_object(&store, nodes[0])
                .unwrap()
                .object
                .transform
                .translation
                .x,
            end as f32 - 2.0
        );
        session
            .advance_segment_to(segment, segment.end_time() + 10.0)
            .unwrap();
        assert!(!session.segment_state(segment).is_complete());
        session.complete_segment(&mut store, segment).unwrap();
        assert!(session.segment_state(segment).is_complete());
        assert_eq!(
            session
                .effective_semantic_object(&store, nodes[0])
                .unwrap()
                .object
                .transform
                .translation
                .x,
            end as f32
        );
        session.take_frame_changes();
    }
}

#[test]
fn pending_object_is_mutated_attached_and_published_once() {
    let (mut store, mut session, root) = rooted_family_fixture();
    let before = session.publication_context();
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 2.0 },
    )));
    transaction
        .set_property(
            pending,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(3.0, 4.0, 0.0),
        )
        .add_member(root, pending);

    let result = session
        .apply_semantic_transaction(&mut store, transaction)
        .unwrap();
    let node = result.resolve(pending).unwrap();
    let effective = session.effective_semantic_object(&store, node).unwrap();
    assert_eq!(effective.object.transform.translation.x, 3.0);
    assert_eq!(effective.object.transform.translation.y, 4.0);
    assert_eq!(session.frame().objects.len(), 1);
    assert_eq!(
        session.last_structural_publication_stats().entered_objects,
        1
    );
    assert_eq!(
        effective.publication.execution_revision(),
        before.execution_revision().checked_next().unwrap()
    );
}

#[test]
fn detached_pending_object_publishes_no_execution_work() {
    let (mut store, mut session, _) = rooted_family_fixture();
    let before = session.publication_context();
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 2.0 },
    )));
    transaction.set_property(
        pending,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(8.0, 0.0, 0.0),
    );

    let result = session
        .apply_semantic_transaction(&mut store, transaction)
        .unwrap();
    let node = result.resolve(pending).unwrap();
    assert!(session.effective_semantic_object(&store, node).is_err());
    let after = session.publication_context();
    assert_eq!(after.execution_revision(), before.execution_revision());
    assert_eq!(
        session.last_structural_publication_stats(),
        StructuralPublicationStats::default()
    );
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn aliases_publish_only_net_membership_and_last_parent_retires_the_object() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let left = store.insert_family();
    let right = store.insert_family();
    let root = store.insert_family();
    store.add_member(left, object).unwrap();
    store.add_member(right, object).unwrap();
    store.add_member(root, left).unwrap();
    store.add_member(root, right).unwrap();
    store.attach_to_scene(root).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();

    let execution_before = session.publication_context().execution_revision();
    let mut remove_alias = SemanticMutationTransaction::new();
    remove_alias.remove_member(left, object);
    session
        .apply_semantic_transaction(&mut store, remove_alias)
        .unwrap();
    assert!(session.effective_semantic_object(&store, object).is_ok());
    assert_eq!(
        session.publication_context().execution_revision(),
        execution_before
    );
    assert_eq!(
        session.last_structural_publication_stats().exited_objects,
        0
    );

    let mut remove_last = SemanticMutationTransaction::new();
    remove_last.remove_member(right, object);
    session
        .apply_semantic_transaction(&mut store, remove_last)
        .unwrap();
    assert!(matches!(
        session.effective_semantic_object(&store, object),
        Err(ExecutionSessionPublicationError::UnknownObject(node)) if node == object
    ));
    assert_eq!(
        session.last_structural_publication_stats().exited_objects,
        1
    );
}

#[test]
fn removing_reachable_family_cascades_only_its_execution_leaves() {
    let mut store = SemanticStore::new();
    let keep = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let removed = [2.0, 3.0].map(|radius| {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    });
    let subtree = store.insert_family();
    let root = store.insert_family();
    for node in removed {
        store.add_member(subtree, node).unwrap();
    }
    store.add_member(root, keep).unwrap();
    store.add_member(root, subtree).unwrap();
    store.attach_to_scene(root).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_member(root, subtree);
    session
        .apply_semantic_transaction(&mut store, transaction)
        .unwrap();

    assert!(session.effective_semantic_object(&store, keep).is_ok());
    for node in removed {
        assert!(session.effective_semantic_object(&store, node).is_err());
    }
    let stats = session.last_structural_publication_stats();
    assert_eq!(stats.exited_objects, 2);
    assert_eq!(stats.preparation.possible_exits, 2);
    assert_eq!(session.runtime.last_patch_stats().full_seeks, 0);
    assert_eq!(session.runtime.last_patch_stats().full_group_rebuilds, 0);
}

#[test]
fn painter_interleaving_fails_before_semantic_or_runtime_publication() {
    let mut store = SemanticStore::new();
    let earlier = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let later = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 2.0,
    }));
    let root = store.insert_family();
    store.add_member(root, later).unwrap();
    store.attach_to_scene(root).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    let context = session.publication_context();
    let frame = session.frame().clone();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_member(root, earlier);
    assert!(matches!(
        session.apply_semantic_transaction(&mut store, transaction),
        Err(ExecutionSessionPublicationError::Lowering(
            SemanticPublicationLoweringError::PainterOrderInterleaving { .. }
        ))
    ));
    assert_eq!(store.scene_revision(), context.scene_revision());
    assert_eq!(session.publication_context(), context);
    assert_eq!(session.frame(), &frame);
    assert_eq!(store.node(root).unwrap().members(), &[later]);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn authored_value_change_during_animation_is_rejected_before_publication() {
    let (mut store, mut session, nodes) = fixture(1);
    let target = add_target(&mut store, &mut session, nodes[0], 4.0);
    let options = AnimationOptions::new()
        .run_time(2.0)
        .rate_func(RateFunction::Linear);
    let mut tx = SemanticMutationTransaction::new();
    tx.add_animation(SemanticAnimationState::new(
        SemanticAnimationIntent::TransformTo {
            target: nodes[0],
            target_state: target,
        },
        options,
    ));
    let result = session.apply_semantic_transaction(&mut store, tx).unwrap();
    let [SemanticMutationImpact::AnimationAdded { animation }] = result.impacts() else {
        panic!()
    };
    let segment = session
        .activate_animation_segment(&store, *animation, options)
        .unwrap();
    session.seek(1.0).unwrap();
    let store_revision = store.scene_revision();
    let publication = session.publication_context();
    let frame = session.frame().clone();
    assert_eq!(
        session.apply_semantic_transaction(&mut store, translation(nodes[0], 100.0)),
        Err(ExecutionSessionPublicationError::SegmentCompletionPending)
    );
    assert_eq!(store.scene_revision(), store_revision);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame(), &frame);
    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.complete_segment(&mut store, segment).unwrap();
}

#[test]
fn authored_publication_is_rejected_while_required_callback_is_pending() {
    let (mut store, mut session, nodes) = fixture(1);
    let context = session.publication_context();
    let frame = session.frame().clone();
    let store_revision = store.scene_revision();
    let token = session
        .begin_required_callback_phase(1.0, [nodes[0]])
        .unwrap()
        .token();

    assert_eq!(
        session.apply_semantic_transaction(&mut store, translation(nodes[0], 3.0)),
        Err(ExecutionSessionPublicationError::RequiredCallbackPending)
    );
    assert_eq!(store.scene_revision(), store_revision);
    assert_eq!(session.publication_context(), context);
    assert_eq!(session.frame(), &frame);
    session.fail_required_callback_phase(token).unwrap();
}
