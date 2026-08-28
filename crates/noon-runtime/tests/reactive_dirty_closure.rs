use noon_core::{GeometryRef, Property, ReactiveExpr, SemanticScene};
use noon_runtime::{ReactiveRuntimeStats, SceneInstance};

const BRANCH_COUNT: usize = 10_000;

#[test]
fn one_input_update_only_evaluates_its_reactive_branch_in_large_graph() {
    let mut scene = SemanticScene::new();
    let mut inputs = Vec::with_capacity(BRANCH_COUNT);

    for _ in 0..BRANCH_COUNT {
        let object = scene.add(GeometryRef::circle(1.0));
        let input = scene.add_input(0.0_f32);
        let derived = scene.add_derived(ReactiveExpr::Add(
            Box::new(ReactiveExpr::signal(input)),
            Box::new(ReactiveExpr::scalar(1.0)),
        ));
        scene.bind(derived, object, Property::Rotation);
        inputs.push(input);
    }

    let target_index = BRANCH_COUNT / 2;
    let target_input = inputs[target_index];
    let mut instance =
        SceneInstance::from_semantic(&scene).expect("large reactive graph must compile");
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
