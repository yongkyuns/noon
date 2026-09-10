use noon_compile::{CompiledScene, SemanticExecutionIndex};
use noon_core::{SemanticObjectState, SemanticStore, StoredGeometry};
use noon_runtime::{ExecutionSlotTable, SceneInstance};

#[test]
fn semantic_projection_reaches_stable_execution_slots() {
    let mut store = SemanticStore::new();

    let mut first_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    first_state.set_z_index(3);
    let first = store.insert_semantic_object(first_state);
    store.attach_to_scene(first).unwrap();

    let mut second_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 2.0 });
    second_state.set_z_index(-2);
    let second = store.insert_semantic_object(second_state);
    store.attach_to_scene(second).unwrap();

    let mut index = SemanticExecutionIndex::new();
    let projection = index.lower_scene(&store).unwrap();
    let compiled = CompiledScene::from_semantic_projection(&projection).unwrap();

    let first_object = index.execution_object_id(first).unwrap();
    let second_object = index.execution_object_id(second).unwrap();
    let slots = ExecutionSlotTable::from_compiled(&compiled);
    let instance = SceneInstance::new(compiled);

    let first_slot = slots.slot_for_object(first_object).unwrap();
    let second_slot = slots.slot_for_object(second_object).unwrap();
    assert_ne!(first_slot, second_slot);
    assert_eq!(slots.len(), 2);
    assert_eq!(instance.frame_index_for_object(second_object), Some(0));
    assert_eq!(instance.frame_index_for_object(first_object), Some(1));
}
