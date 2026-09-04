use noon_compile::{CompiledScene, SemanticExecutionIndex};
use noon_core::{SemanticObjectState, SemanticStore, StoredGeometry};
use noon_runtime::SlottedSceneInstance;

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
    let instance = SlottedSceneInstance::new(compiled);

    let first_slot = instance.slot_for_object(first_object).unwrap();
    let second_slot = instance.slot_for_object(second_object).unwrap();
    assert_ne!(first_slot, second_slot);
    assert_eq!(instance.live_object_count(), 2);
    assert_eq!(instance.frame_index_for_slot(second_slot), Some(0));
    assert_eq!(instance.frame_index_for_slot(first_slot), Some(1));
}
