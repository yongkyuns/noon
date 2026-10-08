use super::*;
use noon_compile::semantic_execution_object_id;
use noon_core::{
    AnimationOptions, GeometryRef, GeometryResourceLookup, MeshResource, RateFunction,
    SemanticAnimationIntent, SemanticAnimationState, SemanticMutationImpact, SemanticNodeCreation,
    SemanticObjectProperty, SemanticObjectRole, SemanticObjectState, SemanticOrientation,
    SemanticRotation3D, SemanticStyle, SemanticTransform, SemanticVec3, StoredGeometry,
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
    let mut transform = store.semantic_object_state_checked(node).unwrap().transform;
    transform.translation.x = 4.0;
    transform.orientation = SemanticOrientation::Planar(0.5);
    let mut tx = SemanticMutationTransaction::new();
    tx.set_object_transform(node, transform);
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
fn one_spatial_transform_publishes_one_mesh_row_and_invalid_batch_is_atomic() {
    let mut store = SemanticStore::new();
    let mesh = MeshResource::new(
        vec![
            SemanticVec3::new(-1.0, -1.0, 0.0),
            SemanticVec3::new(1.0, -1.0, 0.0),
            SemanticVec3::new(0.0, 1.0, 0.0),
        ],
        None,
        vec![0, 1, 2],
    )
    .unwrap();
    let resource = store.insert_geometry_mesh(mesh);
    let first =
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(resource)));
    let second =
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(resource)));
    store.attach_to_scene(first).unwrap();
    store.attach_to_scene(second).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();

    let rotation =
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.4).unwrap();
    let mut transform = store
        .semantic_object_state_checked(first)
        .unwrap()
        .transform;
    transform.translation = SemanticVec3::new(2.0, 3.0, 4.0);
    transform.orientation = SemanticOrientation::Spatial(rotation);
    let expected_world = transform.world_transform().unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_object_transform(first, transform);
    session
        .apply_semantic_transaction(&mut store, transaction)
        .unwrap();

    let effective = session.effective_semantic_object(&store, first).unwrap();
    assert_eq!(effective.object.world_transform(), Some(expected_world));
    assert_eq!(session.take_frame_changes().object_indices(), &[0]);
    assert_eq!(session.runtime.last_patch_stats().objects_recomputed, 0);
    assert_eq!(session.runtime.last_patch_stats().full_seeks, 0);
    assert_eq!(session.runtime.last_patch_stats().full_group_rebuilds, 0);

    let mut component_edit = SemanticMutationTransaction::new();
    component_edit.set_property(
        first,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(5.0, 6.0, 7.0),
    );
    session
        .apply_semantic_transaction(&mut store, component_edit)
        .unwrap();
    assert_eq!(
        session
            .effective_semantic_object(&store, first)
            .unwrap()
            .object
            .world_transform()
            .unwrap()
            .translation,
        SemanticVec3::new(5.0, 6.0, 7.0)
    );
    assert_eq!(session.take_frame_changes().object_indices(), &[0]);

    let before_publication = session.publication_context();
    let before_first = store
        .semantic_object_state_checked(first)
        .unwrap()
        .transform;
    let mut invalid = store
        .semantic_object_state_checked(second)
        .unwrap()
        .transform;
    invalid.orientation = SemanticOrientation::Planar(f64::NAN);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_object_transform(first, SemanticTransform::default());
    transaction.set_object_transform(second, invalid);
    assert!(session
        .apply_semantic_transaction(&mut store, transaction)
        .is_err());
    assert_eq!(session.publication_context(), before_publication);
    assert_eq!(
        store
            .semantic_object_state_checked(first)
            .unwrap()
            .transform,
        before_first
    );
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn live_mesh_admission_publishes_spatial_pose_and_resource_and_rejects_domain_change_atomically() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    store.attach_to_scene(root).unwrap();
    let mesh = MeshResource::new(
        vec![
            SemanticVec3::new(-1.0, -1.0, 0.0),
            SemanticVec3::new(1.0, -1.0, 0.0),
            SemanticVec3::new(0.0, 1.0, 0.0),
        ],
        None,
        vec![0, 1, 2],
    )
    .unwrap();
    let handle = store.insert_geometry_mesh(mesh.clone());
    let rotation =
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.4).unwrap();
    let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
    state.transform.translation = SemanticVec3::new(2.0, 3.0, 4.0);
    state.transform.orientation = SemanticOrientation::Spatial(rotation);
    let expected_world = state.transform.world_transform().unwrap();
    let object = store.insert_semantic_object(state);
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();

    let mut admission = SemanticMutationTransaction::new();
    admission.add_member(root, object);
    session
        .apply_semantic_transaction(&mut store, admission)
        .unwrap();

    let effective = session.effective_semantic_object(&store, object).unwrap();
    assert_eq!(effective.object.world_transform(), Some(expected_world));
    let GeometryRef::External(resource_id) = session.frame().objects[0].content.geometry().unwrap()
    else {
        panic!("spatial mesh admission must retain its geometry resource reference")
    };
    let retained_handle = session
        .geometry_resources()
        .current_handle(*resource_id)
        .expect("admitted mesh resource handle");
    assert_eq!(
        session.geometry_resources().get(retained_handle),
        Some(&noon_core::GeometryResource::Mesh(std::sync::Arc::new(
            mesh
        )))
    );
    assert_eq!(session.take_frame_changes().object_indices(), &[0]);
    assert_eq!(
        session.last_structural_publication_stats().entered_objects,
        1
    );
    assert_eq!(session.runtime.last_patch_stats().objects_recomputed, 0);
    assert_eq!(session.runtime.last_patch_stats().full_seeks, 0);
    assert_eq!(session.runtime.last_patch_stats().full_group_rebuilds, 0);

    let before_context = session.publication_context();
    let before_frame = session.frame().clone();
    let before_state = store.semantic_object_state_checked(object).unwrap().clone();
    let mut cross_domain = SemanticMutationTransaction::new();
    cross_domain.replace_content(object, StoredGeometry::Circle { radius: 1.0 });
    assert!(matches!(
        session.apply_semantic_transaction(&mut store, cross_domain),
        Err(ExecutionSessionPublicationError::Lowering(_))
    ));
    assert_eq!(session.publication_context(), before_context);
    assert_eq!(session.frame(), &before_frame);
    assert_eq!(
        store.semantic_object_state_checked(object).unwrap(),
        &before_state
    );
    assert!(session.take_frame_changes().is_empty());
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
fn detached_creation_rolls_back_with_a_late_lowering_failure() {
    let (mut store, mut session, nodes) = fixture(2);
    let context = session.publication_context();
    let frame = session.frame().clone();
    let node_count = store.len();
    let slot_capacity = store.slot_capacity();
    let authored = store
        .semantic_object_state_checked(nodes[0])
        .unwrap()
        .clone();
    let mut tx = translation(nodes[0], 2.0);
    tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 2.0 },
    )));
    tx.set_property(nodes[1], SemanticObjectProperty::RotationZ, f64::MAX);
    assert!(matches!(
        session.apply_semantic_transaction(&mut store, tx),
        Err(ExecutionSessionPublicationError::Lowering(_))
    ));
    assert_eq!(session.publication_context(), context);
    assert_eq!(store.scene_revision(), context.scene_revision());
    assert_eq!(store.len(), node_count);
    assert_eq!(store.slot_capacity(), slot_capacity);
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
                interpolation: noon_core::SemanticTransformInterpolation::Affine,

                complete_priority: false,
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
    let (mut store, mut session, root) = rooted_family_fixture();
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
        .apply_semantic_transaction_at_root(&mut store, root, transaction)
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
fn detached_creation_and_local_write_preserve_unrelated_rows_and_later_admission() {
    let (mut store, mut session, root, nodes) = rooted_slot_fixture(100_000);
    let before = session.publication_context();
    let mut transaction = translation(nodes[123], 4.0);
    let pending = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 2.0 },
    )));
    transaction.set_property(
        pending,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(8.0, 0.0, 0.0),
    );
    let result = session
        .apply_semantic_transaction_at_root(&mut store, root, transaction)
        .unwrap();
    let detached = result.resolve(pending).unwrap();
    assert_eq!(session.frame().objects.len(), nodes.len());
    assert_eq!(session.take_frame_changes().object_indices(), &[123]);
    assert_eq!(session.frame().objects[123].transform.translation.x, 4.0);
    assert_eq!(session.frame().objects[124].transform.translation.x, 0.0);
    assert_eq!(
        store
            .semantic_object_state_checked(detached)
            .unwrap()
            .transform
            .translation
            .x,
        8.0
    );
    assert!(session.effective_semantic_object(&store, detached).is_err());
    assert_eq!(
        session.last_structural_publication_stats().entered_objects,
        0
    );
    assert_eq!(session.runtime.last_patch_stats().full_seeks, 0);
    assert_eq!(session.runtime.last_patch_stats().full_group_rebuilds, 0);
    assert_eq!(
        session.publication_context().scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );

    let mut admit = SemanticMutationTransaction::new();
    admit.add_member(root, detached);
    session
        .apply_semantic_transaction_at_root(&mut store, root, admit)
        .unwrap();
    assert_eq!(session.frame().objects.len(), nodes.len() + 1);
    assert_eq!(
        session
            .effective_semantic_object(&store, detached)
            .unwrap()
            .object
            .transform
            .translation
            .x,
        8.0
    );
    assert_eq!(
        session.last_structural_publication_stats().entered_objects,
        1
    );
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
fn precreated_detached_object_appends_through_root_order_publication() {
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
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_member(root, earlier);
    session
        .apply_semantic_transaction_at_root(&mut store, root, transaction)
        .unwrap();

    assert_eq!(store.node(root).unwrap().members(), &[later, earlier]);
    let ordered = session
        .painter_order()
        .iter()
        .map(|&index| session.frame().objects[index as usize].id)
        .collect::<Vec<_>>();
    assert_eq!(ordered.len(), 2);
    assert_eq!(ordered[1], semantic_execution_object_id(earlier));
    assert!(session.take_frame_changes().is_structural());
}

