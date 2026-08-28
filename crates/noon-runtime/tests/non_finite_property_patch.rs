use noon_compile::{CompilePatchError, CompiledScene};
use noon_core::{GeometryRef, ObjectStateField, SceneDefinition, ScenePatch, Transform2D, Vec2};
use noon_runtime::{RuntimePatchStats, SceneInstance};

#[test]
fn runtime_rejects_non_finite_transform_without_dirty_or_partial_state() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let compiled = CompiledScene::compile(&scene).expect("valid scene must compile");
    let mut live = SceneInstance::new(compiled);
    live.take_frame_changes();
    let before = live.frame().objects[0].clone();

    let error = live
        .apply_patch(&ScenePatch::SetTransform {
            object,
            transform: Transform2D {
                translation: Vec2::new(f32::NAN, 0.0),
                ..Transform2D::IDENTITY
            },
        })
        .expect_err("non-finite live transform must be rejected");
    assert_eq!(
        error,
        CompilePatchError::InvalidObjectState {
            object,
            field: ObjectStateField::Transform,
        }
    );
    assert_eq!(live.frame().objects[0], before);
    assert_eq!(live.last_patch_stats(), RuntimePatchStats::default());
    assert!(live.take_frame_changes().is_empty());

    live.apply_patch(&ScenePatch::SetTransform {
        object,
        transform: Transform2D {
            translation: Vec2::new(2.0, -1.0),
            ..Transform2D::IDENTITY
        },
    })
    .expect("runtime must remain usable after rejection");
    assert_eq!(live.take_frame_changes().object_indices(), &[0]);
}
