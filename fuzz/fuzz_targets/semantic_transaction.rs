#![no_main]

use libfuzzer_sys::fuzz_target;
use noon_core::{
    SemanticMutationTransaction, SemanticObjectProperty, SemanticObjectState, SemanticStore,
    StoredGeometry,
};

fuzz_target!(|data: &[u8]| {
    // Bounded typed mutation batches exercise validation and late failure rollback.
    if data.len() > 65_536 {
        return;
    }
    let mut store = SemanticStore::new();
    let objects = [
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 1.0,
        })),
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 2.0,
        })),
    ];
    let before = objects.map(|id| store.semantic_object_state_checked(id).unwrap().clone());
    let revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    for chunk in data.chunks_exact(9) {
        let object = objects[usize::from(chunk[0] & 1)];
        let value = f64::from_le_bytes(chunk[1..].try_into().unwrap());
        let property = match (chunk[0] >> 1) % 3 {
            0 => SemanticObjectProperty::ObjectOpacity,
            1 => SemanticObjectProperty::StrokeWidth,
            _ => SemanticObjectProperty::RotationZ,
        };
        transaction.set_property(object, property, value);
    }
    if transaction.apply(&mut store).is_err() {
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(
            objects.map(|id| store.semantic_object_state_checked(id).unwrap().clone()),
            before,
            "rejected transaction partially changed authored state"
        );
    }
});
