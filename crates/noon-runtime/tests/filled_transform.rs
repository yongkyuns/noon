use noon_compile::CompiledScene;
use noon_core::{
    Color, Easing, GeometryRef, ObjectSnapshot, SceneDefinition, TrackTiming, Transform2D, Vec2,
    VectorPath,
};
use noon_runtime::SceneInstance;

fn square() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(-1.0, 1.0))
        .close()
}

fn diamond() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, -1.4))
        .line_to(Vec2::new(1.2, 0.0))
        .line_to(Vec2::new(0.0, 1.4))
        .line_to(Vec2::new(-1.2, 0.0))
        .close()
}

fn filled_snapshot(path: VectorPath, fill: Color) -> ObjectSnapshot {
    let mut snapshot = ObjectSnapshot::new(GeometryRef::path(path));
    snapshot.transform = Transform2D::IDENTITY;
    snapshot.style.fill = Some(fill);
    snapshot.style.stroke = None;
    snapshot.style.stroke_width = 0.0;
    snapshot
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1.0e-6,
        "actual={actual}, expected={expected}"
    );
}

#[test]
fn filled_transform_seek_forward_parity_and_exact_semantic_endpoints() {
    let from = filled_snapshot(square(), Color::rgba(0.2, 0.4, 0.6, 0.8));
    let mut to = filled_snapshot(diamond(), Color::rgba(0.8, 0.6, 0.2, 0.4));
    to.transform.translation = Vec2::new(2.0, -1.0);
    to.transform.rotation = 0.6;
    to.transform.scale = Vec2::new(1.4, 0.7);

    let mut scene = SceneDefinition::new();
    let object = scene.add(from.geometry.clone());
    scene.object_mut(object).unwrap().transform = from.transform;
    scene.object_mut(object).unwrap().style = from.style;
    scene
        .animate_transform(
            object,
            from.clone(),
            to.clone(),
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let compiled = CompiledScene::compile(&scene).expect("certified filled Transform must compile");
    let mut direct = SceneInstance::new(compiled.clone());
    let mut sequential = SceneInstance::new(compiled);

    let start = direct.seek(0.0).unwrap().clone();
    assert_eq!(start.objects[0].geometry, from.geometry);
    assert_eq!(start.objects[0].style, from.style);
    assert_eq!(start.morph(0), 0.0);
    assert_ne!(start.render_geometry(0), &start.objects[0].geometry);

    sequential.advance_to(0.25).unwrap();
    sequential.advance_to(0.50).unwrap();
    sequential.advance_to(0.75).unwrap();
    sequential.advance_to(1.00).unwrap();
    direct.seek(1.00).unwrap();
    assert_eq!(sequential.frame(), direct.frame());

    let midpoint = direct.frame();
    assert_eq!(midpoint.objects[0].id, object);
    assert_eq!(midpoint.objects[0].geometry, from.geometry);
    assert_eq!(midpoint.morph(0), 0.5);
    assert_eq!(midpoint.objects[0].transform.translation, Vec2::new(1.0, -0.5));
    assert_close(midpoint.objects[0].transform.rotation, 0.3);
    assert_eq!(midpoint.objects[0].transform.scale, Vec2::new(1.2, 0.85));
    let fill = midpoint.objects[0].style.fill.expect("fill remains enabled");
    assert_close(fill.red, 0.5);
    assert_close(fill.green, 0.5);
    assert_close(fill.blue, 0.4);
    assert_close(fill.alpha, 0.6);

    let GeometryRef::VectorPath(prepared) = midpoint.render_geometry(0) else {
        panic!("filled Transform must retain one prepared path pair");
    };
    assert!(prepared.morph_target().is_some());

    let end = direct.seek(2.0).unwrap().clone();
    assert_eq!(end.objects[0].geometry, to.geometry);
    assert_eq!(end.objects[0].transform, to.transform);
    assert_eq!(end.objects[0].style, to.style);
    assert_eq!(end.morph(0), 1.0);
    assert_ne!(end.render_geometry(0), &end.objects[0].geometry);
}
