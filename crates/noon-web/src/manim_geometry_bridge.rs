use noon::{Dot, IntoSnapshot, Triangle};
use noon_core::{ObjectSnapshot, Vec2};

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn positive_f32(name: &str, value: f64) -> Result<f32, String> {
    let value = finite_f32(name, value)?;
    if value <= 0.0 {
        return Err(format!("{name} must be positive"));
    }
    Ok(value)
}

fn snapshot_json(snapshot: ObjectSnapshot) -> Result<String, String> {
    serde_json::to_string(&snapshot)
        .map_err(|error| format!("unable to serialize Manim geometry snapshot: {error}"))
}

pub fn manim_dot_snapshot_json(point_x: f64, point_y: f64, radius: f64) -> Result<String, String> {
    let point = Vec2::new(
        finite_f32("point.x", point_x)?,
        finite_f32("point.y", point_y)?,
    );
    snapshot_json(Dot::new(point, positive_f32("radius", radius)?).into_snapshot())
}

pub fn manim_triangle_snapshot_json() -> Result<String, String> {
    snapshot_json(Triangle::new().into_snapshot())
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{manim_dot_snapshot_json, manim_triangle_snapshot_json};

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    #[wasm_bindgen(js_name = manimDotSnapshotJson)]
    pub fn manim_dot_snapshot(point_x: f64, point_y: f64, radius: f64) -> Result<String, JsValue> {
        manim_dot_snapshot_json(point_x, point_y, radius).map_err(js_error)
    }

    #[wasm_bindgen(js_name = manimTriangleSnapshotJson)]
    pub fn manim_triangle_snapshot() -> Result<String, JsValue> {
        manim_triangle_snapshot_json().map_err(js_error)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, PathCommand, BLUE, WHITE};

    use super::*;

    fn decode(value: &str) -> ObjectSnapshot {
        serde_json::from_str(value).expect("valid ObjectSnapshot JSON")
    }

    #[test]
    fn dot_bridge_uses_shared_rust_constructor_defaults() {
        let snapshot = decode(&manim_dot_snapshot_json(2.0, -1.0, 0.2).unwrap());
        assert_eq!(snapshot.transform.translation, Vec2::new(2.0, -1.0));
        assert_eq!(snapshot.style.fill, Some(WHITE));
        assert_eq!(snapshot.style.stroke, Some(WHITE));
        assert_eq!(snapshot.style.stroke_width, 0.0);
        match snapshot.geometry {
            GeometryRef::Circle { radius } => assert_eq!(radius, 0.2),
            other => panic!("expected analytic circle, got {other:?}"),
        }
    }

    #[test]
    fn triangle_bridge_uses_shared_polygon_path_and_blue_default() {
        let snapshot = decode(&manim_triangle_snapshot_json().unwrap());
        assert_eq!(snapshot.style.stroke, Some(BLUE));
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("expected retained vector path")
        };
        assert_eq!(path.commands().len(), 4);
        assert!(matches!(path.commands().last(), Some(PathCommand::Close)));
    }

    #[test]
    fn dot_bridge_rejects_non_renderable_values() {
        assert!(manim_dot_snapshot_json(f64::NAN, 0.0, 0.08).is_err());
        assert!(manim_dot_snapshot_json(0.0, 0.0, 0.0).is_err());
    }
}
