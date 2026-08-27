use noon::{Arc, ArcBetweenPoints};
use noon_core::{ObjectSnapshot, Vec2};

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn snapshot_json(snapshot: &ObjectSnapshot) -> Result<String, String> {
    serde_json::to_string(snapshot)
        .map_err(|error| format!("unable to serialize shared geometry snapshot: {error}"))
}

fn manim_arc_snapshot_json(
    radius: f64,
    start_angle: f64,
    angle: f64,
    num_components: u32,
    center_x: f64,
    center_y: f64,
) -> Result<String, String> {
    let arc = Arc::with_options(
        finite_f32("radius", radius)?,
        finite_f32("start_angle", start_angle)?,
        finite_f32("angle", angle)?,
        num_components as usize,
        Vec2::new(
            finite_f32("arc_center.x", center_x)?,
            finite_f32("arc_center.y", center_y)?,
        ),
    )
    .map_err(|error| error.to_string())?;
    snapshot_json(arc.snapshot())
}

#[allow(clippy::too_many_arguments)]
fn manim_arc_between_points_snapshot_json(
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    angle: f64,
    radius: Option<f64>,
    num_components: u32,
) -> Result<String, String> {
    let radius = radius
        .map(|value| finite_f32("radius", value))
        .transpose()?;
    let arc = ArcBetweenPoints::with_options(
        Vec2::new(
            finite_f32("start.x", start_x)?,
            finite_f32("start.y", start_y)?,
        ),
        Vec2::new(finite_f32("end.x", end_x)?, finite_f32("end.y", end_y)?),
        finite_f32("angle", angle)?,
        radius,
        num_components as usize,
    )
    .map_err(|error| error.to_string())?;
    snapshot_json(arc.snapshot())
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use crate::{WasmAuthoringMobjectHandle, WasmAuthoringStore};

    use super::{manim_arc_between_points_snapshot_json, manim_arc_snapshot_json};

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    #[wasm_bindgen]
    impl WasmAuthoringStore {
        #[wasm_bindgen(js_name = createManimArc)]
        #[allow(clippy::too_many_arguments)]
        pub fn create_manim_arc(
            &self,
            radius: f64,
            start_angle: f64,
            angle: f64,
            num_components: u32,
            center_x: f64,
            center_y: f64,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            let snapshot_json = manim_arc_snapshot_json(
                radius,
                start_angle,
                angle,
                num_components,
                center_x,
                center_y,
            )
            .map_err(js_error)?;
            self.create_mobject(&snapshot_json)
        }

        #[wasm_bindgen(js_name = createManimArcBetweenPoints)]
        #[allow(clippy::too_many_arguments)]
        pub fn create_manim_arc_between_points(
            &self,
            start_x: f64,
            start_y: f64,
            end_x: f64,
            end_y: f64,
            angle: f64,
            num_components: u32,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            let snapshot_json = manim_arc_between_points_snapshot_json(
                start_x,
                start_y,
                end_x,
                end_y,
                angle,
                None,
                num_components,
            )
            .map_err(js_error)?;
            self.create_mobject(&snapshot_json)
        }

        #[wasm_bindgen(js_name = createManimArcBetweenPointsWithRadius)]
        #[allow(clippy::too_many_arguments)]
        pub fn create_manim_arc_between_points_with_radius(
            &self,
            start_x: f64,
            start_y: f64,
            end_x: f64,
            end_y: f64,
            angle: f64,
            radius: f64,
            num_components: u32,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            let snapshot_json = manim_arc_between_points_snapshot_json(
                start_x,
                start_y,
                end_x,
                end_y,
                angle,
                Some(radius),
                num_components,
            )
            .map_err(js_error)?;
            self.create_mobject(&snapshot_json)
        }
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, ObjectSnapshot, PathCommand, Vec2};

    use super::{manim_arc_between_points_snapshot_json, manim_arc_snapshot_json};

    fn snapshot(value: String) -> ObjectSnapshot {
        serde_json::from_str(&value).expect("bridge emits a valid shared snapshot")
    }

    #[test]
    fn arc_bridge_uses_shared_vector_path_geometry() {
        let snapshot = snapshot(
            manim_arc_snapshot_json(2.0, 0.25, 1.5, 9, 3.0, -2.0)
                .expect("valid shared Arc"),
        );
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("Arc bridge must preserve shared VectorPath geometry");
        };
        assert!(matches!(path.commands().first(), Some(PathCommand::MoveTo { .. })));
        assert_eq!(path.commands().len(), 9);
    }

    #[test]
    fn arc_between_points_bridge_preserves_shared_endpoints() {
        let snapshot = snapshot(
            manim_arc_between_points_snapshot_json(-2.0, 1.0, 3.0, -1.0, 1.25, None, 9)
                .expect("valid shared ArcBetweenPoints"),
        );
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("ArcBetweenPoints bridge must preserve shared VectorPath geometry");
        };
        assert_eq!(
            path.commands().first(),
            Some(&PathCommand::MoveTo {
                to: Vec2::new(-2.0, 1.0),
            })
        );
        assert!(matches!(
            path.commands().last(),
            Some(PathCommand::CubicTo { to, .. }) if (*to - Vec2::new(3.0, -1.0)).length() <= 1.0e-5
        ));
    }

    #[test]
    fn bridge_rejects_values_that_would_truncate_before_shared_validation() {
        assert!(manim_arc_snapshot_json(f64::MAX, 0.0, 1.0, 9, 0.0, 0.0)
            .expect_err("overflow must be rejected")
            .contains("radius"));
        assert!(manim_arc_between_points_snapshot_json(
            0.0,
            0.0,
            1.0,
            0.0,
            f64::NAN,
            None,
            9,
        )
        .expect_err("NaN must be rejected")
        .contains("angle"));
    }
}
