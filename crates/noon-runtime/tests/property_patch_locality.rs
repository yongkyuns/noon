use noon_compile::CompiledScene;
use noon_core::{GeometryRef, SceneDefinition, ScenePatch, Transform2D, Vec2};
use noon_runtime::SceneInstance;

const OBJECT_COUNT: usize = 100_000;
const TARGET_INDEX: usize = OBJECT_COUNT / 2;
const UNTOUCHED_INDEX: usize = TARGET_INDEX + 1;

#[test]
fn property_patches_touch_only_one_object_in_a_100k_scene() {
    let mut definition = SceneDefinition::new();
    let mut objects = Vec::with_capacity(OBJECT_COUNT);
    for _ in 0..OBJECT_COUNT {
        objects.push(definition.add(GeometryRef::circle(1.0)));
    }

    let compiled = CompiledScene::compile(&definition).expect("large static scene must compile");
    let mut live = SceneInstance::new(compiled);
    live.take_frame_changes();

    let target = objects[TARGET_INDEX];
    let untouched_before = live.frame().objects[UNTOUCHED_INDEX].clone();
    let transform = Transform2D {
        translation: Vec2::new(3.0, -2.0),
        ..Transform2D::IDENTITY
    };
    live.apply_patch(&ScenePatch::SetTransform {
        object: target,
        transform,
    })
    .expect("local transform patch must succeed");

    let transform_stats = live.last_patch_stats();
    assert_eq!(transform_stats.channels_relowered, 0);
    assert_eq!(transform_stats.scheduler_events_removed, 0);
    assert_eq!(transform_stats.scheduler_events_inserted, 0);
    assert_eq!(transform_stats.object_slots_appended, 0);
    assert_eq!(transform_stats.object_slots_retired, 0);
    assert_eq!(transform_stats.track_locators_removed, 0);
    assert_eq!(transform_stats.full_group_rebuilds, 0);
    assert_eq!(transform_stats.full_seeks, 0);
    assert_eq!(live.take_frame_changes().object_indices(), &[TARGET_INDEX]);
    assert_eq!(live.frame().objects[TARGET_INDEX].transform, transform);
    assert_eq!(live.frame().objects[UNTOUCHED_INDEX], untouched_before);

    let mut style = live.frame().objects[TARGET_INDEX].style;
    style.opacity = 0.25;
    live.apply_patch(&ScenePatch::SetStyle {
        object: target,
        style,
    })
    .expect("local style patch must succeed");

    let style_stats = live.last_patch_stats();
    assert_eq!(style_stats.channels_relowered, 0);
    assert_eq!(style_stats.scheduler_events_removed, 0);
    assert_eq!(style_stats.scheduler_events_inserted, 0);
    assert_eq!(style_stats.object_slots_appended, 0);
    assert_eq!(style_stats.object_slots_retired, 0);
    assert_eq!(style_stats.track_locators_removed, 0);
    assert_eq!(style_stats.full_group_rebuilds, 0);
    assert_eq!(style_stats.full_seeks, 0);
    assert_eq!(live.take_frame_changes().object_indices(), &[TARGET_INDEX]);
    assert_eq!(live.frame().objects[TARGET_INDEX].style, style);
    assert_eq!(live.frame().objects[UNTOUCHED_INDEX], untouched_before);
}