#[test]
fn root_reorder_publishes_painter_order_without_moving_frame_rows() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let nodes = (0..3)
        .map(|radius| {
            let node =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: radius as f32 + 1.0,
                }));
            store.add_member(root, node).unwrap();
            node
        })
        .collect::<Vec<_>>();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    let dense_ids = session
        .frame()
        .objects
        .iter()
        .map(|object| object.id)
        .collect::<Vec<_>>();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(root, nodes[2], Some(nodes[0]));
    session
        .apply_semantic_transaction_at_root(&mut store, root, transaction)
        .unwrap();

    assert_eq!(
        store.node(root).unwrap().members(),
        &[nodes[2], nodes[0], nodes[1]]
    );
    assert_eq!(
        session
            .painter_order()
            .iter()
            .map(|&index| session.frame().objects[index as usize].id)
            .collect::<Vec<_>>(),
        vec![dense_ids[2], dense_ids[0], dense_ids[1]]
    );
    assert_eq!(
        session
            .frame()
            .objects
            .iter()
            .map(|object| object.id)
            .collect::<Vec<_>>(),
        dense_ids
    );
    let changes = session.take_frame_changes();
    assert_eq!(changes.painter_order_range(), Some(0..3));
    assert!(changes.object_indices().is_empty());
}

#[test]
fn unrooted_reorder_fails_before_semantic_or_runtime_publication() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let first = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let second = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 2.0,
    }));
    store.add_member(family, first).unwrap();
    store.add_member(family, second).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    let context = session.publication_context();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(family, second, Some(first));
    assert!(matches!(
        session.apply_semantic_transaction(&mut store, transaction),
        Err(ExecutionSessionPublicationError::Lowering(
            SemanticPublicationLoweringError::PainterOrderRootRequired { family: rejected }
        )) if rejected.existing() == Some(family)
    ));
    assert_eq!(store.node(family).unwrap().members(), &[first, second]);
    assert_eq!(session.publication_context(), context);
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
            interpolation: noon_core::SemanticTransformInterpolation::Affine,

            complete_priority: false,
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

