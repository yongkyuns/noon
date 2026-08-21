use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, ObjectId, ObjectSnapshot, SceneDefinition, TrackTiming, Transform2D, Vec2,
};
use noon_runtime::SceneInstance;

fn copy_scene() -> (CompiledScene, ObjectId, ObjectId, ObjectId) {
    let mut scene = SceneDefinition::new();
    let source = scene.add(GeometryRef::circle(1.0));
    let target = scene.add(GeometryRef::circle(3.0));
    let copy = scene.add(GeometryRef::circle(1.0));

    scene.object_mut(source).expect("source exists").transform = Transform2D {
        translation: Vec2::new(-2.0, 0.0),
        ..Transform2D::IDENTITY
    };
    scene.object_mut(copy).expect("copy exists").transform = Transform2D {
        translation: Vec2::new(-2.0, 0.0),
        ..Transform2D::IDENTITY
    };
    scene.object_mut(target).expect("target exists").transform = Transform2D {
        translation: Vec2::new(4.0, -2.0),
        ..Transform2D::IDENTITY
    };

    let source_snapshot = ObjectSnapshot::from(scene.object(source).expect("source exists"));
    let target_snapshot = ObjectSnapshot::from(scene.object(target).expect("target exists"));
    scene
        .animate_transform(
            copy,
            source_snapshot,
            target_snapshot,
            TrackTiming::new(1.0, 2.0, Easing::Linear),
        )
        .expect("copy transform must be valid");
    scene
        .set_presence_at(copy, false, true, 1.0)
        .expect("copy show must be valid");
    scene
        .set_presence_at(copy, true, false, 3.0)
        .expect("copy hide must be valid");
    scene
        .set_presence_at(target, false, true, 3.0)
        .expect("target handoff must be valid");

    (
        CompiledScene::compile(&scene).expect("copy scene must compile"),
        source,
        target,
        copy,
    )
}

#[test]
fn transform_from_copy_has_exact_presence_phases() {
    let (compiled, source, target, copy) = copy_scene();
    let mut instance = SceneInstance::new(compiled);

    let before = instance.seek(0.5).expect("valid time");
    assert_eq!(before.objects[0].id, source);
    assert_eq!(before.objects[1].id, target);
    assert_eq!(before.objects[2].id, copy);
    assert!(before.is_present(0));
    assert!(!before.is_present(1));
    assert!(!before.is_present(2));

    let start = instance.seek(1.0).expect("valid time");
    assert!(start.is_present(0));
    assert!(!start.is_present(1));
    assert!(start.is_present(2));
    assert_eq!(start.objects[2].geometry, GeometryRef::circle(1.0));
    assert_eq!(start.objects[2].transform.translation, Vec2::new(-2.0, 0.0));

    let middle = instance.seek(2.0).expect("valid time");
    assert!(middle.is_present(0));
    assert!(!middle.is_present(1));
    assert!(middle.is_present(2));
    assert_eq!(middle.objects[2].geometry, GeometryRef::circle(2.0));
    assert_eq!(middle.objects[2].transform.translation, Vec2::new(1.0, -1.0));

    let end = instance.seek(3.0).expect("valid time");
    assert!(end.is_present(0));
    assert!(end.is_present(1));
    assert!(!end.is_present(2));
    assert_eq!(end.objects[1].geometry, GeometryRef::circle(3.0));
    assert_eq!(end.objects[1].transform.translation, Vec2::new(4.0, -2.0));
}

#[test]
fn transform_from_copy_direct_seek_matches_forward_playback_and_rewind() {
    let (compiled, _, _, _) = copy_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    for time in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0] {
        sequential.advance_to(time).expect("valid forward time");
    }
    direct.seek(3.0).expect("valid direct time");
    assert_eq!(sequential.frame(), direct.frame());

    direct.seek(2.0).expect("valid rewind");
    assert!(direct.frame().is_present(0));
    assert!(!direct.frame().is_present(1));
    assert!(direct.frame().is_present(2));

    direct.seek(0.5).expect("valid pre-start rewind");
    assert!(direct.frame().is_present(0));
    assert!(!direct.frame().is_present(1));
    assert!(!direct.frame().is_present(2));
}
