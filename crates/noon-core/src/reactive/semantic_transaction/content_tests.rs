use std::sync::Arc;

use super::*;
use crate::{
    FontResourceArena, GeometryResourceArena, Rect, SemanticObjectState, SemanticVec3,
    StoredGeometry, TextResource, TextResourceHandle, TextSourceKind, Vec2, VectorPath,
};

fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
}

fn rectangle(width: f32, height: f32) -> SemanticObjectContent {
    StoredGeometry::Rectangle {
        size: Vec2::new(width, height),
    }
    .into()
}

fn text_resource(source: &str) -> TextResource {
    TextResource {
        source: Arc::from(source),
        kind: TextSourceKind::Plain,
        runs: Arc::from([]),
        vector_items: Arc::from([]),
        render_items: Arc::from([]),
        parts: Arc::from([]),
        bounds: Rect::new(Vec2::ZERO, Vec2::ZERO),
        baseline: 0.0,
        layout_artifact: None,
    }
}

fn import_text(store: &mut SemanticStore, source: &str) -> TextResourceHandle {
    store
        .import_text_resource(
            text_resource(source),
            &FontResourceArena::new(),
            &GeometryResourceArena::new(),
        )
        .unwrap()
}

#[test]
fn replace_content_changes_only_content_and_preserves_authored_state() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let target = object(&mut store, 1.0);
    {
        let state = store
            .node_mut(target)
            .unwrap()
            .semantic_object_state_mut()
            .unwrap();
        state.transform.translation = SemanticVec3::new(1.0, 2.0, 3.0);
        state.style.object_opacity = 0.75;
        state.set_z_index(7);
    }
    store
        .bind_semantic_signal(signal, target, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let replacement = rectangle(4.0, 2.0);

    let mut transaction = SemanticMutationTransaction::new();
    transaction.replace_content(target, replacement);
    let result = transaction.apply(&mut store).unwrap();

    let after = store.semantic_object_state_checked(target).unwrap();
    assert_eq!(after.content, replacement);
    assert_eq!(after.transform, before.transform);
    assert_eq!(after.style, before.style);
    assert_eq!(after.presentation(), before.presentation());
    assert_eq!(after.signal_bindings(), before.signal_bindings());
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::ObjectContent { object: target }]
    );
}

#[test]
fn content_property_and_subscription_changes_share_one_object_slot() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
    let target = object(&mut store, 1.0);
    let replacement = rectangle(3.0, 5.0);

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .replace_content(target, replacement)
        .set_property(target, SemanticObjectProperty::RotationZ, 0.25_f64)
        .change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(signal));
    let result = transaction.apply(&mut store).unwrap();

    let state = store.semantic_object_state_checked(target).unwrap();
    assert_eq!(state.content, replacement);
    assert_eq!(state.transform.rotation_z, 0.25);
    assert_eq!(state.signal_bindings().len(), 1);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::ObjectContent { object: target },
            SemanticMutationImpact::ObjectProperty {
                object: target,
                property: SemanticObjectProperty::RotationZ,
            },
            SemanticMutationImpact::Subscription {
                object: target,
                property: SemanticObjectProperty::ObjectOpacity,
            },
        ]
    );
}

