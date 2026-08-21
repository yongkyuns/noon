use noon_compile::CompiledScene;
use noon_core::{Easing, GeometryRef, Property, SceneDefinition, TrackTiming};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

#[test]
fn appearance_multiplies_semantic_opacity_in_packed_instances() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    scene
        .object_mut(object)
        .expect("object exists")
        .style
        .opacity = 0.4;
    scene
        .animate_scalar(
            object,
            Property::Appearance,
            1.0,
            0.0,
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .expect("appearance track is valid");

    let compiled = CompiledScene::compile(&scene).expect("scene compiles");
    let mut instance = SceneInstance::new(compiled);
    instance.seek(1.0).expect("valid time");
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(instance.frame());

    assert_eq!(instance.frame().objects[0].style.opacity, 0.4);
    assert_eq!(instance.frame().objects[0].appearance, 0.5);
    assert!((prepared.circles[0].style.opacity - 0.2).abs() < 1e-6);
}
