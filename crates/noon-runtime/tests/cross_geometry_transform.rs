use noon_compile::CompiledScene;
use noon_core::{Easing, GeometryRef, ObjectSnapshot, SceneDefinition, TrackTiming};
use noon_runtime::SceneInstance;

fn scene() -> SceneDefinition {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let from = ObjectSnapshot::from(scene.object(object).unwrap());
    let mut to = from.clone();
    to.geometry = GeometryRef::rectangle(2.0, 2.0);
    scene
        .animate_transform(
            object,
            from,
            to,
            TrackTiming::new(0.0, 2.0, Easing::EaseInOutCubic),
        )
        .unwrap();
    scene
}

#[test]
fn circle_to_rectangle_keeps_semantic_endpoints_and_renderer_only_morph() {
    let compiled = CompiledScene::compile(&scene()).expect("cross-kind Transform must compile");
    let mut instance = SceneInstance::new(compiled.clone());

    let start = instance.seek(0.0).unwrap().clone();
    assert!(matches!(start.objects[0].geometry, GeometryRef::Circle { .. }));
    assert!(matches!(start.render_geometry(0), GeometryRef::VectorPath(_)));
    assert_eq!(start.morph(0), 0.0);

    let midpoint = instance.seek(1.0).unwrap().clone();
    assert!(matches!(
        midpoint.objects[0].geometry,
        GeometryRef::Circle { .. }
    ));
    assert!(matches!(
        midpoint.render_geometry(0),
        GeometryRef::VectorPath(_)
    ));
    assert!((midpoint.morph(0) - 0.5).abs() < 1e-6);

    let end = instance.seek(2.0).unwrap().clone();
    assert!(matches!(
        end.objects[0].geometry,
        GeometryRef::Rectangle { .. }
    ));
    assert!(matches!(end.render_geometry(0), GeometryRef::VectorPath(_)));
    assert_eq!(end.morph(0), 1.0);

    let mut sequential = SceneInstance::new(compiled);
    for step in 1..=20 {
        sequential.advance_to(step as f64 * 0.1).unwrap();
    }
    assert_eq!(sequential.frame(), &end);
}
