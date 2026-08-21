use noon_compile::{CompileError, CompiledScene};
use noon_core::{
    Color, Easing, GeometryRef, ObjectSnapshot, Property, SceneDefinition, Style, TrackTiming,
    Transform2D, Vec2, VectorPath,
};

fn stroke_style() -> Style {
    Style {
        fill: None,
        stroke: Some(Color::WHITE),
        stroke_width: 0.1,
        opacity: 1.0,
    }
}

fn source_path() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0))
}

fn target_path() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, -1.0))
        .line_to(Vec2::new(0.0, 1.0))
}

fn snapshot(geometry: GeometryRef, style: Style) -> ObjectSnapshot {
    ObjectSnapshot {
        geometry,
        transform: Transform2D::IDENTITY,
        style,
    }
}

#[test]
fn path_transform_compiles_to_one_prepared_geometry_pair() {
    let style = stroke_style();
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::path(source_path()));
    scene.object_mut(object).unwrap().style = style;
    let track = scene
        .animate_transform(
            object,
            snapshot(GeometryRef::path(source_path()), style),
            snapshot(GeometryRef::path(target_path()), style),
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let compiled = CompiledScene::compile(&scene).unwrap();
    assert!(compiled.objects()[0].dynamic.transform);
    let compiled_track = compiled
        .tracks()
        .iter()
        .find(|candidate| candidate.id == track)
        .unwrap();
    assert_eq!(compiled_track.property, Property::Transform);
    let GeometryRef::VectorPath(prepared) = compiled_track.transform_geometry.as_ref().unwrap()
    else {
        panic!("path Transform must carry a prepared path pair");
    };
    assert_eq!(prepared.commands(), source_path().commands());
    assert_eq!(prepared.morph_target(), Some(&target_path()));
}

#[test]
fn identical_geometry_transform_needs_no_render_geometry_override() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let from = ObjectSnapshot::from(scene.object(object).unwrap());
    let mut to = from.clone();
    to.transform.translation = Vec2::new(3.0, -2.0);
    to.style.opacity = 0.25;
    scene
        .animate_transform(object, from, to, TrackTiming::new(0.0, 1.0, Easing::Linear))
        .unwrap();

    let compiled = CompiledScene::compile(&scene).unwrap();
    assert!(compiled.tracks()[0].transform_geometry.is_none());
}

#[test]
fn unsupported_cross_geometry_transform_is_rejected_before_runtime() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let from = ObjectSnapshot::from(scene.object(object).unwrap());
    let to = snapshot(GeometryRef::rectangle(2.0, 2.0), Style::default());
    scene
        .animate_transform(object, from, to, TrackTiming::new(0.0, 1.0, Easing::Linear))
        .unwrap();

    assert!(matches!(
        CompiledScene::compile(&scene),
        Err(CompileError::UnsupportedTransformGeometry(_))
    ));
}

#[test]
fn path_stroke_width_change_is_rejected_even_when_geometry_is_identical() {
    let style = stroke_style();
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::path(source_path()));
    scene.object_mut(object).unwrap().style = style;
    let from = snapshot(GeometryRef::path(source_path()), style);
    let mut to = from.clone();
    to.style.stroke_width = 0.2;
    scene
        .animate_transform(object, from, to, TrackTiming::new(0.0, 1.0, Easing::Linear))
        .unwrap();

    assert!(matches!(
        CompiledScene::compile(&scene),
        Err(CompileError::PathTransformRequiresRetessellation(_))
    ));
}

#[test]
fn filled_geometry_changing_path_transform_is_rejected() {
    let style = Style {
        fill: Some(Color::rgb(0.4, 0.2, 0.9)),
        stroke: Some(Color::WHITE),
        stroke_width: 0.1,
        opacity: 1.0,
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