#[test]
fn live_updater_removal_replacement_and_freeze_keep_one_runtime() {
    use crate::RustHostCallbackTable;
    use noon_core::HostCallbackId;
    let mut store = SemanticStore::new();
    let node = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(node).unwrap();
    for _ in 0..1024 {
        let sibling =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.1,
            }));
        store.attach_to_scene(sibling).unwrap();
    }
    let forth = HostCallbackId::new(7);
    let back = HostCallbackId::new(8);
    let mut callbacks = RustHostCallbackTable::new();
    for (id, sign) in [(forth, 1.0), (back, -1.0)] {
        callbacks
            .insert(id, move |context| {
                let mut transform = context.target_state().transform;
                transform.translation.x += sign * context.delta_time() as f32;
                context.set_target_transform(transform)
            })
            .unwrap();
    }
    callbacks
        .add_updater(&mut store, node, forth, 0.0, None)
        .unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let runtime = session.runtime_identity();
    callbacks.advance_to(&mut session, 2.0).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 2.0);
    session.take_frame_changes();
    let before = session.publication_context();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_updater(node, forth, 2.0);
    tx.add_updater(node, back, 2.0, None);
    session.apply_semantic_transaction(&mut store, tx).unwrap();
    assert_eq!(session.runtime_identity(), runtime);
    assert_eq!(session.frame().time, 2.0);
    assert_eq!(session.frame().objects[0].transform.translation.x, 2.0);
    assert_eq!(
        session.publication_context().scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert!(session.take_frame_changes().is_empty());
    assert_eq!(
        session
            .last_structural_publication_stats()
            .preparation
            .object_states_lowered,
        0
    );
    assert_eq!(session.runtime.last_patch_stats().full_seeks, 0);
    assert_eq!(session.runtime.last_patch_stats().objects_recomputed, 0);
    assert_eq!(session.runtime.last_patch_stats().full_group_rebuilds, 0);
    callbacks.advance_to(&mut session, 3.0).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 1.0);
    let mut clear = SemanticMutationTransaction::new();
    clear.clear_updaters(node, 3.0);
    clear.set_property(
        node,
        SemanticObjectProperty::Scale,
        SemanticVec3::new(0.75, 1.0, 1.0),
    );
    session
        .apply_semantic_transaction(&mut store, clear)
        .unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(node)
            .unwrap()
            .transform
            .scale
            .x,
        0.75,
        "an explicit authored channel in the same transaction takes precedence"
    );
    assert_eq!(
        store
            .semantic_object_state_checked(node)
            .unwrap()
            .transform
            .translation
            .x,
        1.0,
        "the released callback translation is reconciled through authored state"
    );
    callbacks.advance_to(&mut session, 4.0).unwrap();
    assert_eq!(
        session.frame().objects[0].transform.translation.x,
        1.0,
        "removal must freeze the last effective value, not restore authored state"
    );
    assert_eq!(session.frame().objects[0].transform.scale.x, 0.75);
    assert_eq!(session.runtime_identity(), runtime);
    assert_eq!(
        session.wake_state().timeline(),
        noon_runtime::TimelineWakeState::Quiescent
    );
    let before = session.publication_context();
    let mut noop = SemanticMutationTransaction::new();
    noop.remove_updater(node, back, 4.0);
    session
        .apply_semantic_transaction(&mut store, noop)
        .unwrap();
    assert_eq!(session.publication_context(), before);
}

