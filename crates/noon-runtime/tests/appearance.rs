use noon_compile::CompiledScene;
use noon_core::{Easing, GeometryRef, Property, SceneDefinition, TrackTiming};
use noon_runtime::SceneInstance;

fn appearance_scene() -> CompiledScene {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    scene.object_mut(object).expect("object exists").style.opacity = 0.4;
    scene
        .animate_scalar(
            object,
            Property::Appearance,
            1.0,
            0.0,
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .expect("appearance track is valid");
    CompiledScene::compile(&scene).expect("appearance scene compiles")
}

#[test]
fn appearance_is_independent_from_semantic_style_opacity() {
    let mut instance = SceneInstance::new(appearance_scene());
    let frame = instance.seek(1.0).expect("valid time");

    assert_eq!(frame.objects[0].style.opacity, 0.4);
    assert_eq!(frame.objects[0].appearance, 0.5);
}

#[test]
fn appearance_seek_and_rewind_are_deterministic() {
    let compiled = appearance_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    sequential.advance_to(0.5).expect("valid time");
    sequential.advance_to(1.0).expect("valid time");
    sequential.advance_to(2.0).expect("valid time");
    direct.seek(2.0).expect("valid time");
    assert_eq!(sequential.frame(), direct.frame());
    assert_eq!(direct.frame().objects[0].appearance, 0.0);

    direct.seek(0.5).expect("valid rewind");
    assert_eq!(direct.frame().objects[0].appearance, 0.75);
    assert_eq!(direct.frame().objects[0].style.opacity, 0.4);
}

#[test]
fn appearance_values_are_clamped_to_normalized_visibility() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    scene
        .animate_scalar(
            object,
            Property::Appearance,
            2.0,
            -1.0,
            TrackTiming::new(0.0, 1.0, Easing::Linear),
        )
        .expect("scalar track is structurally valid");
    let compiled = CompiledScene::compile(&scene).expect("scene compiles");
    let mut instance = SceneInstance::new(compiled);

    assert_eq!(instance.seek(0.0).unwrap().objects[0].appearance, 1.0);
    assert_eq!(instance.seek(1.0).unwrap().objects[0].appearance, 0.0);
}
