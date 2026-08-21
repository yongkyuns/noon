use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, ObjectSnapshot, SceneDefinition, TrackTiming, Transform2D, Vec2,
};
use noon_runtime::SceneInstance;

fn replacement_scene() -> (CompiledScene, noon_core::ObjectId, noon_core::ObjectId) {
    let mut scene = SceneDefinition::new();
    let source = scene.add(GeometryRef::circle(1.0));
    let target = scene.add(GeometryRef::circle(3.0));

    scene.object_mut(target).expect("target exists").transform = Transform2D {
        translation: Vec2::new(4.0, -2.0),
        ..Transform2D::IDENTITY
    };

    let source_snapshot = ObjectSnapshot::from(scene.object(source).expect("source exists"));
    let target_snapshot = ObjectSnapshot::from(scene.object(target).expect("target exists"));

    scene
        .animate_transform(
            source,
            source_snapshot,
            target_snapshot,
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .expect("replacement transform must be valid");
    scene
        .set_presence_at(source, true, false, 2.0)
        .expect("source handoff must be valid");
    scene
        .set_presence_at(target, false, true, 2.0)
        .expect("target handoff must be valid");

    (
        CompiledScene::compile(&scene).expect("replacement scene must compile"),
        source,
        target,
    )
}

#[test]
fn replacement_transform_has_exact_stable_identity_handoff() {
    let (compiled, source, target) = replacement_scene();
    let mut instance = SceneInstance::new(compiled);

    let before = instance.seek(0.0).expect("valid time");
    assert_eq!(before.objects.len(), 2);
    assert_eq!(before.objects[0].id, source);
    assert_eq!(before.objects[1].id, target);
    assert!(before.is_present(0));
    assert!(!before.is_present(1));

    let middle = instance.seek(1.0).expect("valid time");
    assert!(middle.is_present(0));
    assert!(!middle.is_present(1));
    assert_eq!(
        middle.objects[0].geometry,
        GeometryRef::circle(2.0),
        "source identity carries interpolated replacement geometry"
    );
    assert_eq!(middle.objects[0].transform.translation, Vec2::new(2.0, -1.0));

    let handoff = instance.seek(2.0).expect("valid time");
    assert!(!handoff.is_present(0));
    assert!(handoff.is_present(1));
    assert_eq!(handoff.objects[0].id, source);
    assert_eq!(handoff.objects[1].id, target);
    assert_eq!(handoff.objects[1].geometry, GeometryRef::circle(3.0));
    assert_eq!(handoff.objects[1].transform.translation, Vec2::new(4.0, -2.0));
}

#[test]
fn replacement_transform_direct_seek_matches_forward_playback_and_rewinds() {
    let (compiled, _, _) = replacement_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    for time in [0.25, 0.5, 1.0, 1.5, 2.0] {
        sequential.advance_to(time).expect("valid forward time");
    }
    direct.seek(2.0).expect("valid direct time");
    assert_eq!(sequential.frame(), direct.frame());

    direct.seek(0.75).expect("valid rewind");
    assert!(direct.frame().is_present(0));
    assert!(!direct.frame().is_present(1));

    direct.seek(2.0).expect("valid second direct seek");
    assert!(!direct.frame().is_present(0));
    assert!(direct.frame().is_present(1));
    assert_eq!(sequential.frame(), direct.frame());
}
