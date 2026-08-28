use noon_compile::CompiledScene;
use noon_core::{GeometryRef, ObjectId, SceneDefinition, ScenePatch, Transform2D, Vec2};
use noon_runtime::{RuntimePatchStats, SceneInstance};

const SCENE_OBJECTS: usize = 10_000;
const FAILURE_ITERATIONS: u64 = 1_000;
const TARGET_INDEX: usize = SCENE_OBJECTS / 2;

#[test]
fn rejected_property_churn_preserves_runtime_and_allows_recovery() {
    let mut definition = SceneDefinition::new();
    let mut objects = Vec::with_capacity(SCENE_OBJECTS);
    for _ in 0..SCENE_OBJECTS {
        objects.push(definition.add(GeometryRef::circle(0.25)));
    }

    let compiled = CompiledScene::compile(&definition).expect("large static scene must compile");
    let mut live = SceneInstance::new(compiled);
    live.take_frame_changes();

    let target = objects[TARGET_INDEX];
    let target_before = live.frame().objects[TARGET_INDEX].clone();
    let adjacent_before = live.frame().objects[TARGET_INDEX + 1].clone();
    let frame_slots = live.frame().objects.len();

    for iteration in 0..FAILURE_ITERATIONS {
        let patch = if iteration % 2 == 0 {
            ScenePatch::SetTransform {
                object: target,
                transform: Transform2D {
                    translation: Vec2::new(f32::NAN, 0.0),
                    ..Transform2D::IDENTITY
                },
            }
        } else {
            ScenePatch::SetTransform {
                object: ObjectId::new(1_000_000 + iteration),
                transform: Transform2D {
                    translation: Vec2::new(1.0, -1.0),
                    ..Transform2D::IDENTITY
                },
            }
        };

        assert!(
            live.apply_patch(&patch).is_err(),
            "iteration {iteration} must reject the invalid property edit"
        );
        assert_eq!(live.frame().objects.len(), frame_slots);
        assert_eq!(live.frame().objects[TARGET_INDEX], target_before);
        assert_eq!(live.frame().objects[TARGET_INDEX + 1], adjacent_before);
        assert_eq!(live.last_patch_stats(), RuntimePatchStats::default());
        assert!(
            live.take_frame_changes().is_empty(),
            "iteration {iteration} must not publish dirty state"
        );
    }

    let valid = ScenePatch::SetTransform {
        object: target,
        transform: Transform2D {
            translation: Vec2::new(3.0, -2.0),
            ..Transform2D::IDENTITY
        },
    };
    live.apply_patch(&valid)
        .expect("runtime must remain usable after rejected churn");

    assert_eq!(
        live.take_frame_changes().object_indices(),
        &[TARGET_INDEX],
        "recovery edit must remain object-local"
    );
    assert_eq!(
        live.frame().objects[TARGET_INDEX].transform.translation,
        Vec2::new(3.0, -2.0)
    );
    assert_eq!(live.frame().objects[TARGET_INDEX + 1], adjacent_before);
    assert_eq!(live.frame().objects.len(), frame_slots);
}
