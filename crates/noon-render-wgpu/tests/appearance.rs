use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Easing, GeometryRef, ObjectId, Property, Style, TrackDefinition, TrackId, TrackTiming,
    TrackValues, Transform2D,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

#[test]
fn appearance_multiplies_semantic_opacity_in_packed_instances() {
    let object = ObjectId::new(0);
    let compiled = CompiledScene::compile_objects(
        vec![CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style {
                opacity: 0.4,
                ..Style::default()
            },
        )],
        &[TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::Appearance,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            time_map: Default::default(),
        }],
    )
    .expect("execution data compiles");
    let mut instance = SceneInstance::new(compiled);
    instance.seek(1.0).expect("valid time");
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(instance.frame());

    assert_eq!(instance.frame().objects[0].style.opacity, 0.4);
    assert_eq!(instance.frame().objects[0].appearance, 0.5);
    assert!((prepared.circles[0].style.opacity - 0.2).abs() < 1e-6);
}
