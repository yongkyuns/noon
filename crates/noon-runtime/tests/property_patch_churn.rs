use noon_compile::CompiledScene;
use noon_core::{GeometryRef, SceneDefinition, ScenePatch, Transform2D, Vec2};
use noon_runtime::{RuntimePatchStats, SceneInstance};

const OBJECT_COUNT: usize = 10_000;
const EDIT_COUNT: usize = 1_000;
const TARGET_INDEX: usize = OBJECT_COUNT / 2;
const UNTOUCHED_INDEX: usize = TARGET_INDEX + 1;

#[test]
fn repeated_local_property_edits_stay_bounded_and_local() {
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
    let frame_capacity = live.frame().objects.len();

    for edit in 0..EDIT_COUNT {
        if edit % 2 == 0 {
            let step = (edit + 1) as f32;
            let transform = Transform2D {
                translation: Vec2::new(step * 0.01, -step * 0.005),
                ..Transform2D::IDENTITY
            };
            live.apply_patch(&ScenePatch::SetTransform {
                object: target,
                transform,
            })
            .expect("local transform patch must succeed");
            assert_eq!(live.frame().objects[TARGET_INDEX].transform, transform);
        } else {
            let mut style = live.frame().objects[TARGET_INDEX].style;
            style.opacity = 0.25 + 0.5 * ((edit % 8) as f32 / 7.0);
            live.apply_patch(&ScenePatch::SetStyle {
                object: target,
                style,
            })
            .expect("local style patch must succeed");
            assert_eq!(live.frame().objects[TARGET_INDEX].style, style);
        }

        assert_eq!(live.last_patch_stats(), RuntimePatchStats::default());
        let changes = live.take_frame_changes();
        assert!(!changes.is_all());
        assert_eq!(changes.object_indices(), &[TARGET_INDEX]);
        assert_eq!(live.frame().objects.len(), frame_capacity);
        assert_eq!(live.frame().objects[UNTOUCHED_INDEX], untouched_before);
    }

    assert_eq!(live.frame().objects.len(), OBJECT_COUNT);
}