#[test]
fn callback_release_persists_owned_camera_channels_and_leaves_active_targets_alone() {
    use crate::RustHostCallbackTable;
    use noon_core::HostCallbackId;

    let mut store = SemanticStore::new();
    let mut camera_state = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: noon_core::Vec2::new(8.0, 4.0),
    });
    camera_state.style.object_opacity = 0.0;
    camera_state.set_role(SemanticObjectRole::Camera2D);
    let camera = store.insert_semantic_object(camera_state);
    let other = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 0.5,
    }));
    store.attach_to_scene(camera).unwrap();
    store.attach_to_scene(other).unwrap();

    let camera_callback = HostCallbackId::new(31);
    let other_callback = HostCallbackId::new(32);
    let mut callbacks = RustHostCallbackTable::new();
    callbacks
        .insert(camera_callback, |context| {
            let mut transform = context.target_state().transform;
            transform.translation.x += 2.0 * context.delta_time() as f32;
            transform.scale.x = 0.5;
            transform.scale.y = 0.5;
            context.set_target_transform(transform)
        })
        .unwrap();
    callbacks
        .insert(other_callback, |context| {
            let mut transform = context.target_state().transform;
            transform.translation.x += 3.0 * context.delta_time() as f32;
            context.set_target_transform(transform)
        })
        .unwrap();
    callbacks
        .add_updater(&mut store, camera, camera_callback, 0.0, None)
        .unwrap();
    callbacks
        .add_updater(&mut store, other, other_callback, 0.0, None)
        .unwrap();

    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    callbacks.advance_to(&mut session, 1.0).unwrap();
    let effective_before_release = session.frame().clone();
    let revision_before_rejected_release = store.scene_revision();

    let mut rejected = SemanticMutationTransaction::new();
    rejected.remove_updater(camera, camera_callback, 1.0);
    rejected.set_property(camera, SemanticObjectProperty::ObjectOpacity, f64::NAN);
    assert!(session
        .apply_semantic_transaction(&mut store, rejected)
        .is_err());
    assert_eq!(store.scene_revision(), revision_before_rejected_release);
    assert_eq!(session.frame(), &effective_before_release);
    assert_eq!(
        store
            .semantic_object_state_checked(camera)
            .unwrap()
            .transform
            .translation
            .x,
        0.0,
        "a rejected release must not partially reconcile authored state"
    );

    let mut remove_camera = SemanticMutationTransaction::new();
    remove_camera.remove_updater(camera, camera_callback, 1.0);
    session
        .apply_semantic_transaction(&mut store, remove_camera)
        .unwrap();
    let authored_camera = store
        .semantic_object_state_checked(camera)
        .unwrap()
        .transform;
    assert_eq!(authored_camera.translation.x, 2.0);
    assert_eq!(authored_camera.scale.x, 0.5);
    assert_eq!(authored_camera.planar_rotation(), Some(0.0));
    assert_eq!(
        store
            .semantic_object_state_checked(other)
            .unwrap()
            .transform
            .translation
            .x,
        0.0,
        "an unrelated callback that remains active keeps its authored baseline"
    );

    callbacks.advance_to(&mut session, 1.5).unwrap();
    let camera_frame = session
        .effective_semantic_object(&store, camera)
        .unwrap()
        .object
        .transform;
    assert_eq!(camera_frame.translation.x, 2.0);
    assert_eq!(camera_frame.scale.x, 0.5);
    assert_eq!(
        session.camera().unwrap().center,
        noon_core::Vec2::new(2.0, 0.0)
    );
    assert_eq!(session.camera().unwrap().height, 2.0);
    assert_eq!(
        session
            .effective_semantic_object(&store, other)
            .unwrap()
            .object
            .transform
            .translation
            .x,
        4.5,
        "the still-active callback continues advancing its own target"
    );

    let mut add_restore_target = SemanticMutationTransaction::new();
    add_restore_target.add_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Rectangle {
            size: noon_core::Vec2::new(8.0, 4.0),
        },
    )));
    let result = session
        .apply_semantic_transaction(&mut store, add_restore_target)
        .unwrap();
    let [SemanticMutationImpact::NodeAdded {
        node: restore_target,
    }] = result.impacts()
    else {
        panic!("Restore target allocation")
    };
    let restore_options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let mut add_restore = SemanticMutationTransaction::new();
    add_restore.add_animation(SemanticAnimationState::new(
        SemanticAnimationIntent::TransformTo {
            target: camera,
            target_state: *restore_target,
            interpolation: noon_core::SemanticTransformInterpolation::Affine,
            complete_priority: false,
        },
        restore_options,
    ));
    let result = session
        .apply_semantic_transaction(&mut store, add_restore)
        .unwrap();
    let [SemanticMutationImpact::AnimationAdded {
        animation: restore_animation,
    }] = result.impacts()
    else {
        panic!("Restore animation declaration")
    };
    let segment = session
        .activate_animation_segment(&store, *restore_animation, restore_options)
        .unwrap();
    callbacks
        .advance_segment_to(&mut session, segment, segment.start_time())
        .unwrap();
    let restore_start = session
        .effective_semantic_object(&store, camera)
        .unwrap()
        .object
        .transform;
    assert_eq!(restore_start.translation.x, 2.0);
    assert_eq!(restore_start.scale.x, 0.5);
    assert_eq!(restore_start.rotation, 0.0);
}

