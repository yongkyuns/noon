use noon::legacy::{Elbow, IntoSnapshot};
use noon_core::{GeometryRef, ObjectSnapshot, PathCommand, Vec2};
use serde_json::{json, Map, Value};

fn transformed_point(snapshot: &ObjectSnapshot, point: Vec2) -> Vec2 {
    let scaled = Vec2::new(
        point.x * snapshot.transform.scale.x,
        point.y * snapshot.transform.scale.y,
    );
    let (sin, cos) = snapshot.transform.rotation.sin_cos();
    Vec2::new(
        scaled.x * cos - scaled.y * sin + snapshot.transform.translation.x,
        scaled.x * sin + scaled.y * cos + snapshot.transform.translation.y,
    )
}

fn endpoints(snapshot: &ObjectSnapshot) -> (Vec2, Vec2) {
    let GeometryRef::VectorPath(path) = &snapshot.geometry else {
        panic!("Elbow oracle requires retained VectorPath geometry")
    };

    let mut first = None;
    let mut last = None;
    for command in path.commands() {
        let point = match command {
            PathCommand::MoveTo { to } | PathCommand::LineTo { to } => Some(*to),
            _ => None,
        };
        if let Some(point) = point {
            first.get_or_insert(point);
            last = Some(point);
        }
    }

    let first = first.expect("Elbow path must contain a start point");
    let last = last.expect("Elbow path must contain an end point");
    (
        transformed_point(snapshot, first),
        transformed_point(snapshot, last),
    )
}

fn observation(snapshot: ObjectSnapshot) -> Value {
    let center = snapshot.center();
    let (start, end) = endpoints(&snapshot);
    json!({
        "center": [center.x, center.y],
        "start": [start.x, start.y],
        "end": [end.x, end.y],
        "width": snapshot.width(),
        "height": snapshot.height(),
    })
}

fn insert(observations: &mut Map<String, Value>, name: &str, width: f32, angle: f32) {
    observations.insert(
        name.to_owned(),
        observation(
            Elbow::with_options(width, angle)
                .expect("Elbow oracle fixture must be valid")
                .into_snapshot(),
        ),
    );
}

fn main() {
    let mut observations = Map::new();
    insert(&mut observations, "default", 0.2, 0.0);
    insert(
        &mut observations,
        "rotated_wide",
        2.0,
        5.0 * std::f32::consts::PI / 4.0,
    );
    insert(
        &mut observations,
        "zero_width",
        0.0,
        std::f32::consts::FRAC_PI_3,
    );
    insert(
        &mut observations,
        "negative_width",
        -0.5,
        -std::f32::consts::FRAC_PI_6,
    );

    println!(
        "{}",
        serde_json::to_string(&Value::Object(observations)).expect("serialize observations")
    );
}
