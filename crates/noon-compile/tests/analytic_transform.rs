use noon_compile::{CompiledObject, CompiledScene, TransformGeometryPlan};
use noon_core::{
    CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition,
    TrackId, TrackTiming, TrackValues, Transform2D, TransformTrackEndpoint, Vec2,
};

#[test]
fn compiler_selects_analytic_geometry_plans() {
    let pairs = [
        (GeometryRef::circle(1.0), GeometryRef::circle(3.0)),
        (
            GeometryRef::rectangle(2.0, 4.0),
            GeometryRef::rectangle(6.0, 8.0),
        ),
        (
            GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
            GeometryRef::line(Vec2::new(0.0, -2.0), Vec2::new(0.0, 2.0)),
        ),
    ];
    let mut objects = Vec::new();
    let mut tracks = Vec::new();
    for (index, (from, to)) in pairs.into_iter().enumerate() {
        let object = ObjectId::new(index as u64);
        objects.push(CompiledObject::new(
            object,
            from.clone(),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(index as u64),
            object,
            property: Property::Transform,
            values: TrackValues::Object {
                from: TransformTrackEndpoint::new(from),
                to: TransformTrackEndpoint::new(to),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
    }
    let compiled = CompiledScene::compile_objects(objects, &tracks).unwrap();
    assert!(matches!(
        compiled.tracks()[0].transform_geometry_plan,
        Some(TransformGeometryPlan::Circle {
            from_radius: 1.0,
            to_radius: 3.0
        })
    ));
    assert!(matches!(
        compiled.tracks()[1].transform_geometry_plan,
        Some(TransformGeometryPlan::Rectangle {
            from_size: Vec2 { x: 2.0, y: 4.0 },
            to_size: Vec2 { x: 6.0, y: 8.0 }
        })
    ));
    assert!(matches!(
        compiled.tracks()[2].transform_geometry_plan,
        Some(TransformGeometryPlan::Line {
            from_start: Vec2 { x: -1.0, y: 0.0 },
            from_end: Vec2 { x: 1.0, y: 0.0 },
            to_start: Vec2 { x: 0.0, y: -2.0 },
            to_end: Vec2 { x: 0.0, y: 2.0 }
        })
    ));
}
