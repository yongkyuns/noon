use super::*;
use crate::{SceneRevision, SemanticVec3};

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
        .replace_content(
            first,
            SemanticObjectContent::Geometry(StoredGeometry::Circle { radius: 3.0 }),
        );
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