#[test]
fn removing_one_duplicate_callback_registration_does_not_release_its_driver() {
    use crate::RustHostCallbackTable;
    use noon_core::HostCallbackId;

    let (mut store, _, nodes) = fixture(1);
    let node = nodes[0];
    let callback = HostCallbackId::new(41);
    let mut callbacks = RustHostCallbackTable::new();
    callbacks
        .insert(callback, |context| {
            let mut transform = context.target_state().transform;
            transform.translation.x += context.delta_time() as f32;
            context.set_target_transform(transform)
        })
        .unwrap();
    callbacks
        .add_updater(&mut store, node, callback, 0.0, None)
        .unwrap();
    callbacks
        .add_updater(&mut store, node, callback, 0.0, None)
        .unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    callbacks.advance_to(&mut session, 1.0).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 2.0);

    let mut remove_one = SemanticMutationTransaction::new();
    remove_one.remove_updater(node, callback, 1.0);
    session
        .apply_semantic_transaction(&mut store, remove_one)
        .unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(node)
            .unwrap()
            .transform
            .translation
            .x,
        0.0,
        "the second active occurrence still owns this callback target"
    );
    callbacks.advance_to(&mut session, 1.5).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 2.5);
}

#[test]
fn live_updater_edits_reject_pending_phases_retroactivity_and_unindexed_targets_atomically() {
    use crate::execution_session::CallbackAdvance;
    use noon_core::HostCallbackId;
    let mut store = SemanticStore::new();
    let node = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(node).unwrap();
    let other = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let mut initial = SemanticMutationTransaction::new();
    initial.add_updater(node, HostCallbackId::new(1), 0.0, None);
    initial.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let CallbackAdvance::HostRequired { overlay, .. } =
        session.advance_to_callback_barrier(0.0).unwrap()
    else {
        panic!("initial phase")
    };
    let before = session.publication_context();
    let mut remove = SemanticMutationTransaction::new();
    remove.clear_updaters(node, 0.0);
    assert!(matches!(
        session.apply_semantic_transaction(&mut store, remove),
        Err(ExecutionSessionPublicationError::RequiredCallbackPending)
    ));
    assert_eq!(store.scene_revision(), before.scene_revision());
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    let CallbackAdvance::HostRequired { overlay, .. } =
        session.advance_to_callback_barrier(1.0).unwrap()
    else {
        panic!("next phase")
    };
    let stale = overlay.clone().finish();
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    let before = session.publication_context();
    let before_authored_transform = store.semantic_object_state_checked(node).unwrap().transform;
    let before_effective_transform = session.frame().objects[0].transform;
    for (target, time) in [(node, 0.5), (other, 1.0)] {
        let mut tx = SemanticMutationTransaction::new();
        // A rejected callback revision also rolls back unrelated authored edits.
        tx.set_property(
            node,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(3.0, 0.0, 0.0),
        );
        tx.add_updater(target, HostCallbackId::new(2), time, None);
        assert!(session.apply_semantic_transaction(&mut store, tx).is_err());
        assert_eq!(session.publication_context(), before);
        assert_eq!(store.scene_revision(), before.scene_revision());
        assert_eq!(
            store.semantic_object_state_checked(node).unwrap().transform,
            before_authored_transform
        );
        assert_eq!(
            session.frame().objects[0].transform,
            before_effective_transform
        );
    }
    let mut remove = SemanticMutationTransaction::new();
    remove.clear_updaters(node, 1.0);
    session
        .apply_semantic_transaction(&mut store, remove)
        .unwrap();
    assert!(session.commit_required_callback_phase(stale).is_err());
    assert!(matches!(
        session.advance_to_callback_barrier(1.0).unwrap(),
        CallbackAdvance::Ready(_)
    ));
}

