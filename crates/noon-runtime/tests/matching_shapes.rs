use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, ObjectSnapshot, SceneDefinition, TrackTiming, Vec2,
};
use noon_runtime::SceneInstance;

fn matching_scene() -> CompiledScene {
    let mut scene = SceneDefinition::new();
    let source_circle = scene.add(GeometryRef::circle(1.0));
    let source_rectangle = scene.add(GeometryRef::rectangle(2.0, 1.0));
    let target_circle = scene.add(GeometryRef::circle(2.0));
    let target_rectangle = scene.add(GeometryRef::rectangle(4.0, 2.0));

    scene.object_mut(target_circle).expect("target circle exists").transform.translation =
        Vec2::new(3.0, 1.0);
    scene
        .object_mut(target_rectangle)
        .expect("target rectangle exists")
        .transform
        .translation = Vec2::new(-2.0, -1.0);

    let source_circle_snapshot =
        ObjectSnapshot::from(scene.object(source_circle).expect("source circle exists"));
    let source_rectangle_snapshot = ObjectSnapshot::from(
        scene
            .object(source_rectangle)
            .expect("source rectangle exists"),
    );
    let target_circle_snapshot =
        ObjectSnapshot::from(scene.object(target_circle).expect("target circle exists"));
    let target_rectangle_snapshot = ObjectSnapshot::from(
        scene
            .object(target_rectangle)
            .expect("target rectangle exists"),
    );

    scene
        .animate_transform(
            source_circle,
            source_circle_snapshot,
            target_circle_snapshot,
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .expect("circle match transform is valid");
    scene
        .animate_transform(
            source_rectangle,
            source_rectangle_snapshot,
            target_rectangle_snapshot,
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .expect("rectangle match transform is valid");

    scene
        .set_presence_at(source_circle, true, false, 2.0)
        .expect("source circle hide is valid");
    scene
        .set_presence_at(target_circle, false, true, 2.0)
        .expect("target circle show is valid");
    scene
        .set_presence_at(source_rectangle, true, false, 2.0)
        .expect("source rectangle hide is valid");
    scene
        .set_presence_at(target_rectangle, false, true, 2.0)
        .expect("target rectangle show is valid");

    CompiledScene::compile(&scene).expect("matching-shape lowering must compile")
}

#[test]
fn simultaneous_matches_keep_sources_until_atomic_handoff() {
    let mut instance = SceneInstance::new(matching_scene());

    let before = instance.seek(0.5).expect("valid time");
    assert!(before.is_present(0));
    assert!(before.is_present(1));
    assert!(!before.is_present(2));
    assert!(!before.is_present(3));

    let middle = instance.seek(1.0).expect("valid time");
    assert!(middle.is_present(0));
    assert!(middle.is_present(1));
    assert!(!middle.is_present(2));
    assert!(!middle.is_present(3));
    assert_eq!(middle.objects[0].geometry, GeometryRef::circle(1.5));
    assert_eq!(
        middle.objects[1].geometry,
        GeometryRef::rectangle(3.0, 1.5)
    );
    assert_eq!(middle.objects[0].transform.translation, Vec2::new(1.5, 0.5));
    assert_eq!(
        middle.objects[1].transform.translation,
        Vec2::new(-1.0, -0.5)
    );

    let handoff = instance.seek(2.0).expect("valid time");
    assert!(!handoff.is_present(0));
    assert!(!handoff.is_present(1));
    assert!(handoff.is_present(2));
    assert!(handoff.is_present(3));
    assert_eq!(handoff.objects[2].geometry, GeometryRef::circle(2.0));
    assert_eq!(
        handoff.objects[3].geometry,
        GeometryRef::rectangle(4.0, 2.0)
    );
}

#[test]
fn simultaneous_matches_direct_seek_matches_forward_and_rewind() {
    let compiled = matching_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    for time in [0.25, 0.5, 1.0, 1.5, 2.0] {
        sequential.advance_to(time).expect("valid forward time");
    }
    direct.seek(2.0).expect("valid direct seek");
    assert_eq!(sequential.frame(), direct.frame());

    direct.seek(1.0).expect("valid rewind");
    assert!(direct.frame().is_present(0));
    assert!(direct.frame().is_present(1));
    assert!(!direct.frame().is_present(2));
    assert!(!direct.frame().is_present(3));

    direct.seek(2.0).expect("valid second direct seek");
    assert_eq!(sequential.frame(), direct.frame());
}
