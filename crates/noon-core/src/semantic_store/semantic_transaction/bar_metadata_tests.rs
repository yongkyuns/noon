use std::sync::Arc;

use super::*;
use crate::{Color, SemanticBarMetadata, SemanticObjectState, StoredGeometry};

fn object(store: &mut SemanticStore) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Rectangle {
        size: crate::Vec2::ONE,
    }))
}

fn metadata(value: f64) -> Arc<SemanticBarMetadata> {
    Arc::new(SemanticBarMetadata {
        value,
        original_color: Color::BLUE,
        width: 0.6,
        fill_opacity: 0.7,
        stroke_width: 3.0,
    })
}

#[test]
fn bar_metadata_is_atomic_local_and_does_not_change_the_object_role() {
    let mut store = SemanticStore::new();
    let target = object(&mut store);
    let before = store.semantic_object_state_checked(target).unwrap().clone();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_bar_metadata(target, Some(metadata(4.0)));
    let result = transaction.apply(&mut store).unwrap();

    let after = store.semantic_object_state_checked(target).unwrap();
    assert_eq!(after.bar_metadata(), Some(metadata(4.0).as_ref()));
    assert_eq!(after.role(), before.role());
    assert_eq!(after.content, before.content);
    assert_eq!(after.transform, before.transform);
    assert_eq!(after.style, before.style);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::BarMetadata { object: target }]
    );
}

#[test]
fn invalid_or_duplicate_bar_metadata_rolls_back_the_whole_transaction() {
    let mut store = SemanticStore::new();
    let target = object(&mut store);
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let mut invalid = SemanticMutationTransaction::new();
    invalid
        .set_property(target, SemanticObjectProperty::RotationZ, 0.25_f64)
        .set_bar_metadata(target, Some(metadata(f64::NAN)));

    assert_eq!(
        invalid.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidBarMetadata {
            index: 1,
            object: target.into(),
        })
    );
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut duplicate = SemanticMutationTransaction::new();
    duplicate
        .set_bar_metadata(target, Some(metadata(1.0)))
        .set_bar_metadata(target, Some(metadata(2.0)));
    assert_eq!(
        duplicate.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateBarMetadata {
            index: 1,
            object: target,
        })
    );
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}