#[test]
fn live_updater_revision_preserves_target_preorder_and_future_barriers() {
    use crate::execution_session::CallbackAdvance;
    use noon_core::HostCallbackId;
    let mut store = SemanticStore::new();
    let nodes = (0..2)
        .map(|_| {
            let node =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }));
            store.attach_to_scene(node).unwrap();
            node
        })
        .collect::<Vec<_>>();
    let mut initial = SemanticMutationTransaction::new();
    for &node in &nodes {
        initial.add_updater(node, HostCallbackId::new(1), 0.0, None);
    }
    initial.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    for time in [0.0, 1.0] {
        let CallbackAdvance::HostRequired { overlay, .. } =
            session.advance_to_callback_barrier(time).unwrap()
        else {
            panic!("active phase")
        };
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
    }
    let mut revision = SemanticMutationTransaction::new();
    revision.clear_updaters(nodes[0], 1.0);
    revision.add_updater(nodes[0], HostCallbackId::new(2), 2.0, None);
    session
        .apply_semantic_transaction(&mut store, revision)
        .unwrap();
    let CallbackAdvance::HostRequired {
        overlay,
        invocations,
    } = session.advance_to_callback_barrier(3.0).unwrap()
    else {
        panic!("activation barrier")
    };
    assert_eq!(overlay.time(), 2.0);
    assert_eq!(
        invocations
            .iter()
            .map(|item| (item.target(), item.callback_id()))
            .collect::<Vec<_>>(),
        vec![
            (nodes[0], HostCallbackId::new(2)),
            (nodes[1], HostCallbackId::new(1))
        ]
    );
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
}

