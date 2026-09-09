use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    CompositionTimeMap, Easing, GeometryRef, ObjectId, Property, Style, TrackDefinition, TrackId,
    TrackTiming, TrackValues, Transform2D, TransformTrackEndpoint,
};
use noon_runtime::SceneInstance;

fn scene() -> CompiledScene {
    let mut objects = Vec::new();
    let mut tracks = Vec::new();
    let object = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        object,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let from = TransformTrackEndpoint {
        geometry: objects[object.get() as usize].geometry().unwrap().clone(),
        transform: objects[object.get() as usize].base_transform,
        style: objects[object.get() as usize].base_style,
    };
    let mut to = from.clone();
    to.geometry = GeometryRef::rectangle(2.0, 2.0);
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object,
        property: Property::Transform,
        values: TrackValues::Object { from, to },
        timing: TrackTiming::new(0.0, 2.0, Easing::EaseInOutCubic),
        time_map: CompositionTimeMap::identity(),
    });
    CompiledScene::compile_objects(objects, &tracks).unwrap()
}

#[test]
fn circle_to_rectangle_keeps_semantic_endpoints_and_renderer_only_morph() {
    let compiled = scene();
    let mut instance = SceneInstance::new(compiled.clone());

    let start = instance.seek(0.0).unwrap().clone();
    assert!(matches!(
        start.objects[0].geometry(),
        Some(GeometryRef::Circle { .. })
    ));
    assert!(matches!(
        start.render_geometry(0),
        Some(GeometryRef::VectorPath(_))
    ));
    assert_eq!(start.morph(0), 0.0);

    let midpoint = instance.seek(1.0).unwrap().clone();
    assert!(matches!(
        midpoint.objects[0].geometry(),
        Some(GeometryRef::Circle { .. })
    ));
    assert!(matches!(
        midpoint.render_geometry(0),
        Some(GeometryRef::VectorPath(_))
    ));
    assert!((midpoint.morph(0) - 0.5).abs() < 1e-6);

    let end = instance.seek(2.0).unwrap().clone();
    assert!(matches!(
        end.objects[0].geometry(),
        Some(GeometryRef::Rectangle { .. })
    ));
    assert!(matches!(
        end.render_geometry(0),
        Some(GeometryRef::VectorPath(_))
    ));
    assert_eq!(end.morph(0), 1.0);

    let mut sequential = SceneInstance::new(compiled);
    for step in 1..=20 {
        sequential.advance_to(step as f64 * 0.1).unwrap();
    }
    assert_eq!(sequential.frame(), &end);
}
