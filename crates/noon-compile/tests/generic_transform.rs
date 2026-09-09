use noon_compile::CompiledObject;
use noon_compile::{CompileError, CompiledScene, TransformGeometryPlan};
use noon_core::{
    Color, GeometryRef, Property, RateFunction, Style, TrackTiming, TrackValues, Transform2D,
    TransformTrackEndpoint, Vec2, VectorPath,
};
use noon_core::{CompositionTimeMap, ObjectId, TrackDefinition, TrackId};

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

fn snapshot(geometry: GeometryRef, style: Style) -> TransformTrackEndpoint {
    TransformTrackEndpoint {
        geometry,
        transform: Transform2D::IDENTITY,
        style,
    }
}

#[test]
fn path_transform_compiles_to_one_prepared_geometry_pair() {
    let style = stroke_style();
    let mut source_objects = Vec::new();
    let mut source_tracks = Vec::new();
    let object = ObjectId::new(source_objects.len() as u64);
    source_objects.push(CompiledObject::new(
        object,
        GeometryRef::path(source_path()),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    source_objects[object.get() as usize].base_style = style;
    let track = TrackId::new(source_tracks.len() as u64);
    source_tracks.push(TrackDefinition {
        id: track,
        object,
        property: Property::Transform,
        values: TrackValues::Object {
            from: snapshot(GeometryRef::path(source_path()), style),
            to: snapshot(GeometryRef::path(target_path()), style),
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    let compiled = CompiledScene::compile_objects(source_objects, &source_tracks).unwrap();
    assert!(compiled.objects()[0].dynamic.transform);
    let compiled_track = compiled
        .tracks()
        .iter()
        .find(|candidate| candidate.id == track)
        .unwrap();
    assert_eq!(compiled_track.property, Property::Transform);
    let Some(TransformGeometryPlan::PathPair { geometry, .. }) =
        compiled_track.transform_geometry_plan.as_ref()
    else {
        panic!("path Transform must carry a prepared path pair");
    };
    let GeometryRef::VectorPath(prepared) = geometry.as_ref() else {
        panic!("prepared path");
    };
    assert_eq!(prepared.commands(), source_path().commands());
    assert_eq!(prepared.morph_target(), Some(&target_path()));
}

#[test]
fn identical_geometry_transform_needs_no_render_geometry_override() {
    let mut source_objects = Vec::new();
    let mut source_tracks = Vec::new();
    let object = ObjectId::new(source_objects.len() as u64);
    source_objects.push(CompiledObject::new(
        object,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let from = TransformTrackEndpoint {
        geometry: source_objects[object.get() as usize]
            .geometry()
            .unwrap()
            .clone(),
        transform: source_objects[object.get() as usize].base_transform,
        style: source_objects[object.get() as usize].base_style,
    };
    let mut to = from.clone();
    to.transform.translation = Vec2::new(3.0, -2.0);
    to.style.opacity = 0.25;
    source_tracks.push(TrackDefinition {
        id: TrackId::new(source_tracks.len() as u64),
        object,
        property: Property::Transform,
        values: TrackValues::Object { from, to },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    let compiled = CompiledScene::compile_objects(source_objects, &source_tracks).unwrap();
    assert!(matches!(
        compiled.tracks()[0].transform_geometry_plan,
        Some(TransformGeometryPlan::Static)
    ));
}

#[test]
fn circle_to_rectangle_transform_uses_renderer_only_path_pair() {
    let mut source_objects = Vec::new();
    let mut source_tracks = Vec::new();
    let object = ObjectId::new(source_objects.len() as u64);
    source_objects.push(CompiledObject::new(
        object,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let from = TransformTrackEndpoint {
        geometry: source_objects[object.get() as usize]
            .geometry()
            .unwrap()
            .clone(),
        transform: source_objects[object.get() as usize].base_transform,
        style: source_objects[object.get() as usize].base_style,
    };
    let mut to = from.clone();
    to.geometry = GeometryRef::rectangle(2.0, 2.0);
    let track = TrackId::new(source_tracks.len() as u64);
    source_tracks.push(TrackDefinition {
        id: track,
        object,
        property: Property::Transform,
        values: TrackValues::Object {
            from: from.clone(),
            to: to.clone(),
        },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
        .expect("closed analytic shapes should morph");
    let compiled_track = compiled
        .tracks()
        .iter()
        .find(|candidate| candidate.id == track)
        .unwrap();
    let Some(TransformGeometryPlan::PathPair { geometry, .. }) =
        compiled_track.transform_geometry_plan.as_ref()
    else {
        panic!("cross-kind closed analytic Transform must use a prepared path pair");
    };
    let GeometryRef::VectorPath(prepared) = geometry.as_ref() else {
        panic!("prepared path");
    };
    let canonical_source =
        noon_geometry::canonical_outline_path(&from.geometry).expect("canonical circle outline");
    let canonical_target =
        noon_geometry::canonical_outline_path(&to.geometry).expect("canonical rectangle outline");
    assert_eq!(prepared.commands(), canonical_source.commands());
    assert_eq!(prepared.morph_target(), Some(&canonical_target));
    assert_eq!(
        prepared.commands().len(),
        10,
        "Manim circle uses eight cubic curves"
    );
    assert_eq!(
        canonical_target.commands().len(),
        5,
        "Manim rectangle uses four edges"
    );

    let TrackValues::Object {
        from: compiled_from,
        to: compiled_to,
    } = &compiled_track.values
    else {
        panic!("Transform must retain semantic object snapshots");
    };
    for (compiled, authored) in [(compiled_from, &from), (compiled_to, &to)] {
        assert_eq!(compiled.geometry, authored.geometry);
        assert_eq!(compiled.transform, authored.transform);
        assert_eq!(compiled.style, authored.style);
    }
    assert!(matches!(compiled_from.geometry, GeometryRef::Circle { .. }));
    assert!(matches!(
        compiled_to.geometry,
        GeometryRef::Rectangle { .. }
    ));
}

#[test]
fn unsupported_open_closed_cross_geometry_transform_is_rejected_before_runtime() {
    let mut source_objects = Vec::new();
    let mut source_tracks = Vec::new();
    let object = ObjectId::new(source_objects.len() as u64);
    source_objects.push(CompiledObject::new(
        object,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let from = TransformTrackEndpoint {
        geometry: source_objects[object.get() as usize]
            .geometry()
            .unwrap()
            .clone(),
        transform: source_objects[object.get() as usize].base_transform,
        style: source_objects[object.get() as usize].base_style,
    };
    let to = snapshot(
        GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
        Style::default(),
    );
    source_tracks.push(TrackDefinition {
        id: TrackId::new(source_tracks.len() as u64),
        object,
        property: Property::Transform,
        values: TrackValues::Object { from, to },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    assert!(matches!(
        CompiledScene::compile_objects(source_objects, &source_tracks),
        Err(CompileError::UnsupportedTransformGeometry(_))
    ));
}

#[test]
fn path_stroke_width_change_is_rejected_even_when_geometry_is_identical() {
    let style = stroke_style();
    let mut source_objects = Vec::new();
    let mut source_tracks = Vec::new();
    let object = ObjectId::new(source_objects.len() as u64);
    source_objects.push(CompiledObject::new(
        object,
        GeometryRef::path(source_path()),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    source_objects[object.get() as usize].base_style = style;
    let from = snapshot(GeometryRef::path(source_path()), style);
    let mut to = from.clone();
    to.style.stroke_width = 0.2;
    source_tracks.push(TrackDefinition {
        id: TrackId::new(source_tracks.len() as u64),
        object,
        property: Property::Transform,
        values: TrackValues::Object { from, to },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    assert!(matches!(
        CompiledScene::compile_objects(source_objects, &source_tracks),
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
    let mut source_objects = Vec::new();
    let mut source_tracks = Vec::new();
    let object = ObjectId::new(source_objects.len() as u64);
    source_objects.push(CompiledObject::new(
        object,
        GeometryRef::path(source.clone()),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    source_objects[object.get() as usize].base_style = style;
    source_tracks.push(TrackDefinition {
        id: TrackId::new(source_tracks.len() as u64),
        object,
        property: Property::Transform,
        values: TrackValues::Object {
            from: snapshot(GeometryRef::path(source), style),
            to: snapshot(GeometryRef::path(target), style),
        },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
        .expect("certified filled path Transform");
    assert!(matches!(
        compiled.tracks()[0].transform_geometry_plan,
        Some(TransformGeometryPlan::PathPair { .. })
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
    let mut source_objects = Vec::new();
    let mut source_tracks = Vec::new();
    let object = ObjectId::new(source_objects.len() as u64);
    source_objects.push(CompiledObject::new(
        object,
        GeometryRef::path(source.clone()),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    source_objects[object.get() as usize].base_style = style;
    source_tracks.push(TrackDefinition {
        id: TrackId::new(source_tracks.len() as u64),
        object,
        property: Property::Transform,
        values: TrackValues::Object {
            from: snapshot(GeometryRef::path(source), style),
            to: snapshot(GeometryRef::path(bow_tie), style),
        },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    assert!(matches!(
        CompiledScene::compile_objects(source_objects, &source_tracks),
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
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::path(source_path.clone()),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let mut from = TransformTrackEndpoint {
            geometry: source_objects[object.get() as usize]
                .geometry()
                .unwrap()
                .clone(),
            transform: source_objects[object.get() as usize].base_transform,
            style: source_objects[object.get() as usize].base_style,
        };
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
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Transform,
            values: TrackValues::Object { from, to },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        assert!(matches!(
            CompiledScene::compile_objects(source_objects, &source_tracks),
            Err(CompileError::PathTransformRequiresRetessellation(_))
        ));
    }
}
