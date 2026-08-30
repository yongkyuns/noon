use noon_core::{GeometryRef, ObjectSnapshot, Vec2};
use noon_geometry::point_from_proportion;

fn validate_alpha(alpha: f64) -> Result<f32, String> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(format!(
            "path proportion must be finite and between 0 and 1: {alpha}"
        ));
    }
    Ok(alpha as f32)
}

fn transform_point(snapshot: &ObjectSnapshot, point: Vec2) -> Vec2 {
    let transform = snapshot.transform;
    let scaled = Vec2::new(point.x * transform.scale.x, point.y * transform.scale.y);
    let sine = transform.rotation.sin();
    let cosine = transform.rotation.cos();
    Vec2::new(
        scaled.x * cosine - scaled.y * sine + transform.translation.x,
        scaled.x * sine + scaled.y * cosine + transform.translation.y,
    )
}

/// Query ManimCE v0.21-compatible path position from an ordinary retained snapshot.
///
/// Vector paths use the shared `noon-geometry` ten-sample Bezier length measure.
/// Analytic lines stay analytic and use exact linear interpolation. The returned point
/// is transformed into world space from the snapshot's current retained transform.
pub fn manim_point_from_proportion(snapshot_json: &str, alpha: f64) -> Result<Vec2, String> {
    let alpha = validate_alpha(alpha)?;
    let snapshot: ObjectSnapshot = serde_json::from_str(snapshot_json)
        .map_err(|error| format!("invalid path query snapshot: {error}"))?;

    let local = match &snapshot.geometry {
        GeometryRef::Line { start, end } => *start + (*end - *start) * alpha,
        GeometryRef::VectorPath(path) => {
            point_from_proportion(path, alpha).map_err(|error| error.to_string())?
        }
        _ => {
            return Err(
                "point_from_proportion requires retained line or vector-path geometry".to_owned(),
            )
        }
    };
    Ok(transform_point(&snapshot, local))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::manim_point_from_proportion;

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    #[wasm_bindgen]
    pub struct WasmManimPathPoint {
        x: f64,
        y: f64,
    }

    #[wasm_bindgen]
    impl WasmManimPathPoint {
        #[wasm_bindgen(getter)]
        pub fn x(&self) -> f64 {
            self.x
        }

        #[wasm_bindgen(getter)]
        pub fn y(&self) -> f64 {
            self.y
        }
    }

    #[wasm_bindgen(js_name = manimPointFromProportion)]
    pub fn manim_point_from_proportion_wasm(
        snapshot_json: &str,
        alpha: f64,
    ) -> Result<WasmManimPathPoint, JsValue> {
        manim_point_from_proportion(snapshot_json, alpha)
            .map(|point| WasmManimPathPoint {
                x: f64::from(point.x),
                y: f64::from(point.y),
            })
            .map_err(js_error)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon::{Arc, IntoSnapshot};
    use noon_core::{GeometryRef, ObjectSnapshot, Vec2, VectorPath};

    use super::*;

    fn encode(snapshot: &ObjectSnapshot) -> String {
        serde_json::to_string(snapshot).expect("valid snapshot")
    }

    fn assert_point(actual: Vec2, expected: Vec2) {
        assert!(
            (actual.x - expected.x).abs() <= 1e-5 && (actual.y - expected.y).abs() <= 1e-5,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn query_preserves_analytic_line_and_current_transform() {
        let snapshot =
            ObjectSnapshot::new(GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(3.0, 0.0)))
                .scale_xy(Vec2::new(2.0, 3.0))
                .shift(Vec2::new(1.0, -1.0));

        assert_point(
            manim_point_from_proportion(&encode(&snapshot), 0.25).expect("line query"),
            Vec2::new(1.0, -1.0),
        );
    }

    #[test]
    fn query_delegates_bezier_measure_to_shared_geometry() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .quadratic_to(Vec2::new(1.0, 2.0), Vec2::new(2.0, 0.0));
        let snapshot = ObjectSnapshot::new(GeometryRef::VectorPath(path));

        assert_point(
            manim_point_from_proportion(&encode(&snapshot), 0.5).expect("Bezier query"),
            Vec2::new(1.0, 1.0),
        );
    }

    #[test]
    fn query_handles_shared_arc_vector_path_endpoints() {
        let snapshot = Arc::with_options(2.0, 0.0, std::f32::consts::FRAC_PI_2, 9, Vec2::ZERO)
            .expect("valid arc")
            .into_snapshot();
        let json = encode(&snapshot);

        assert_point(
            manim_point_from_proportion(&json, 0.0).expect("arc start"),
            Vec2::new(2.0, 0.0),
        );
        assert_point(
            manim_point_from_proportion(&json, 1.0).expect("arc end"),
            Vec2::new(0.0, 2.0),
        );
    }

    #[test]
    fn query_rejects_invalid_alpha_snapshot_and_unsupported_geometry() {
        let circle = encode(&ObjectSnapshot::new(GeometryRef::circle(1.0)));
        assert!(manim_point_from_proportion(&circle, -0.1).is_err());
        assert!(manim_point_from_proportion(&circle, f64::NAN).is_err());
        assert!(manim_point_from_proportion("not json", 0.5).is_err());
        assert!(manim_point_from_proportion(&circle, 0.5).is_err());
    }
}
