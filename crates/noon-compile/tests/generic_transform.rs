use noon_compile::{CompileError, CompiledScene, TransformGeometryPlan};
use noon_core::{
    Color, Easing, GeometryRef, ObjectSnapshot, Property, SceneDefinition, Style, TrackTiming,
    TrackValues, Transform2D, Vec2, VectorPath,
};

fn stroke_style() -> Style {
    Style {
        fill: None,
        stroke: Some(Color::WHITE),
        stroke_width: 0.1,
        stroke_width_mode: Default::default(),
        opacity: 1.0,
        stroke_join: noon_core::StrokeJoin::Round,
        stroke_cap: noon_core::StrokeCap::Round,
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
    let Some(TransformGeometryPlan::PathPair(GeometryRef::VectorPath(prepared))) =
        compiled_track.transform_geometry_plan.as_ref()
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
    assert!(matches!(
        compiled.tracks()[0].transform_geometry_plan,
        Some(TransformGeometryPlan::Static)
    ));
}

#[test]
fn circle_to_rectangle_transform_uses_renderer_only_path_pair() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let from = ObjectSnapshot::from(scene.object(object).unwrap());
    let mut to = from.clone();
    to.geometry = GeometryRef::rectangle(2.0, 2.0);
    let track = scene
        .animate_transform(
            object,
            from.clone(),
            to.clone(),
            TrackTiming::new(0.0, 1.0, Easing::Linear),
        )
        .unwrap();

    let compiled = CompiledScene::compile(&scene).expect("closed analytic shapes should morph");
    let compiled_track = compiled
        .tracks()
        .iter()
        .find(|candidate| candidate.id == track)
        .unwrap();
    let Some(TransformGeometryPlan::PathPair(GeometryRef::VectorPath(prepared))) =
        compiled_track.transform_geometry_plan.as_ref()
    else {
        panic!("cross-kind closed analytic Transform must use a prepared path pair");
    };
    assert!(!prepared.commands().is_empty());
    assert!(prepared.morph_target().is_some());

    let TrackValues::Object {
        from: compiled_from,
        to: compiled_to,
    } = &compiled_track.values
    else {
        panic!("Transform must retain semantic object snapshots");
    };
    assert_eq!(compiled_from, &from);
    assert_eq!(compiled_to, &to);
    assert!(matches!(compiled_from.geometry, GeometryRef::Circle { .. }));
    assert!(matches!(
        compiled_to.geometry,
        GeometryRef::Rectangle { .. }
    ));
}

#[test]
fn unsupported_open_closed_cross_geometry_transform_is_rejected_before_runtime() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let from = ObjectSnapshot::from(scene.object(object).unwrap());
    let to = snapshot(
        GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
        Style::default(),
    );
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
fn certified_closed_filled_path_transform_compiles() {
    let style = Style {
        fill: Some(Color::rgb(0.4, 0.2, 0.9)),
        stroke: Some(Color::WHITE),
        stroke_width: 0.1,
        stroke_width_mode: Default::default(),
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
        stroke_width_mode: Default::default(),
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

#[test]
fn path_transform_rejects_join_or_cap_topology_changes() {
    let source_path = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0));
    let target_path = VectorPath::new()
        .move_to(Vec2::new(0.0, -1.0))
        .line_to(Vec2::new(0.0, 1.0));
    for change_join in [true, false] {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(source_path.clone()));
        let mut from = ObjectSnapshot::from(scene.object(object).unwrap());
        from.style.fill = None;
        from.style.stroke = Some(Color::WHITE);
        from.style.stroke_width = 0.1;
        let mut to = from.clone();
        to.geometry = GeometryRef::path(target_path.clone());
        if change_join {
            to.style.stroke_join = noon_core::StrokeJoin::Bevel;
        } else {
            to.style.stroke_cap = noon_core::StrokeCap::Butt;
        }
        scene
            .animate_transform(object, from, to, TrackTiming::new(0.0, 1.0, Easing::Linear))
            .unwrap();
        assert!(matches!(
            CompiledScene::compile(&scene),
            Err(CompileError::PathTransformRequiresRetessellation(_))
        ));
    }
}
