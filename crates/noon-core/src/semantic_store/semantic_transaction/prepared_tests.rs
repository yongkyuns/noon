use super::*;
use crate::{
    SceneRevision, SemanticObjectContent, SemanticObjectRole, SemanticStyle, SemanticTransform2_5D,
    SemanticVec3, Vec2, VectorPath,
};

fn pending_path_node(
    transaction: &mut SemanticMutationTransaction,
    path: VectorPath,
) -> SemanticLocalNodeToken {
    let resource = transaction.stage_geometry_path(path).unwrap();
    transaction.create_node(SemanticNodeCreation::pending_path_object(
        resource,
        SemanticTransform2_5D::default(),
        SemanticStyle::default(),
        0.0,
        SemanticObjectRole::default(),
    ))
}

fn object(store: &mut SemanticStore) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}

#[test]
fn already_positioned_reorder_reserves_a_candidate_but_commits_no_change() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let first = object(&mut store);
    let last = object(&mut store);
    store.add_member(family, first).unwrap();
    store.add_member(family, last).unwrap();
    let revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(family, first, Some(last));
    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(prepared.candidate_mutations().count(), 1);
    assert_eq!(
        prepared.proposed_scene_revision(),
        revision.checked_next().unwrap()
    );
    let committed = prepared.commit();
    assert!(committed.impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn prepare_and_drop_leave_state_revision_and_work_counters_untouched() {
    let mut store = SemanticStore::new();
    let target = object(&mut store);
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let revision = store.scene_revision();
    store.set_last_mutation_writes(7);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64);
    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.proposed_scene_revision(),
        revision.checked_next().unwrap()
    );
    assert_eq!(prepared.candidate_mutations().count(), 1);
    assert_eq!(prepared.store().scene_revision(), revision);
    assert_eq!(prepared.store().last_mutation_stats().slots_written, 7);
    assert_eq!(
        prepared
            .store()
            .semantic_object_state_checked(target)
            .unwrap(),
        &before
    );
    drop(prepared);
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before
    );
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats().slots_written, 7);
}

#[test]
fn late_invalid_prepare_preserves_prior_state_and_stats() {
    let mut store = SemanticStore::new();
    let target = object(&mut store);
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let revision = store.scene_revision();
    store.set_last_mutation_writes(3);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_property(
            target,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .set_property(target, SemanticObjectProperty::RotationZ, f64::NAN);
    assert!(transaction.prepare(&mut store).is_err());
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before
    );
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats().slots_written, 3);
}

#[test]
fn grouped_overlay_matches_commit_for_every_property_and_content_style() {
    let mut store = SemanticStore::new();
    let first = object(&mut store);
    let second = object(&mut store);
    // Unrelated authored nodes must not appear in the derived overlay.
    for _ in 0..2000 {
        object(&mut store);
    }
    let revision = store.scene_revision();
    let style = SemanticStyle {
        stroke_width: 7.0,
        ..SemanticStyle::default()
    };
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_property(
            second,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(2.0, 3.0, 4.0),
        )
        .replace_style(first, style)
        .set_property(
            second,
            SemanticObjectProperty::Scale,
            SemanticVec3::new(2.0, 3.0, 1.0),
        )
        .set_property(second, SemanticObjectProperty::RotationZ, 0.7_f64)
        .set_property(second, SemanticObjectProperty::FillOpacity, 0.2_f64)
        .set_property(second, SemanticObjectProperty::StrokeOpacity, 0.3_f64)
        .set_property(second, SemanticObjectProperty::StrokeWidth, 2.5_f64)
        .set_property(second, SemanticObjectProperty::ObjectOpacity, 0.4_f64)
        .replace_content(first, StoredGeometry::Circle { radius: 3.0 });
    let prepared = transaction.prepare(&mut store).unwrap();
    let updates: Vec<_> = prepared.object_updates().collect();
    assert_eq!(
        updates.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [second, first]
    );
    assert_eq!(prepared.candidate_mutations().count(), 9);
    assert_eq!(prepared.store().scene_revision(), revision);
    let result = prepared.commit();
    assert_eq!(result.impacts().len(), 9);
    for (id, state) in updates {
        assert_eq!(store.semantic_object_state_checked(id).unwrap(), &state);
    }
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
}

