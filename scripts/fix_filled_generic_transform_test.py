from pathlib import Path

path = Path("crates/noon-compile/tests/generic_transform.rs")
text = path.read_text()
old = r'''#[test]
fn filled_geometry_changing_path_transform_is_rejected() {
    let style = Style {
        fill: Some(Color::rgb(0.4, 0.2, 0.9)),
        stroke: Some(Color::WHITE),
        stroke_width: 0.1,
        opacity: 1.0,
        stroke_join: noon_core::StrokeJoin::Round,
        stroke_cap: noon_core::StrokeCap::Round,
    };
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::path(source_path()));
    scene.object_mut(object).unwrap().style = style;
    scene
        .animate_transform(
            object,
            snapshot(GeometryRef::path(source_path()), style),
            snapshot(GeometryRef::path(target_path()), style),
            TrackTiming::new(0.0, 1.0, Easing::Linear),
        )
        .unwrap();

    assert!(matches!(
        CompiledScene::compile(&scene),
        Err(CompileError::PathTransformRequiresRetessellation(_))
    ));
}
'''
new = r'''#[test]
fn certified_closed_filled_path_transform_compiles() {
    let style = Style {
        fill: Some(Color::rgb(0.4, 0.2, 0.9)),
        stroke: Some(Color::WHITE),
        stroke_width: 0.1,
        opacity: 1.0,
        stroke_join: noon_core::StrokeJoin::Round,
        stroke_cap: noon_core::StrokeCap::Round,
    };
    let source = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(-1.0, 1.0))
        .close();
    let target = VectorPath::new()
        .move_to(Vec2::new(0.0, -1.4))
        .line_to(Vec2::new(1.2, 0.0))
        .line_to(Vec2::new(0.0, 1.4))
        .line_to(Vec2::new(-1.2, 0.0))
        .close();
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::path(source.clone()));
    scene.object_mut(object).unwrap().style = style;
    scene
        .animate_transform(
            object,
            snapshot(GeometryRef::path(source), style),
            snapshot(GeometryRef::path(target), style),
            TrackTiming::new(0.0, 1.0, Easing::Linear),
        )
        .unwrap();

    let compiled = CompiledScene::compile(&scene).expect("certified filled path Transform");
    assert!(matches!(
        compiled.tracks()[0].transform_geometry_plan,
        Some(TransformGeometryPlan::PathPair(_))
    ));
}

#[test]
fn unsafe_filled_path_transform_is_rejected_before_runtime() {
    let style = Style {
        fill: Some(Color::rgb(0.4, 0.2, 0.9)),
        stroke: Some(Color::WHITE),
        stroke_width: 0.1,
        opacity: 1.0,
        stroke_join: noon_core::StrokeJoin::Round,
        stroke_cap: noon_core::StrokeCap::Round,
    };
    let source = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(-1.0, 1.0))
        .close();
    let bow_tie = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(-1.0, 1.0))
        .close();
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::path(source.clone()));
    scene.object_mut(object).unwrap().style = style;
    scene
        .animate_transform(
            object,
            snapshot(GeometryRef::path(source), style),
            snapshot(GeometryRef::path(bow_tie), style),
            TrackTiming::new(0.0, 1.0, Easing::Linear),
        )
        .unwrap();

    assert!(matches!(
        CompiledScene::compile(&scene),
        Err(CompileError::UnsafeFilledPathTransform(_))
    ));
}
'''
if old not in text:
    raise SystemExit("legacy filled path rejection test marker missing")
path.write_text(text.replace(old, new, 1))