#[test]
fn duplicate_content_replacement_is_rejected_before_mutation() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let before = store.semantic_object_state_checked(target).unwrap().content;
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .replace_content(target, rectangle(2.0, 2.0))
        .replace_content(target, rectangle(3.0, 3.0));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateContent {
            index: 1,
            object: target,
        })
    );
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap().content,
        before
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn stale_late_content_target_rolls_back_earlier_mutations() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let live = object(&mut store, 1.0);
    let stale = object(&mut store, 2.0);
    store.remove_node(stale).unwrap();
    let replacement_node = object(&mut store, 3.0);
    assert_eq!(stale.slot(), replacement_node.slot());
    assert_ne!(stale.generation(), replacement_node.generation());
    let live_before = store.semantic_object_state_checked(live).unwrap().clone();
    let signal_before = store.semantic_signal_state(signal).unwrap().clone();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .set_property(live, SemanticObjectProperty::RotationZ, 0.5_f64)
        .replace_content(stale, rectangle(4.0, 4.0));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Object {
            index: 2,
            error: SemanticSceneOperationError::UnknownNode(stale),
        })
    );
    assert_eq!(store.semantic_signal_state(signal).unwrap(), &signal_before);
    assert_eq!(
        store.semantic_object_state_checked(live).unwrap(),
        &live_before
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn unchanged_content_is_a_noop_and_wrong_target_fails_closed() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let content = store.semantic_object_state_checked(target).unwrap().content;
    let mut unchanged = SemanticMutationTransaction::new();
    unchanged.replace_content(target, content);
    let result = unchanged.apply(&mut store).unwrap();
    assert!(result.impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let family = store.insert_family();
    let mut wrong_target = SemanticMutationTransaction::new();
    wrong_target.replace_content(family, rectangle(2.0, 2.0));
    assert_eq!(
        wrong_target.apply(&mut store),
        Err(SemanticMutationTransactionError::Object {
            index: 0,
            error: SemanticSceneOperationError::NotSemanticObject(family),
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn content_replacement_is_one_slot_local_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let target = object(&mut store, 0.5);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.replace_content(target, rectangle(8.0, 6.0));

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::ObjectContent { object: target }]
    );
}

#[test]
fn unavailable_geometry_resource_replacement_rolls_back_atomically() {
    let mut store = SemanticStore::new();
    let live = object(&mut store, 1.0);
    let earlier = object(&mut store, 2.0);
    let valid = store
        .insert_geometry_path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 0.0)),
        )
        .unwrap();
    let mut foreign_store = SemanticStore::new();
    let foreign = foreign_store
        .insert_geometry_path(VectorPath::new().move_to(Vec2::ZERO))
        .unwrap();
    let stale = crate::GeometryResourceHandle {
        version: valid.version + 1,
        ..valid
    };

    for unavailable in [foreign, stale] {
        let before = store.semantic_object_state_checked(live).unwrap().clone();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_property(earlier, SemanticObjectProperty::RotationZ, 0.5_f64)
            .replace_content(live, StoredGeometry::Resource(unavailable));

        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::InvalidGeometryResource {
                index: 1,
                resource: unavailable,
            })
        );
        assert_eq!(store.semantic_object_state_checked(live).unwrap(), &before);
        assert_eq!(
            store
                .semantic_object_state_checked(earlier)
                .unwrap()
                .transform
                .rotation_z,
            0.0
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }
}

#[test]
fn foreign_text_resource_with_matching_slot_and_version_is_rejected_for_new_node() {
    let mut store = SemanticStore::new();
    let local = import_text(&mut store, "local");
    let mut foreign_store = SemanticStore::new();
    let foreign = import_text(&mut foreign_store, "foreign");
    assert_eq!(local.id, foreign.id);
    assert_eq!(local.version, foreign.version);
    assert_ne!(local.arena, foreign.arena);
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_node(SemanticNodeCreation::object(SemanticObjectState::new(
        SemanticObjectContent::Text(foreign),
    )));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidTextResource {
            index: 0,
            resource: foreign,
        })
    );
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn unavailable_text_replacement_rolls_back_an_earlier_valid_property_write() {
    let mut store = SemanticStore::new();
    let text = import_text(&mut store, "current");
    let stale = TextResourceHandle {
        version: text.version + 1,
        ..text
    };
    let mut foreign_store = SemanticStore::new();
    let foreign = import_text(&mut foreign_store, "foreign");
    assert_eq!(text.id, foreign.id);
    assert_eq!(text.version, foreign.version);
    assert_ne!(text.arena, foreign.arena);
    let target = object(&mut store, 1.0);
    let earlier = object(&mut store, 2.0);

    for unavailable in [foreign, stale] {
        let before_target = store.semantic_object_state_checked(target).unwrap().clone();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_property(earlier, SemanticObjectProperty::RotationZ, 0.5_f64)
            .replace_content(target, SemanticObjectContent::Text(unavailable));

        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::InvalidTextResource {
                index: 1,
                resource: unavailable,
            })
        );
        assert_eq!(
            store.semantic_object_state_checked(target).unwrap(),
            &before_target
        );
        assert_eq!(
            store
                .semantic_object_state_checked(earlier)
                .unwrap()
                .transform
                .rotation_z,
            0.0
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }
}

#[test]
fn imported_same_store_text_is_valid_for_add_and_replace() {
    let mut store = SemanticStore::new();
    let first = import_text(&mut store, "first");
    let second = import_text(&mut store, "second");

    let mut add = SemanticMutationTransaction::new();
    let token = add.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        SemanticObjectContent::Text(first),
    )));
    let result = add.apply(&mut store).unwrap();
    let object = result.resolve(token).unwrap();
    assert_eq!(
        store.semantic_object_state_checked(object).unwrap().content,
        SemanticObjectContent::Text(first)
    );

    let mut replace = SemanticMutationTransaction::new();
    replace.replace_content(object, SemanticObjectContent::Text(second));
    replace.apply(&mut store).unwrap();
    assert_eq!(
        store.semantic_object_state_checked(object).unwrap().content,
        SemanticObjectContent::Text(second)
    );
}
