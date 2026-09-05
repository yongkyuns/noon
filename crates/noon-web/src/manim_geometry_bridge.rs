use noon::legacy::{
    arc_center_from_snapshot, arc_end_from_snapshot, arc_start_from_snapshot,
    arc_stop_angle_from_snapshot, Arc, ArcBetweenPoints, ArcMetadata, Dot, IntoSnapshot,
    RoundedRectangle, Triangle,
};
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

fn point(name: &str, x: f64, y: f64) -> Result<Vec2, String> {
    Ok(Vec2::new(
        finite_f32(&format!("{name}.x"), x)?,
        finite_f32(&format!("{name}.y"), y)?,
    ))
}

fn snapshot_json(snapshot: ObjectSnapshot) -> Result<String, String> {
    serde_json::to_string(&snapshot)
        .map_err(|error| format!("unable to serialize Manim geometry snapshot: {error}"))
}

pub fn manim_dot_snapshot_json(point_x: f64, point_y: f64, radius: f64) -> Result<String, String> {
    let point = point("point", point_x, point_y)?;
    snapshot_json(Dot::new(point, positive_f32("radius", radius)?).into_snapshot())
}

pub fn manim_triangle_snapshot_json() -> Result<String, String> {
    snapshot_json(Triangle::new().into_snapshot())
}

#[derive(Clone, Debug, PartialEq)]
pub struct ManimArcBridgeSpec {
    snapshot: ObjectSnapshot,
    metadata: ArcMetadata,
}

impl ManimArcBridgeSpec {
    fn new(snapshot: ObjectSnapshot, metadata: ArcMetadata) -> Self {
        Self { snapshot, metadata }
    }

    pub fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }

    pub fn metadata(&self) -> ArcMetadata {
        self.metadata
    }

    pub fn snapshot_json(&self) -> Result<String, String> {
        snapshot_json(self.snapshot.clone())
    }
}

pub fn manim_arc_bridge_spec(
    radius: f64,
    start_angle: f64,
    angle: f64,
    num_components: u32,
    center_x: f64,
    center_y: f64,
) -> Result<ManimArcBridgeSpec, String> {
    let arc = Arc::with_options(
        finite_f32("radius", radius)?,
        finite_f32("start_angle", start_angle)?,
        finite_f32("angle", angle)?,
        num_components as usize,
        point("arc_center", center_x, center_y)?,
    )
    .map_err(|error| error.to_string())?;
    let metadata = arc.metadata();
    Ok(ManimArcBridgeSpec::new(arc.into_snapshot(), metadata))
}

#[allow(clippy::too_many_arguments)]
pub fn manim_arc_between_points_bridge_spec(
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    angle: f64,
    radius: Option<f64>,
    num_components: u32,
) -> Result<ManimArcBridgeSpec, String> {
    let radius = radius
        .map(|value| finite_f32("radius", value))
        .transpose()?;
    let arc = ArcBetweenPoints::with_options(
        point("start", start_x, start_y)?,
        point("end", end_x, end_y)?,
        finite_f32("angle", angle)?,
        radius,
        num_components as usize,
    )
    .map_err(|error| error.to_string())?;
    let metadata = arc.metadata();
    Ok(ManimArcBridgeSpec::new(arc.into_snapshot(), metadata))
}

#[derive(Clone, Debug, PartialEq)]
pub struct ManimArcSnapshotQuery {
    snapshot: ObjectSnapshot,
}

impl ManimArcSnapshotQuery {
    pub fn from_json(snapshot_json: &str) -> Result<Self, String> {
        serde_json::from_str(snapshot_json)
            .map(|snapshot| Self { snapshot })
            .map_err(|error| format!("invalid Manim Arc snapshot: {error}"))
    }

    pub fn start(&self) -> Result<Vec2, String> {
        arc_start_from_snapshot(&self.snapshot)
            .ok_or_else(|| "Manim Arc snapshot has no path start".to_owned())
    }

    pub fn end(&self) -> Result<Vec2, String> {
        arc_end_from_snapshot(&self.snapshot)
            .ok_or_else(|| "Manim Arc snapshot has no path end".to_owned())
    }

    pub fn center(&self) -> Vec2 {
        arc_center_from_snapshot(&self.snapshot)
    }

    pub fn stop_angle(&self) -> Result<f32, String> {
        arc_stop_angle_from_snapshot(&self.snapshot)
            .ok_or_else(|| "Manim Arc snapshot has no stop angle".to_owned())
    }
}

