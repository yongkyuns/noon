use noon_compile::{lower_semantic_execution, SemanticExecutionIndex};
use noon_core::{
    SemanticObjectProperty, SemanticObjectState, SemanticSignalExpr, SemanticStore, StoredGeometry,
};
use noon_runtime::{ReactiveRuntimeStats, SceneInstance};

const BRANCH_COUNT: usize = 10_000;

#[test]
fn one_input_update_only_evaluates_its_reactive_branch_in_large_graph() {
    let mut scene = SemanticStore::new();
    let mut inputs = Vec::with_capacity(BRANCH_COUNT);

    for _ in 0..BRANCH_COUNT {
        let object =
            scene.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        scene.attach_to_scene(object).unwrap();
        let input = scene.insert_semantic_input_signal(0.0_f64).unwrap();
        let derived = scene
            .insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(input)),
                Box::new(SemanticSignalExpr::scalar(1.0)),
            ))
            .unwrap();
        scene
            .bind_semantic_signal(derived, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        inputs.push(input);
    }

    let target_index = BRANCH_COUNT / 2;
    let target_input = inputs[target_index];
    let lowered = lower_semantic_execution(&scene, &mut SemanticExecutionIndex::new()).unwrap();
    let target_input = lowered
        .reactive()
        .execution_signal_id(target_input)
        .unwrap();
    let mut instance = SceneInstance::from_semantic_execution(lowered);
    instance.take_frame_changes();

    instance
        .set_reactive_input(target_input, 5.0_f32)
        .expect("single reactive input update must succeed");

    assert_eq!(
        instance.frame().objects[target_index].transform.rotation,
        6.0
    );
    assert_eq!(
        instance.take_frame_changes().object_indices(),
        &[target_index]
    );
    assert_eq!(
        instance.last_reactive_stats(),
        ReactiveRuntimeStats {
            derived_signals_evaluated: 1,
            bindings_invalidated: 1,
            dense_targets_applied: 1,
            dense_targets_changed: 1,
        }
    );
}