#[test]
fn exact_noops_have_no_overlay_or_revision_and_reset_committed_work() {
    let mut store = SemanticStore::new();
    let target = object(&mut store);
    let state = store.semantic_object_state_checked(target).unwrap().clone();
    let revision = store.scene_revision();
    store.set_last_mutation_writes(5);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_property(
            target,
            SemanticObjectProperty::Translation,
            state.transform.translation,
        )
        .replace_content(target, state.content)
        .replace_style(target, state.style);
    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(prepared.proposed_scene_revision(), revision);
    assert_eq!(prepared.mutations().len(), 3);
    assert_eq!(prepared.candidate_mutations().count(), 0);
    assert_eq!(prepared.object_updates().count(), 0);
    assert!(prepared.commit().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn exhausted_revision_rejects_changes_before_writes_but_accepts_noop() {
    let mut store = SemanticStore::new();
    let target = object(&mut store);
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let revision = SceneRevision::new(u64::MAX);
    store.publish_scene_revision(revision);
    store.set_last_mutation_writes(3);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64);
    assert!(matches!(
        transaction.prepare(&mut store),
        Err(SemanticMutationTransactionError::SceneRevisionExhausted)
    ));
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before
    );
    assert_eq!(store.last_mutation_stats().slots_written, 3);
    assert_eq!(store.scene_revision(), revision);
    let prepared = SemanticMutationTransaction::new()
        .prepare(&mut store)
        .unwrap();
    assert_eq!(prepared.proposed_scene_revision(), revision);
    assert!(prepared.commit().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn detached_additions_allocate_only_on_commit() {
    let mut store = SemanticStore::new();
    let before_len = store.len();
    let before_capacity = store.slot_capacity();
    let revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_node(SemanticNodeCreation::family())
        .add_node(SemanticNodeCreation::object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 2.0 },
        )));
    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(prepared.mutations().len(), 2);
    assert_eq!(prepared.candidate_mutations().count(), 2);
    assert_eq!(prepared.object_updates().count(), 0);
    assert_eq!(prepared.store().len(), before_len);
    assert_eq!(prepared.store().slot_capacity(), before_capacity);
    drop(prepared);
    assert_eq!(store.len(), before_len);
    assert_eq!(store.slot_capacity(), before_capacity);
    assert_eq!(store.scene_revision(), revision);

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_node(SemanticNodeCreation::family())
        .add_node(SemanticNodeCreation::object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 2.0 },
        )));
    let committed = transaction.prepare(&mut store).unwrap().commit();
    assert_eq!(committed.impacts().len(), 2);
    assert_eq!(store.len(), before_len + 2);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
}

#[test]
fn proposed_pending_state_matches_commit_after_canceled_creation() {
    let mut store = SemanticStore::new();
    let existing = object(&mut store);
    assert_eq!(
        store
            .semantic_object_state_checked(existing)
            .unwrap()
            .insertion_order(),
        0
    );
    let mut transaction = SemanticMutationTransaction::new();
    let canceled = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let kept = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 2.0 },
    )));
    transaction
        .set_property(kept, SemanticObjectProperty::RotationZ, 0.5_f64)
        .remove_node(canceled);
    let prepared = transaction.prepare(&mut store).unwrap();
    let proposed = prepared.proposed_object_state(kept).unwrap();
    assert_eq!(proposed.insertion_order(), 1);
    assert_eq!(proposed.transform.rotation_z, 0.5);
    let result = prepared.commit();
    assert_eq!(result.resolve(canceled), None);
    let kept = result.resolve(kept).unwrap();
    assert_eq!(
        store.semantic_object_state_checked(kept).unwrap(),
        &proposed
    );
}

#[test]
fn surviving_pending_objects_reserve_insertion_order_before_commit() {
    let mut store = SemanticStore::new();
    store.set_next_insertion_order_for_test(u64::MAX);
    let revision = store.scene_revision();
    let capacity = store.slot_capacity();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));

    assert!(matches!(
        transaction.prepare(&mut store),
        Err(SemanticMutationTransactionError::InsertionOrderExhausted)
    ));
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.slot_capacity(), capacity);
}

#[test]
fn canceled_pending_object_consumes_no_insertion_order_capacity() {
    let mut store = SemanticStore::new();
    store.set_next_insertion_order_for_test(u64::MAX);
    let mut transaction = SemanticMutationTransaction::new();
    let canceled = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    transaction.remove_node(canceled);

    let result = transaction.prepare(&mut store).unwrap().commit();
    assert_eq!(result.resolve(canceled), None);
    assert_eq!(store.next_insertion_order(), u64::MAX);
}