pub fn manim_rounded_rectangle_snapshot_json(
    width: f64,
    height: f64,
    corner_radius: f64,
) -> Result<String, String> {
    let rectangle = RoundedRectangle::new(
        positive_f32("width", width)?,
        positive_f32("height", height)?,
        finite_f32("corner_radius", corner_radius)?,
    )
    .map_err(|error| error.to_string())?;
    snapshot_json(rectangle.into_snapshot())
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{
        manim_arc_between_points_bridge_spec, manim_arc_bridge_spec, manim_dot_snapshot_json,
        manim_rounded_rectangle_snapshot_json, manim_triangle_snapshot_json, ManimArcBridgeSpec,
        ManimArcSnapshotQuery,
    };

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

    #[wasm_bindgen]
    pub struct WasmManimArcSpec(ManimArcBridgeSpec);

    #[wasm_bindgen]
    impl WasmManimArcSpec {
        #[wasm_bindgen(js_name = snapshotJson)]
        pub fn snapshot_json(&self) -> Result<String, JsValue> {
            self.0.snapshot_json().map_err(js_error)
        }

        #[wasm_bindgen(getter)]
        pub fn radius(&self) -> f64 {
            f64::from(self.0.metadata().radius())
        }

        #[wasm_bindgen(getter, js_name = startAngle)]
        pub fn start_angle(&self) -> f64 {
            f64::from(self.0.metadata().start_angle())
        }

        #[wasm_bindgen(getter)]
        pub fn angle(&self) -> f64 {
            f64::from(self.0.metadata().angle())
        }

        #[wasm_bindgen(getter, js_name = numComponents)]
        pub fn num_components(&self) -> usize {
            self.0.metadata().num_components()
        }
    }

    #[wasm_bindgen(js_name = createManimArcSpec)]
    pub fn create_manim_arc_spec(
        radius: f64,
        start_angle: f64,
        angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<WasmManimArcSpec, JsValue> {
        manim_arc_bridge_spec(
            radius,
            start_angle,
            angle,
            num_components,
            center_x,
            center_y,
        )
        .map(WasmManimArcSpec)
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = createManimArcBetweenPointsSpec)]
    #[allow(clippy::too_many_arguments)]
    pub fn create_manim_arc_between_points_spec(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        angle: f64,
        radius: Option<f64>,
        num_components: u32,
    ) -> Result<WasmManimArcSpec, JsValue> {
        manim_arc_between_points_bridge_spec(
            start_x,
            start_y,
            end_x,
            end_y,
            angle,
            radius,
            num_components,
        )
        .map(WasmManimArcSpec)
        .map_err(js_error)
    }

    #[wasm_bindgen]
    pub struct WasmManimArcSnapshotQuery(ManimArcSnapshotQuery);

    #[wasm_bindgen]
    impl WasmManimArcSnapshotQuery {
        #[wasm_bindgen(constructor)]
        pub fn new(snapshot_json: &str) -> Result<WasmManimArcSnapshotQuery, JsValue> {
            ManimArcSnapshotQuery::from_json(snapshot_json)
                .map(WasmManimArcSnapshotQuery)
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = startX)]
        pub fn start_x(&self) -> Result<f64, JsValue> {
            self.0
                .start()
                .map(|point| f64::from(point.x))
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = startY)]
        pub fn start_y(&self) -> Result<f64, JsValue> {
            self.0
                .start()
                .map(|point| f64::from(point.y))
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = endX)]
        pub fn end_x(&self) -> Result<f64, JsValue> {
            self.0
                .end()
                .map(|point| f64::from(point.x))
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = endY)]
        pub fn end_y(&self) -> Result<f64, JsValue> {
            self.0
                .end()
                .map(|point| f64::from(point.y))
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = centerX)]
        pub fn center_x(&self) -> f64 {
            f64::from(self.0.center().x)
        }

        #[wasm_bindgen(getter, js_name = centerY)]
        pub fn center_y(&self) -> f64 {
            f64::from(self.0.center().y)
        }

        #[wasm_bindgen(getter, js_name = stopAngle)]
        pub fn stop_angle(&self) -> Result<f64, JsValue> {
            self.0.stop_angle().map(f64::from).map_err(js_error)
        }
    }

    #[wasm_bindgen(js_name = manimRoundedRectangleSnapshotJson)]
    pub fn manim_rounded_rectangle_snapshot(
        width: f64,
        height: f64,
        corner_radius: f64,
    ) -> Result<String, JsValue> {
        manim_rounded_rectangle_snapshot_json(width, height, corner_radius).map_err(js_error)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon::legacy::{arc_center_from_snapshot, arc_end_from_snapshot, arc_start_from_snapshot};
    use noon_core::{GeometryRef, PathCommand, BLUE, TAU, WHITE};

    use super::*;

    fn decode(value: &str) -> ObjectSnapshot {
        serde_json::from_str(value).expect("valid ObjectSnapshot JSON")
    }

    fn assert_close(left: f32, right: f32) {
        assert!((left - right).abs() <= 1e-5, "{left} != {right}");
    }

    fn assert_vec_close(left: Vec2, right: Vec2) {
        assert_close(left.x, right.x);
        assert_close(left.y, right.y);
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
    fn arc_bridge_keeps_constructor_metadata_with_rust_snapshot() {
        let spec = manim_arc_bridge_spec(2.0, 0.25, 1.5, 7, 3.0, -4.0).unwrap();
        assert_close(spec.metadata().radius(), 2.0);
        assert_close(spec.metadata().start_angle(), 0.25);
        assert_close(spec.metadata().angle(), 1.5);
        assert_eq!(spec.metadata().num_components(), 7);
        assert!(matches!(
            spec.snapshot().geometry,
            GeometryRef::VectorPath(_)
        ));
    }

    #[test]
    fn bridge_queries_current_serialized_snapshot_through_shared_arc_math() {
        let spec = manim_arc_bridge_spec(1.25, 0.1, f64::from(TAU) / 3.0, 9, 0.5, -0.25).unwrap();
        let current = spec
            .snapshot()
            .clone()
            .scale_xy(Vec2::new(1.5, 0.75))
            .rotate_by(0.4)
            .shift(Vec2::new(-2.0, 3.0));
        let json = serde_json::to_string(&current).unwrap();
        let query = ManimArcSnapshotQuery::from_json(&json).unwrap();

        assert_vec_close(
            query.start().unwrap(),
            arc_start_from_snapshot(&current).unwrap(),
        );
        assert_vec_close(
            query.end().unwrap(),
            arc_end_from_snapshot(&current).unwrap(),
        );
        assert_vec_close(query.center(), arc_center_from_snapshot(&current));
        assert_close(
            query.stop_angle().unwrap(),
            arc_stop_angle_from_snapshot(&current).unwrap(),
        );
    }

    #[test]
    fn between_points_bridge_preserves_resolved_negative_radius_metadata() {
        let spec = manim_arc_between_points_bridge_spec(
            -2.0,
            0.0,
            2.0,
            0.0,
            TAU as f64 / 4.0,
            Some(-3.0),
            9,
        )
        .unwrap();
        assert_close(spec.metadata().radius(), 3.0);
        assert!(spec.metadata().angle() < 0.0);
    }

    #[test]
    fn query_rejects_non_arc_snapshot_shape_for_endpoint_queries() {
        let snapshot = ObjectSnapshot::new(GeometryRef::circle(1.0));
        let json = serde_json::to_string(&snapshot).unwrap();
        let query = ManimArcSnapshotQuery::from_json(&json).unwrap();
        assert!(query.start().is_err());
        assert!(query.end().is_err());
        assert!(query.stop_angle().is_err());
    }

    #[test]
    fn rounded_rectangle_bridge_uses_shared_path_and_clamping() {
        let snapshot = decode(&manim_rounded_rectangle_snapshot_json(4.0, 2.0, 10.0).unwrap());
        assert_eq!(snapshot.style.stroke, Some(WHITE));
        let GeometryRef::VectorPath(ref path) = snapshot.geometry else {
            panic!("expected retained vector path")
        };
        // Clamping to radius 1 collapses the two vertical straight spans, leaving
        // Move + four cubic corners + two horizontal edges.
        assert_eq!(path.commands().len(), 7);
        assert_eq!(snapshot.width(), 4.0);
        assert_eq!(snapshot.height(), 2.0);
        assert!(matches!(path.commands()[1], PathCommand::CubicTo { .. }));
    }

    #[test]
    fn rounded_rectangle_bridge_preserves_negative_corner_radius() {
        let positive = decode(&manim_rounded_rectangle_snapshot_json(4.0, 2.0, 0.5).unwrap());
        let negative = decode(&manim_rounded_rectangle_snapshot_json(4.0, 2.0, -0.5).unwrap());
        assert_ne!(positive.geometry, negative.geometry);
        assert_eq!(positive.width(), negative.width());
        assert_eq!(positive.height(), negative.height());
    }

    #[test]
    fn geometry_bridge_rejects_non_renderable_values() {
        assert!(manim_dot_snapshot_json(f64::NAN, 0.0, 0.08).is_err());
        assert!(manim_dot_snapshot_json(0.0, 0.0, 0.0).is_err());
        assert!(manim_arc_bridge_spec(f64::NAN, 0.0, 1.0, 9, 0.0, 0.0).is_err());
        assert!(manim_arc_bridge_spec(1.0, 0.0, 1.0, 1, 0.0, 0.0).is_err());
        assert!(
            manim_arc_between_points_bridge_spec(0.0, 0.0, f64::INFINITY, 1.0, 1.0, None, 9,)
                .is_err()
        );
        assert!(manim_rounded_rectangle_snapshot_json(0.0, 2.0, 0.5).is_err());
        assert!(manim_rounded_rectangle_snapshot_json(4.0, f64::INFINITY, 0.5).is_err());
        assert!(manim_rounded_rectangle_snapshot_json(4.0, 2.0, f64::NAN).is_err());
    }
}
