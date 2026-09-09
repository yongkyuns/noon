"""One-shot source edit; not shipped in the product PR."""
from pathlib import Path

p = Path('crates/noon-core/src/semantic_store/semantic_transaction.rs')
s = p.read_text()
old = 'impl std::error::Error for SemanticMutationTransactionError {}'
assert s.count(old) == 1
p.write_text(s.replace(old, '''impl std::error::Error for SemanticMutationTransactionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Signal { error, .. } => Some(error),
            Self::SignalTrack { error, .. } => Some(error),
            Self::Object { error, .. }
            | Self::Family { error, .. }
            | Self::AnimationTarget { error, .. } => Some(error),
            Self::Node { error, .. } => Some(error),
            _ => None,
        }
    }
}'''))
p = Path('crates/noon-core/src/semantic_store/semantic_transaction/prepared.rs')
s = p.read_text()
old = 'impl std::error::Error for SemanticTransactionReadError {}'
assert s.count(old) == 1
p.write_text(s.replace(old, '''impl std::error::Error for SemanticTransactionReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Existing(error) => Some(error),
            _ => None,
        }
    }
}'''))
p = Path('crates/noon-core/tests/transaction_error_causes.rs')
assert not p.exists()
p.write_text('''//! Existing transaction failures expose their typed causes without new policy.
use std::error::Error;

use noon_core::{
    SemanticMutationTransaction, SemanticMutationTransactionError as TransactionError,
    SemanticObjectProperty, SemanticObjectState, SemanticScalarSignalTrackError,
    SemanticSceneOperationError, SemanticSignalError, SemanticStore, SemanticStoreError,
    SemanticTransactionReadError, StoredGeometry,
};

fn assert_cause<T: Error + PartialEq + 'static>(error: &dyn Error, expected: &T) {
    assert_eq!(error.source().and_then(|cause| cause.downcast_ref::<T>()), Some(expected));
}

#[test]
fn every_cause_bearing_transaction_variant_returns_its_existing_error() {
    let mut store = SemanticStore::new();
    let id = store.insert_family();
    let semantic = SemanticSceneOperationError::NotSemanticObject(id);
    for error in [
        TransactionError::Object { index: 3, error: semantic.clone() },
        TransactionError::Family { index: 3, error: semantic.clone() },
        TransactionError::AnimationTarget { index: 3, error: semantic.clone() },
    ] {
        assert_cause(&error, &semantic);
    }
    let signal = TransactionError::Signal { index: 4, error: SemanticSignalError::UnknownSignal(id) };
    assert_cause(&signal, &SemanticSignalError::UnknownSignal(id));
    let track = TransactionError::SignalTrack { index: 5, error: SemanticScalarSignalTrackError::ZeroDuration(id) };
    assert_cause(&track, &SemanticScalarSignalTrackError::ZeroDuration(id));
    let node = TransactionError::Node { index: 6, error: SemanticStoreError::UnknownNode(id) };
    assert_cause(&node, &SemanticStoreError::UnknownNode(id));

    // The trait borrows the error already held by the wrapper: no cloned cause.
    let TransactionError::Node { error: inner, .. } = &node else { unreachable!() };
    assert!(std::ptr::eq(node.source().unwrap().downcast_ref::<SemanticStoreError>().unwrap(), inner));
}

#[test]
fn terminal_rejections_do_not_invent_a_cause() {
    let mut store = SemanticStore::new();
    let id = store.insert_family();
    for error in [
        TransactionError::SceneRevisionExhausted,
        TransactionError::UnknownAnimation { index: 0, animation: id },
        TransactionError::InvalidAnimationRunTime { index: 0 },
        TransactionError::InvalidStyle { index: 0, object: id },
        TransactionError::UnsupportedPropertyWrite { index: 0, object: id.into(), property: SemanticObjectProperty::RotationZ },
    ] {
        assert!(error.source().is_none());
    }
    assert!(SemanticTransactionReadError::UnknownExistingNode(id).source().is_none());
    assert!(SemanticTransactionReadError::NotFamily(id.into()).source().is_none());
}

#[test]
fn rejected_prepare_keeps_authored_state_resources_revision_and_recovery() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }));
    let family = store.insert_family();
    let before = store.semantic_object_state_checked(object).unwrap().clone();
    let revision = store.scene_revision();
    let stats = store.last_mutation_stats();
    let nodes = store.len();
    let resources = (store.geometry_resources().len(), store.text_resources().len(), store.font_resources().len());
    let mut rejected = SemanticMutationTransaction::new();
    rejected.set_property(object, SemanticObjectProperty::RotationZ, 0.5_f64)
        .set_property(family, SemanticObjectProperty::RotationZ, 1.0_f64);
    let error = rejected.prepare(&mut store).err().expect("family is not a scalar property target");
    assert!(matches!(&error, TransactionError::Object { index: 1, error: SemanticSceneOperationError::NotSemanticObject(id) } if *id == family));
    assert_cause(&error, &SemanticSceneOperationError::NotSemanticObject(family));
    assert_eq!(store.semantic_object_state_checked(object).unwrap(), &before);
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats(), stats);
    assert_eq!(store.len(), nodes);
    assert_eq!((store.geometry_resources().len(), store.text_resources().len(), store.font_resources().len()), resources);

    let mut recovery = SemanticMutationTransaction::new();
    recovery.set_property(object, SemanticObjectProperty::RotationZ, 0.5_f64);
    let result = recovery.apply(&mut store).unwrap();
    assert_eq!(result.impacts().len(), 1);
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn staged_read_cause_is_typed_and_dropping_the_view_does_not_publish() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }));
    let family = store.insert_family();
    let revision = store.scene_revision();
    let before = store.semantic_object_state_checked(object).unwrap().clone();
    let prepared = SemanticMutationTransaction::new().prepare(&mut store).unwrap();
    let error = prepared.object_state(family).unwrap_err();
    assert!(matches!(&error, SemanticTransactionReadError::Existing(SemanticSceneOperationError::NotSemanticObject(id)) if *id == family));
    assert_cause(&error, &SemanticSceneOperationError::NotSemanticObject(family));
    assert_eq!(prepared.object_state(object).unwrap(), &before);
    drop(prepared);
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.semantic_object_state_checked(object).unwrap(), &before);
}
''')