#[test]
fn rejected_pending_extension_never_reuses_an_escaped_local_token() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let prepared = transaction.prepare(&mut store).unwrap();
    let mut rejected = None;
    let result = prepared.with_pending_object_update(|transaction| {
        let local = transaction.create_node(SemanticNodeCreation::object(
            SemanticObjectState::new(StoredGeometry::Circle { radius: 2.0 }),
        ));
        rejected = Some(local);
        transaction.set_property(local, SemanticObjectProperty::RotationZ, f64::NAN);
    });
    let Err((prepared, error)) = result else {
        panic!("non-finite pending update must fail preflight");
    };
    assert!(matches!(
        error,
        SemanticMutationTransactionError::PendingNonFinitePropertyValue { .. }
    ));

    let escaped = rejected.expect("the rejected extension exposed one local token");
    let mut recovered = prepared.into_transaction();
    let retry = recovered.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 3.0 },
    )));
    assert_ne!(retry, escaped);

    let result = recovered.prepare(&mut store).unwrap().commit();
    assert!(result.resolve(escaped).is_none());
    assert!(result.resolve(retry).is_some());
}

#[test]
fn pending_property_coalescing_restores_the_prior_prefix_after_a_late_failure() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let local = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let prepared = transaction.prepare(&mut store).unwrap();
    let prepared = match prepared.with_pending_object_update(|transaction| {
        transaction
            .replace_pending_object_property(
                local,
                SemanticObjectProperty::Translation,
                SemanticVec3::new(1.0, 2.0, 0.0),
            )
            .replace_pending_object_style(
                local,
                SemanticStyle {
                    fill_opacity: 0.25,
                    ..SemanticStyle::default()
                },
            );
    }) {
        Ok(prepared) => prepared,
        Err(_) => panic!("finite pending update must remain valid"),
    };
    let result = prepared.with_pending_object_update(|transaction| {
        transaction
            .replace_pending_object_property(
                local,
                SemanticObjectProperty::Translation,
                SemanticVec3::new(9.0, 9.0, 0.0),
            )
            .replace_pending_object_style(
                local,
                SemanticStyle {
                    fill_opacity: 0.75,
                    ..SemanticStyle::default()
                },
            )
            .replace_pending_object_property(local, SemanticObjectProperty::RotationZ, f64::NAN);
    });
    let Err((prepared, error)) = result else {
        panic!("non-finite pending update must fail preflight");
    };
    assert!(matches!(
        error,
        SemanticMutationTransactionError::PendingNonFinitePropertyValue { .. }
    ));
    let state = prepared.proposed_object_state(local).unwrap();
    assert_eq!(
        state.transform.translation,
        SemanticVec3::new(1.0, 2.0, 0.0)
    );
    assert_eq!(state.style.fill_opacity, 0.25);
    let result = prepared.commit();
    let node = result.resolve(local).unwrap();
    let state = store.semantic_object_state_checked(node).unwrap();
    assert_eq!(
        state.transform.translation,
        SemanticVec3::new(1.0, 2.0, 0.0)
    );
    assert_eq!(state.style.fill_opacity, 0.25);
}

#[test]
fn pending_path_materializes_only_inside_final_publication_scope() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let local = pending_path_node(
        &mut transaction,
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(2.0, 1.0)),
    );
    let result = transaction
        .prepare(&mut store)
        .unwrap()
        .with_pending_geometry_paths(|prepared| Ok::<_, ()>(prepared.commit()))
        .unwrap();
    let node = result.resolve(local).unwrap();
    assert_eq!(store.geometry_resources().len(), 1);
    assert!(matches!(
        store.semantic_object_state_checked(node).unwrap().content,
        SemanticObjectContent::Geometry(StoredGeometry::Resource(handle))
            if store.geometry_resources().get(handle).is_some()
    ));
}

#[test]
fn failed_pending_path_publication_leaves_no_resource_or_semantic_state() {
    let mut store = SemanticStore::new();
    let revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    pending_path_node(
        &mut transaction,
        VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE),
    );
    let error = transaction
        .prepare(&mut store)
        .unwrap()
        .with_pending_geometry_paths(|_| Err::<(), _>("terminal publication failure"))
        .unwrap_err();
    assert!(matches!(
        error,
        PendingGeometryPublicationError::Publication("terminal publication failure")
    ));
    assert_eq!(store.geometry_resources().len(), 0);
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.len(), 0);
}

#[test]
fn pending_path_payloads_are_bounded_before_store_admission() {
    let mut transaction = SemanticMutationTransaction::new();
    for _ in 0..SemanticMutationTransaction::MAX_PENDING_GEOMETRY_RESOURCES {
        transaction.stage_geometry_path(VectorPath::new()).unwrap();
    }
    assert!(matches!(
        transaction.stage_geometry_path(VectorPath::new()),
        Err(SemanticMutationTransactionError::PendingGeometryLimitExceeded)
    ));
}

