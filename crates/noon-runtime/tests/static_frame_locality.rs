use noon_compile::CompiledScene;
use noon_core::{GeometryRef, SceneDefinition};
use noon_runtime::{EvaluationStats, SceneInstance};

const STATIC_OBJECTS: usize = 100_000;

#[test]
fn hundred_thousand_static_objects_do_zero_timeline_work_on_unchanged_frame() {
    let mut scene = SceneDefinition::new();
    for _ in 0..STATIC_OBJECTS {
        scene.add(GeometryRef::circle(1.0));
    }

    let compiled = CompiledScene::compile(&scene).expect("static scene must compile");
    let mut runtime = SceneInstance::new(compiled);
    assert_eq!(runtime.frame().objects.len(), STATIC_OBJECTS);
    assert!(runtime.take_frame_changes().is_all());

    runtime
        .advance_to(1.0)
        .expect("finite forward time must evaluate");

    assert_eq!(runtime.last_stats(), EvaluationStats::default());
    assert!(runtime.take_frame_changes().is_empty());
}
