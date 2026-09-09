use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{GeometryRef, ObjectId, Style, Transform2D, Vec2};
use noon_runtime::{RuntimePatchStats, SceneInstance};

const OBJECT_COUNT: usize = 100_000;
const TARGET_INDEX: usize = OBJECT_COUNT / 2;
const UNTOUCHED_INDEX: usize = TARGET_INDEX + 1;

#[test]
fn geometry_patch_touches_only_one_object_in_a_100k_scene() {
    let objects: Vec<_> = (0..OBJECT_COUNT)
        .map(|index| ObjectId::new(index as u64))
        .collect();
    let compiled_objects = objects
        .iter()
        .map(|&id| {
            CompiledObject::new(
                id,
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            )
        })
        .collect();
    let compiled = CompiledScene::compile_objects(compiled_objects, &[])
        .expect("static execution data compiles");
    let mut live = SceneInstance::new(compiled);
    live.take_frame_changes();

    let target = objects[TARGET_INDEX];
    let untouched_before = live.frame().objects[UNTOUCHED_INDEX].clone();
    let replacement = GeometryRef::line(Vec2::new(-2.0, 1.0), Vec2::new(3.0, -1.0));

    live.apply_execution_patch(&ExecutionPatch::SetContent {
        object: target,
        content: replacement.clone().into(),
        text_bounds: None,
    })
    .expect("local geometry patch must succeed");

    assert_eq!(live.last_patch_stats(), RuntimePatchStats::default());
    assert_eq!(live.take_frame_changes().object_indices(), &[TARGET_INDEX]);
    assert_eq!(
        live.frame().objects[TARGET_INDEX].geometry(),
        Some(&replacement)
    );
    assert_eq!(live.frame().objects[UNTOUCHED_INDEX], untouched_before);
    assert_eq!(live.frame().objects.len(), OBJECT_COUNT);
}