fn rooted_slot_fixture(
    count: usize,
) -> (
    SemanticStore,
    ExecutionSession,
    SemanticNodeId,
    Vec<SemanticNodeId>,
) {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let nodes = (0..count)
        .map(|_| {
            let node =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 1.0,
                }));
            store.add_member(root, node).unwrap();
            node
        })
        .collect();
    store.attach_to_scene(root).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    (store, session, root, nodes)
}

#[test]
fn semantic_replacement_churn_reuses_durable_slots_and_publishes_atomically() {
    // One slot, a rotating subset, and whole-set replacement exercise the same
    // product publication path. No independent runtime wrapper drives mutations.
    for (working_set, batch_size) in [(1, 1), (128, 1), (32, 32)] {
        let (mut store, mut session, root, mut nodes) = rooted_slot_fixture(working_set);
        for iteration in 0..1_000 {
            let start = iteration % working_set;
            let replaced: Vec<_> = (0..batch_size)
                .map(|offset| (start + offset) % working_set)
                .collect();
            let stale: Vec<_> = replaced
                .iter()
                .map(|&index| {
                    let object = session.execution_object_id(nodes[index]).unwrap();
                    (nodes[index], session.slots.slot_for_object(object).unwrap())
                })
                .collect();
            let untouched = if batch_size < working_set {
                let node = nodes[(start + batch_size) % working_set];
                Some((
                    node,
                    session
                        .slots
                        .slot_for_object(session.execution_object_id(node).unwrap())
                        .unwrap(),
                ))
            } else {
                None
            };
            let before = session.publication_context();
            let mut transaction = SemanticMutationTransaction::new();
            let pending: Vec<_> = (0..batch_size)
                .map(|_| {
                    let node = transaction.create_node(SemanticNodeCreation::object(
                        SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }),
                    ));
                    transaction.add_member(root, node);
                    node
                })
                .collect();
            // Semantic node removals form the transaction's terminal suffix;
            // execution membership still retires exits before allocating entries.
            for &(node, _) in &stale {
                transaction.remove_node(node);
            }
            let result = session
                .apply_semantic_transaction(&mut store, transaction)
                .unwrap();
            assert_eq!(session.slots.slot_capacity(), working_set);
            assert_eq!(session.slots.len(), working_set);
            let after = session.publication_context();
            assert_eq!(
                after.scene_revision(),
                before.scene_revision().checked_next().unwrap()
            );
            assert_eq!(
                after.execution_revision(),
                before.execution_revision().checked_next().unwrap()
            );
            assert_eq!(
                after.frame_epoch(),
                before.frame_epoch().checked_next().unwrap()
            );
            assert_eq!(
                session.last_structural_publication_stats().entered_objects,
                batch_size
            );
            assert_eq!(
                session.last_structural_publication_stats().exited_objects,
                batch_size
            );
            assert_eq!(session.last_patch_stats().full_seeks, 0);
            assert_eq!(session.last_patch_stats().full_group_rebuilds, 0);
            for &(node, slot) in &stale {
                assert!(store.node(node).is_none());
                assert_eq!(session.slots.object_for_slot(slot), None);
            }
            for (index, pending) in replaced.into_iter().zip(pending) {
                let node = result.resolve(pending).unwrap();
                let object = session.execution_object_id(node).unwrap();
                let slot = session.slots.slot_for_object(object).unwrap();
                let previous = stale
                    .iter()
                    .find(|(_, old)| old.slot() == slot.slot())
                    .unwrap()
                    .1;
                assert_eq!(slot.generation(), previous.generation() + 1);
                assert_eq!(session.slots.object_for_slot(slot), Some(object));
                nodes[index] = node;
            }
            if let Some((node, slot)) = untouched {
                assert_eq!(
                    session
                        .slots
                        .slot_for_object(session.execution_object_id(node).unwrap()),
                    Some(slot)
                );
            }
            session.take_frame_changes();
        }
    }
}

