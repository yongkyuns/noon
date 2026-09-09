use super::*;
use crate::{SemanticNodeResidency, SourceIdentity, StoredGeometry, Vec2, VectorPath};

fn object_state(radius: f32) -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Circle { radius })
}

fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    store.insert_semantic_object(object_state(radius))
}

fn scalar_input(store: &SemanticStore, signal: SemanticNodeId) -> f64 {
    let SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) =
        store.semantic_signal_state(signal).unwrap().source()
    else {
        panic!("expected scalar input signal")
    };
    *value
}

#[test]
fn add_node_allocates_detached_object_and_family_and_reports_real_identities() {
    let mut store = SemanticStore::new();
    let root_count = store.scene_root_count();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_node(SemanticNodeCreation::object(object_state(1.0)))
        .add_node(SemanticNodeCreation::family());
    let result = transaction.apply(&mut store).unwrap();

    let [SemanticMutationImpact::NodeAdded { node: object }, SemanticMutationImpact::NodeAdded { node: family }] =
        result.impacts()
    else {
        panic!("expected two node-added impacts")
    };
    assert!(store.semantic_object_state_checked(*object).is_ok());
    assert!(matches!(
        store.node(*family).unwrap().kind(),
        SemanticNodeKind::Family
    ));
    assert_eq!(
        store.node(*object).unwrap().residency(),
        SemanticNodeResidency::Detached
    );
    assert_eq!(
        store.node(*family).unwrap().residency(),
        SemanticNodeResidency::Detached
    );
    assert_eq!(store.scene_root_count(), root_count);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}

#[test]
fn repeated_identical_add_node_mutations_allocate_distinct_identities() {
    let mut store = SemanticStore::new();
    let state = object_state(1.0);

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_node(SemanticNodeCreation::object(state.clone()))
        .add_node(SemanticNodeCreation::object(state));
    let result = transaction.apply(&mut store).unwrap();

    let [SemanticMutationImpact::NodeAdded { node: first }, SemanticMutationImpact::NodeAdded { node: second }] =
        result.impacts()
    else {
        panic!("expected two node-added impacts")
    };
    assert_ne!(first, second);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}

#[test]
fn invalid_object_state_rolls_back_earlier_mutation_before_allocation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let before_len = store.len();
    let mut invalid = object_state(1.0);
    invalid.transform.rotation_z = f64::NAN;

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .add_node(SemanticNodeCreation::object(invalid));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidNodeObjectState { index: 1 })
    );
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn unavailable_geometry_resource_is_rejected_before_node_allocation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
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
        let before_len = store.len();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(signal, 2.0_f64)
            .add_node(SemanticNodeCreation::object(SemanticObjectState::new(
                StoredGeometry::Resource(unavailable),
            )));

        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::InvalidGeometryResource {
                index: 1,
                resource: unavailable,
            })
        );
        assert_eq!(scalar_input(&store, signal), 1.0);
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }
}

#[test]
fn cloned_object_with_stale_binding_is_rejected_before_allocation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let source = object(&mut store, 1.0);
    store
        .bind_semantic_signal(signal, source, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    let cloned = store.semantic_object_state_checked(source).unwrap().clone();
    store.remove_node(signal).unwrap();
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_node(SemanticNodeCreation::object(cloned));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Signal {
            index: 0,
            error: SemanticSignalError::UnknownSignal(signal),
        })
    );
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn new_object_cannot_bind_signal_removed_by_same_transaction() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let source = object(&mut store, 1.0);
    store
        .bind_semantic_signal(signal, source, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    let cloned = store.semantic_object_state_checked(source).unwrap().clone();
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_node(SemanticNodeCreation::object(cloned))
        .remove_node(signal);

    assert_eq!(
        transaction.apply(&mut store),
        Err(
            SemanticMutationTransactionError::NodeCreationUsesRemovedNode {
                index: 0,
                node: signal,
            }
        )
    );
    assert_eq!(store.len(), before_len);
    assert!(store.node(signal).is_some());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn duplicate_pending_source_identity_rolls_back_every_creation() {
    let mut store = SemanticStore::new();
    let source = SourceIdentity::ExplicitKey("same".to_owned());
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_node(
            SemanticNodeCreation::object(object_state(1.0)).with_source_identity(source.clone()),
        )
        .add_node(SemanticNodeCreation::family().with_source_identity(source.clone()));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Node {
            index: 1,
            error: SemanticStoreError::DuplicateSourceIdentity(source),
        })
    );
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn source_identity_can_move_atomically_from_removed_node_to_replacement() {
    let mut store = SemanticStore::new();
    let source = SourceIdentity::ExplicitKey("stable-source".to_owned());
    let old = object(&mut store, 1.0);
    store
        .set_source_identity(old, Some(source.clone()))
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_node(
            SemanticNodeCreation::object(object_state(2.0)).with_source_identity(source.clone()),
        )
        .remove_node(old);
    let result = transaction.apply(&mut store).unwrap();

    let [SemanticMutationImpact::NodeAdded { node: replacement }, SemanticMutationImpact::NodeRemoved { node }] =
        result.impacts()
    else {
        panic!("expected replacement creation followed by old-node removal")
    };
    assert_eq!(*node, old);
    assert!(store.node(old).is_none());
    assert_eq!(store.node_for_source(&source), Some(*replacement));
    assert_eq!(
        store.node(*replacement).unwrap().residency(),
        SemanticNodeResidency::Detached
    );
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}

#[test]
fn add_node_is_local_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_node(SemanticNodeCreation::object(object_state(0.25)));
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(result.impacts().len(), 1);
    assert!(matches!(
        result.impacts()[0],
        SemanticMutationImpact::NodeAdded { .. }
    ));
    assert_eq!(store.len(), before_len + 1);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn invalid_paint_and_inline_geometry_fail_before_any_node_is_published() {
    let mut store = SemanticStore::new();
    let mut invalid_paint = object_state(1.0);
    invalid_paint.style.fill = Some(crate::SemanticPaint::Solid(crate::Color {
        red: f32::NAN,
        ..crate::Color::BLUE
    }));
    for state in [
        invalid_paint,
        object_state(f32::NAN),
        SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(1.0, f32::INFINITY),
        }),
        SemanticObjectState::new(StoredGeometry::Line {
            start: Vec2::ZERO,
            end: Vec2::new(f32::NAN, 0.0),
        }),
    ] {
        let revision = store.scene_revision();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .add_node(SemanticNodeCreation::object(object_state(1.0)))
            .add_node(SemanticNodeCreation::object(state));
        assert!(transaction.apply(&mut store).is_err());
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.len(), 0);
    }
}
