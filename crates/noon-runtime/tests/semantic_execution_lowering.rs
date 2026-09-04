use noon_compile::{lower_semantic_execution, SemanticExecutionIndex};
use noon_core::{SemanticObjectProperty, SemanticObjectState, SemanticStore, StoredGeometry};
use noon_runtime::SceneInstance;

#[test]
fn canonical_semantic_execution_output_builds_runtime_without_recompiling_authored_scene() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 2.0,
    }));
    store.attach_to_scene(object).unwrap();
    store
        .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
        .unwrap();

    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution(&store, &mut index).unwrap();
    let execution_object = index.execution_object_id(object).unwrap();
    let execution_signal = lowered.reactive().execution_signal_id(signal).unwrap();

    let mut instance = SceneInstance::from_semantic_execution(lowered);
    assert_eq!(instance.frame().objects.len(), 1);
    assert_eq!(instance.frame().objects[0].id, execution_object);
    assert_eq!(instance.frame().objects[0].style.opacity, 0.4);

    instance.take_frame_changes();
    instance
        .set_reactive_input(execution_signal, 0.7_f32)
        .unwrap();

    assert_eq!(instance.frame().objects[0].style.opacity, 0.7);
    assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
}