#[test]
fn semantic_temporary_scene_releases_membership_and_spatial_leaves() {
    const TEMPORARY_OBJECTS: usize = 4_096;
    const SURVIVORS: usize = 8;
    let (mut store, mut session, root, nodes) = rooted_slot_fixture(TEMPORARY_OBJECTS);
    let viewport = noon_core::Rect::new(
        noon_core::Vec2::new(-2.0, -2.0),
        noon_core::Vec2::new(2.0, 2.0),
    );
    assert_eq!(
        session.query_viewport(viewport).object_indices().len(),
        TEMPORARY_OBJECTS
    );
    let survivors: Vec<_> = nodes[..SURVIVORS]
        .iter()
        .map(|&node| {
            let object = session.execution_object_id(node).unwrap();
            (object, session.slots.slot_for_object(object).unwrap())
        })
        .collect();
    let mut transaction = SemanticMutationTransaction::new();
    for &node in &nodes[SURVIVORS..] {
        transaction.remove_node(node);
    }
    session
        .apply_semantic_transaction(&mut store, transaction)
        .unwrap();
    assert_eq!(session.slots.len(), SURVIVORS);
    assert_eq!(
        session.query_viewport(viewport).object_indices().len(),
        SURVIVORS
    );
    assert_eq!(session.last_spatial_update_stats().full_rebuilds, 0);
    assert_eq!(
        session.last_spatial_update_stats().leaves_removed,
        TEMPORARY_OBJECTS - SURVIVORS
    );
    for _ in 0..1_000 {
        let mut create = SemanticMutationTransaction::new();
        let pending = create.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 1.0 },
        )));
        create.add_member(root, pending);
        let result = session
            .apply_semantic_transaction(&mut store, create)
            .unwrap();
        let node = result.resolve(pending).unwrap();
        let object = session.execution_object_id(node).unwrap();
        let slot = session.slots.slot_for_object(object).unwrap();
        assert_eq!(session.slots.len(), SURVIVORS + 1);
        assert_eq!(session.slots.slot_capacity(), TEMPORARY_OBJECTS);
        let mut remove = SemanticMutationTransaction::new();
        remove.remove_node(node);
        session
            .apply_semantic_transaction(&mut store, remove)
            .unwrap();
        assert_eq!(session.slots.object_for_slot(slot), None);
        assert_eq!(session.slots.len(), SURVIVORS);
        assert_eq!(session.slots.slot_capacity(), TEMPORARY_OBJECTS);
        for &(object, slot) in &survivors {
            assert_eq!(session.slots.slot_for_object(object), Some(slot));
        }
        session.take_frame_changes();
    }
}

#[test]
fn rejected_semantic_replacement_does_not_consume_slots_or_publication() {
    let (mut store, mut session, root, nodes) = rooted_slot_fixture(1);
    let node = nodes[0];
    let object = session.execution_object_id(node).unwrap();
    let slot = session.slots.slot_for_object(object).unwrap();
    let before = session.publication_context();
    let mut invalid = SemanticMutationTransaction::new();
    let replacement = invalid.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    invalid.add_member(root, replacement);
    invalid.set_property(replacement, SemanticObjectProperty::Translation, f64::NAN);
    invalid.remove_node(node);
    assert!(session
        .apply_semantic_transaction(&mut store, invalid)
        .is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.slots.slot_for_object(object), Some(slot));
    assert_eq!(session.slots.len(), 1);
    assert_eq!(session.slots.slot_capacity(), 1);
    assert!(session.take_frame_changes().is_empty());
    assert!(session.effective_semantic_object(&store, node).is_ok());

    let mut valid = SemanticMutationTransaction::new();
    let replacement = valid.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 2.0 },
    )));
    valid.add_member(root, replacement);
    valid.remove_node(node);
    let result = session
        .apply_semantic_transaction(&mut store, valid)
        .unwrap();
    let replacement = result.resolve(replacement).unwrap();
    let replacement_object = session.execution_object_id(replacement).unwrap();
    let replacement_slot = session.slots.slot_for_object(replacement_object).unwrap();
    assert_eq!(replacement_slot.slot(), slot.slot());
    assert_eq!(replacement_slot.generation(), slot.generation() + 1);
    assert_eq!(session.slots.object_for_slot(slot), None);
    assert_eq!(session.slots.slot_capacity(), 1);
}
