use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, ObjectSnapshot, SceneDefinition, TrackTiming, Transform2D, Vec2,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

fn copy_scene() -> SceneInstance {
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
        .expect("target show must be valid");

    SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"))
}

fn prepared_ids(instance: &mut SceneInstance, preparer: &mut FramePreparer, time: f64) -> Vec<u64> {
    instance.advance_to(time).expect("valid time");
    let changes = instance.take_frame_changes();
    let prepared = preparer.prepare_incremental(instance.frame(), &changes);
    prepared.circle_ids.iter().map(|id| id.get()).collect()
}

#[test]
fn renderer_tracks_transform_from_copy_visible_instance_phases() {
    let mut instance = copy_scene();
    let mut preparer = FramePreparer::new();

    assert_eq!(prepared_ids(&mut instance, &mut preparer, 0.5), vec![0]);
    assert_eq!(prepared_ids(&mut instance, &mut preparer, 1.0), vec![0, 2]);
    assert_eq!(prepared_ids(&mut instance, &mut preparer, 2.0), vec![0, 2]);
    assert_eq!(prepared_ids(&mut instance, &mut preparer, 3.0), vec![0, 1]);
}
