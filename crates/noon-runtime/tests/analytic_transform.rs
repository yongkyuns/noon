use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Color, CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, TransformTrackEndpoint, Vec2,
};
use noon_runtime::SceneInstance;

fn snapshot(geometry: GeometryRef, transform: Transform2D, style: Style) -> TransformTrackEndpoint {
    TransformTrackEndpoint {
        geometry,
        transform,
        style,
    }
}

fn build_scene() -> CompiledScene {
    let mut objects = Vec::new();
    let mut tracks = Vec::new();
    let style_a = Style {
        fill: Some(Color::rgb(0.2, 0.3, 0.4)),
        stroke: None,
        stroke_width: 1.0,
        stroke_width_mode: Default::default(),
        opacity: 1.0,
        stroke_join: noon_core::StrokeJoin::Round,
        stroke_cap: noon_core::StrokeCap::Round,
    };
    let style_b = Style {
        fill: Some(Color::rgb(0.8, 0.6, 0.4)),
        opacity: 0.5,
        stroke_join: noon_core::StrokeJoin::Round,
        stroke_cap: noon_core::StrokeCap::Round,
        ..style_a
    };
    let transform_a = Transform2D::IDENTITY;
    let transform_b = Transform2D {
        translation: Vec2::new(4.0, -2.0),
        rotation: 0.8,
        scale: Vec2::new(2.0, 0.5),
    };

    let circle = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        circle,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    objects[circle.get() as usize].base_style = style_a;
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: circle,
        property: Property::Transform,
        values: TrackValues::Object {
            from: snapshot(GeometryRef::circle(1.0), transform_a, style_a),
            to: snapshot(GeometryRef::circle(3.0), transform_b, style_b),
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    let rectangle = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        rectangle,
        GeometryRef::rectangle(2.0, 4.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: rectangle,
        property: Property::Transform,
        values: TrackValues::Object {
            from: snapshot(
                GeometryRef::rectangle(2.0, 4.0),
                Transform2D::IDENTITY,
                Style::default(),
            ),
            to: snapshot(
                GeometryRef::rectangle(6.0, 8.0),
                Transform2D::IDENTITY,
                Style::default(),
            ),
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    let line = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        line,
        GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: line,
        property: Property::Transform,
        values: TrackValues::Object {
            from: snapshot(
                GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
                Transform2D::IDENTITY,
                Style::default(),
            ),
            to: snapshot(
                GeometryRef::line(Vec2::new(0.0, -2.0), Vec2::new(0.0, 2.0)),
                Transform2D::IDENTITY,
                Style::default(),
            ),
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });
    CompiledScene::compile_objects(objects, &tracks).unwrap()
}

#[test]
fn analytic_transform_has_exact_endpoints_and_midpoints() {
    let mut instance = SceneInstance::new(build_scene());
    let frame = instance.seek(1.0).unwrap();

    assert_eq!(frame.objects[0].geometry(), Some(&GeometryRef::circle(2.0)));
    assert_eq!(frame.objects[0].transform.translation, Vec2::new(2.0, -1.0));
    assert_eq!(frame.objects[0].transform.rotation, 0.4);
    assert_eq!(frame.objects[0].transform.scale, Vec2::new(1.5, 0.75));
    assert!((frame.objects[0].style.opacity - 0.75).abs() < 1.0e-6);

    assert_eq!(
        frame.objects[1].geometry(),
        Some(&GeometryRef::rectangle(4.0, 6.0))
    );
    assert_eq!(
        frame.objects[2].geometry(),
        Some(&GeometryRef::line(
            Vec2::new(-0.5, -1.0),
            Vec2::new(0.5, 1.0)
        ))
    );
    assert!(frame.render_geometries.iter().all(Option::is_none));
    assert!(frame.morphs.iter().all(|value| *value == 0.0));

    let end = instance.seek(2.0).unwrap();
    assert_eq!(end.objects[0].geometry(), Some(&GeometryRef::circle(3.0)));
    assert_eq!(
        end.objects[1].geometry(),
        Some(&GeometryRef::rectangle(6.0, 8.0))
    );
    assert_eq!(
        end.objects[2].geometry(),
        Some(&GeometryRef::line(
            Vec2::new(0.0, -2.0),
            Vec2::new(0.0, 2.0)
        ))
    );
}

#[test]
fn direct_seek_and_forward_playback_match_for_analytic_transform() {
    let compiled = build_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);
    for step in 1..=13 {
        sequential.advance_to(step as f64 * 0.1).unwrap();
    }
    direct.seek(1.3).unwrap();
    assert_eq!(sequential.frame(), direct.frame());
}

#[test]
fn sequential_circle_transforms_are_continuous_at_boundary() {
    let mut objects = Vec::new();
    let mut tracks = Vec::new();
    let object = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        object,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let style = Style::default();
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object,
        property: Property::Transform,
        values: TrackValues::Object {
            from: snapshot(GeometryRef::circle(1.0), Transform2D::IDENTITY, style),
            to: snapshot(GeometryRef::circle(3.0), Transform2D::IDENTITY, style),
        },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object,
        property: Property::Transform,
        values: TrackValues::Object {
            from: snapshot(GeometryRef::circle(3.0), Transform2D::IDENTITY, style),
            to: snapshot(GeometryRef::circle(5.0), Transform2D::IDENTITY, style),
        },
        timing: TrackTiming::new(1.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    let mut instance =
        SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap());
    assert_eq!(
        instance.seek(1.0).unwrap().objects[0].geometry(),
        Some(&GeometryRef::circle(3.0))
    );
    assert_eq!(
        instance.seek(1.5).unwrap().objects[0].geometry(),
        Some(&GeometryRef::circle(4.0))
    );
}