#[test]
fn pending_path_resource_extension_rollback_drops_only_its_payload_suffix() {
    let mut store = SemanticStore::new();
    let prepared = SemanticMutationTransaction::new()
        .prepare(&mut store)
        .unwrap();
    let result = prepared.with_pending_resource_object(|transaction| {
        let local = pending_path_node(transaction, VectorPath::new().move_to(Vec2::ZERO));
        transaction.set_property(local, SemanticObjectProperty::RotationZ, f64::NAN);
        Ok::<_, SemanticMutationTransactionError>(local)
    });
    let Err((prepared, error)) = result else {
        panic!("late invalid path extension must fail preflight");
    };
    assert!(matches!(
        error,
        PendingResourceExtensionError::Preflight(
            SemanticMutationTransactionError::PendingNonFinitePropertyValue { .. }
        )
    ));
    let recovered = prepared.into_transaction();
    assert_eq!(recovered.pending_resource_count(), 0);
    assert_eq!(store.geometry_resources().len(), 0);
}

#[test]
fn direct_pending_path_commit_materializes_before_store_admission() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    pending_path_node(
        &mut transaction,
        VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE),
    );
    transaction.apply(&mut store).unwrap();
    assert_eq!(store.geometry_resources().len(), 1);
}

#[test]
fn pending_path_uses_true_staged_fields_then_materializes_them() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let local = pending_path_node(
        &mut transaction,
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(4.0, 2.0)),
    );
    transaction.replace_pending_object_property(
        local,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(3.0, -2.0, 1.0),
    );
    let style = SemanticStyle {
        fill_opacity: 0.4,
        ..SemanticStyle::default()
    };
    transaction.replace_pending_object_style(local, style.clone());

    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.pending_path_transform(local).unwrap().translation,
        SemanticVec3::new(3.0, -2.0, 1.0)
    );
    assert_eq!(prepared.pending_path_style(local).unwrap(), style);
    assert_eq!(
        prepared.pending_geometry_local_bounds(local).unwrap(),
        Some(crate::Rect::new(Vec2::ZERO, Vec2::new(4.0, 2.0)))
    );
    let result = prepared
        .with_pending_geometry_paths(|prepared| Ok::<_, ()>(prepared.commit()))
        .unwrap();
    let node = result.resolve(local).unwrap();
    let state = store.semantic_object_state_checked(node).unwrap();
    assert_eq!(
        state.transform.translation,
        SemanticVec3::new(3.0, -2.0, 1.0)
    );
    assert_eq!(state.style, style);
}

#[test]
fn canceled_pending_path_never_enters_the_resource_arena() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let local = pending_path_node(
        &mut transaction,
        VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE),
    );
    transaction.remove_node(local);
    let result = transaction.prepare(&mut store).unwrap().commit();
    assert_eq!(result.resolve(local), None);
    assert_eq!(store.geometry_resources().len(), 0);
    assert_eq!(store.len(), 0);
}

#[test]
fn repeated_pending_path_staging_admits_each_live_payload_once() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let first = pending_path_node(
        &mut transaction,
        VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE),
    );
    let second = pending_path_node(
        &mut transaction,
        VectorPath::new()
            .move_to(Vec2::new(2.0, 0.0))
            .line_to(Vec2::new(3.0, 0.0)),
    );
    let result = transaction.prepare(&mut store).unwrap().commit();
    assert!(result.resolve(first).is_some());
    assert!(result.resolve(second).is_some());
    assert_eq!(store.geometry_resources().len(), 2);
}

fn pending_path_node_for_resource(
    transaction: &mut SemanticMutationTransaction,
    resource: SemanticLocalResourceToken,
) -> SemanticLocalNodeToken {
    transaction.create_node(SemanticNodeCreation::pending_path_object(
        resource,
        SemanticTransform2_5D::default(),
        SemanticStyle::default(),
        0.0,
        SemanticObjectRole::default(),
    ))
}

#[test]
fn shared_pending_path_resource_materializes_once_for_two_live_nodes() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let resource = transaction
        .stage_geometry_path(VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE))
        .unwrap();
    let first = pending_path_node_for_resource(&mut transaction, resource);
    let second = pending_path_node_for_resource(&mut transaction, resource);

    let result = transaction.prepare(&mut store).unwrap().commit();
    assert!(result.resolve(first).is_some());
    assert!(result.resolve(second).is_some());
    assert_eq!(store.geometry_resources().len(), 1);
}

