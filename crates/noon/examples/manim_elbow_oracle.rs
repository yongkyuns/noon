//! Explicit differential-test observations from a normal typed Rust session.
use noon::{
    GeometryRef, GeometryResource, GeometryResourceLookup, ManimGeometryOptions, Mobject,
    PathCommand, Scene,
};
use serde_json::{json, Map, Value};
use std::rc::Rc;

fn observation(width: f32, angle: f32) -> Value {
    let mut scene = Scene::new();
    let elbow = Mobject::from_manim_geometry(
        Rc::clone(scene.store()),
        ManimGeometryOptions::elbow(width.into(), angle.into()).unwrap(),
    )
    .unwrap();
    scene.add(&elbow).unwrap();
    let session = scene.execution_session().unwrap();
    let transform = session.frame().render_transform(0);
    let path = match session.frame().render_geometry(0).unwrap() {
        GeometryRef::VectorPath(path) => path.clone(),
        GeometryRef::External(id) => {
            let resources = session.geometry_resources();
            let GeometryResource::VectorPath(path) = resources
                .get(resources.current_handle(*id).unwrap())
                .unwrap();
            (**path).clone()
        }
        other => panic!("expected Elbow path geometry, got {other:?}"),
    };
    let bounds = GeometryRef::path(path.clone())
        .world_bounds(transform)
        .unwrap();
    let points: Vec<_> = path
        .commands()
        .iter()
        .filter_map(|command| match command {
            PathCommand::MoveTo { to } | PathCommand::LineTo { to } => {
                Some(transform.transform_point(*to))
            }
            _ => None,
        })
        .collect();
    let start = points.first().unwrap();
    let end = points.last().unwrap();
    json!({ "center": [bounds.center().x, bounds.center().y],
        "start": [start.x, start.y], "end": [end.x, end.y],
        "width": bounds.width(), "height": bounds.height() })
}

fn main() {
    let mut observations = Map::new();
    for (name, width, angle) in [
        ("default", 0.2, 0.0),
        ("rotated_wide", 2.0, 5.0 * std::f32::consts::PI / 4.0),
        ("zero_width", 0.0, std::f32::consts::FRAC_PI_3),
        ("negative_width", -0.5, -std::f32::consts::FRAC_PI_6),
    ] {
        observations.insert(name.into(), observation(width, angle));
    }
    println!("{}", Value::Object(observations));
}