#[test]
fn canceled_and_live_pending_paths_commit_directly_without_admitting_the_canceled_payload() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let canceled = pending_path_node(
        &mut transaction,
        VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE),
    );
    let live = pending_path_node(
        &mut transaction,
        VectorPath::new()
            .move_to(Vec2::new(2.0, 0.0))
            .line_to(Vec2::new(3.0, 0.0)),
    );
    transaction.remove_node(canceled);

    let result = transaction.prepare(&mut store).unwrap().commit();
    assert_eq!(result.resolve(canceled), None);
    assert!(result.resolve(live).is_some());
    assert_eq!(store.geometry_resources().len(), 1);
}

#[test]
fn canceled_and_live_pending_paths_share_the_scoped_publication_boundary() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let canceled = pending_path_node(
        &mut transaction,
        VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE),
    );
    let live = pending_path_node(
        &mut transaction,
        VectorPath::new()
            .move_to(Vec2::new(2.0, 0.0))
            .line_to(Vec2::new(3.0, 0.0)),
    );
    transaction.remove_node(canceled);

    let result = transaction
        .prepare(&mut store)
        .unwrap()
        .with_pending_geometry_paths(|prepared| Ok::<_, ()>(prepared.commit()))
        .unwrap();
    assert_eq!(result.resolve(canceled), None);
    assert!(result.resolve(live).is_some());
    assert_eq!(store.geometry_resources().len(), 1);
}

#[test]
fn rejected_pending_object_update_discards_its_fresh_path_payload_suffix() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let local = pending_path_node(
        &mut transaction,
        VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE),
    );
    transaction.replace_pending_object_property(
        local,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(1.0, 2.0, 0.0),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    let Err((prepared, _)) = prepared.with_pending_object_update(|transaction| {
        transaction
            .stage_geometry_path(VectorPath::new().move_to(Vec2::new(8.0, 0.0)))
            .unwrap();
        transaction.replace_pending_object_property(
            local,
            SemanticObjectProperty::RotationZ,
            f64::NAN,
        );
    }) else {
        panic!("non-finite extension must reject");
    };
    assert_eq!(
        prepared.pending_path_transform(local).unwrap().translation,
        SemanticVec3::new(1.0, 2.0, 0.0)
    );
    assert_eq!(prepared.into_transaction().pending_resource_count(), 1);
}

#[test]
fn rejected_resource_extension_restores_a_coalesced_prior_property() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let local = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    transaction.replace_pending_object_property(
        local,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(1.0, 0.0, 0.0),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    let result = prepared.with_pending_resource_object(|transaction| {
        transaction
            .stage_geometry_path(VectorPath::new().move_to(Vec2::ZERO))
            .map_err(|_| "resource stage")?;
        transaction.replace_pending_object_property(
            local,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(9.0, 0.0, 0.0),
        );
        Err::<(), &'static str>("reject extension")
    });
    let Err((prepared, PendingResourceExtensionError::Extension("reject extension"))) = result
    else {
        panic!("extension error must preserve the original proof");
    };
    assert_eq!(
        prepared
            .proposed_object_state(local)
            .unwrap()
            .transform
            .translation,
        SemanticVec3::new(1.0, 0.0, 0.0)
    );
    assert_eq!(prepared.into_transaction().pending_resource_count(), 0);
}

#[test]
fn pending_path_budget_counts_nested_morph_payloads_and_bounds_depth() {
    let mut transaction = SemanticMutationTransaction::new();
    let mut target = VectorPath::new();
    for _ in 0..SemanticMutationTransaction::MAX_PENDING_GEOMETRY_COMMANDS {
        target = target.line_to(Vec2::ZERO);
    }
    let path = VectorPath::new()
        .move_to(Vec2::ONE)
        .with_morph_target(target);
    assert!(matches!(
        transaction.stage_geometry_path(path),
        Err(SemanticMutationTransactionError::PendingGeometryLimitExceeded)
    ));
    assert_eq!(transaction.pending_resource_count(), 0);
    let mut nested = VectorPath::new();
    for _ in 0..SemanticMutationTransaction::MAX_PENDING_GEOMETRY_RESOURCES {
        nested = VectorPath::new().with_morph_target(nested);
    }
    assert!(matches!(
        transaction.stage_geometry_path(nested),
        Err(SemanticMutationTransactionError::PendingGeometryLimitExceeded)
    ));
    assert_eq!(transaction.pending_resource_count(), 0);
}
