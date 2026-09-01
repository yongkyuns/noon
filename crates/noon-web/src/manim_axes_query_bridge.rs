use noon::{
    transformed_axes_line_graph_vector_path, Axes2DState, CoordinateSystemError, IntoSnapshot,
    LineGraphAuthoringError, NumberRange, Path, TransformedAxes2DState,
};
use noon_core::{GeometryRef, ObjectSnapshot, Vec2};
use noon_geometry::{point_from_geometry_proportion, GeometryProportionError};
use serde::Deserialize;

const MANIM_GRAPH_X_SEARCH_TOLERANCE: f64 = 1.0e-4;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AxesQueryRequest {
    x_range: [f64; 3],
    y_range: [f64; 3],
    x_length: f32,
    y_length: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxesQueryPlan {
    axes: Axes2DState,
}

impl AxesQueryPlan {
    pub fn from_json(request_json: &str) -> Result<Self, ManimAxesQueryError> {
        let request: AxesQueryRequest = serde_json::from_str(request_json)
            .map_err(|error| ManimAxesQueryError::InvalidRequest(error.to_string()))?;
        let x_range = NumberRange::new(request.x_range[0], request.x_range[1], request.x_range[2])?;
        let y_range = NumberRange::new(request.y_range[0], request.y_range[1], request.y_range[2])?;
        Ok(Self {
            axes: Axes2DState::new(x_range, y_range, request.x_length, request.y_length)?,
        })
    }

    pub fn coords_to_point_json(
        &self,
        x: f64,
        y: f64,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimAxesQueryError> {
        let transformed = self.transformed_axes(x_axis_snapshot_json, y_axis_snapshot_json)?;
        serialize_pair(transformed.coords_to_point(x, y)?)
    }

    pub fn point_to_coords_json(
        &self,
        x: f32,
        y: f32,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimAxesQueryError> {
        let transformed = self.transformed_axes(x_axis_snapshot_json, y_axis_snapshot_json)?;
        let (x, y) = transformed.point_to_coords(Vec2::new(x, y))?;
        serde_json::to_string(&[x, y])
            .map_err(|error| ManimAxesQueryError::Serialization(error.to_string()))
    }

    /// Build the retained corner path for ManimCE v0.21 `Axes.plot_line_graph`.
    ///
    /// Python/JS frontends pass only normalized coordinate arrays. Current retained
    /// axis transforms and all path geometry remain owned by this shared Rust plan.
    pub fn line_graph_snapshot_json(
        &self,
        values_json: &str,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimAxesQueryError> {
        let [x_values, y_values]: [Vec<f64>; 2] = serde_json::from_str(values_json)
            .map_err(|error| ManimAxesQueryError::InvalidLineGraphValues(error.to_string()))?;
        let transformed = self.transformed_axes(x_axis_snapshot_json, y_axis_snapshot_json)?;
        let path = transformed_axes_line_graph_vector_path(transformed, &x_values, &y_values)?;
        serde_json::to_string(&Path::new(path).into_snapshot())
            .map_err(|error| ManimAxesQueryError::Serialization(error.to_string()))
    }

    /// ManimCE v0.21 generic `input_to_graph_point` fallback for retained path-like graphs.
    ///
    /// The authored-function fast path remains frontend-visible because it calls the user's
    /// callback directly. For generic VMobjects, however, path proportion, current graph
    /// transform, current Axes transforms, and Manim's binary-search semantics all remain
    /// inside Rust so the host does not repeatedly cross the WASM boundary.
    pub fn graph_point_for_x_json(
        &self,
        x: f64,
        graph_snapshot_json: &str,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimAxesQueryError> {
        if !x.is_finite() {
            return Err(ManimAxesQueryError::NonFiniteGraphX(x));
        }
        let graph: ObjectSnapshot = serde_json::from_str(graph_snapshot_json)
            .map_err(|error| ManimAxesQueryError::InvalidGraphSnapshot(error.to_string()))?;
        let transformed = self.transformed_axes(x_axis_snapshot_json, y_axis_snapshot_json)?;
        let alpha = binary_search_graph_x(&transformed, &graph, x)?;
        if let Some(alpha) = alpha {
            return serialize_pair(graph_point_from_proportion(&graph, alpha)?);
        }

        let start = graph_point_from_proportion(&graph, 0.0)?;
        let end = graph_point_from_proportion(&graph, 1.0)?;
        let start_x = transformed.point_to_coords(start)?.0;
        let end_x = transformed.point_to_coords(end)?.0;
        Err(ManimAxesQueryError::GraphXOutOfRange {
            x,
            start: start_x,
            end: end_x,
        })
    }

    fn transformed_axes(
        &self,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<TransformedAxes2DState, ManimAxesQueryError> {
        let x_axis = parse_axis_snapshot(x_axis_snapshot_json, "x")?;
        let y_axis = parse_axis_snapshot(y_axis_snapshot_json, "y")?;
        Ok(TransformedAxes2DState::new(
            self.axes,
            x_axis.transform,
            y_axis.transform,
        ))
    }
}

fn graph_point_from_proportion(
    graph: &ObjectSnapshot,
    alpha: f64,
) -> Result<Vec2, ManimAxesQueryError> {
    let local = point_from_geometry_proportion(&graph.geometry, alpha as f32)?;
    Ok(graph.transform.transform_point(local))
}

fn graph_x_at_proportion(
    axes: &TransformedAxes2DState,
    graph: &ObjectSnapshot,
    alpha: f64,
) -> Result<f64, ManimAxesQueryError> {
    Ok(axes
        .point_to_coords(graph_point_from_proportion(graph, alpha)?)?
        .0)
}

/// Exact control flow of ManimCE v0.21 `binary_search` for graph-x lookup.
fn binary_search_graph_x(
    axes: &TransformedAxes2DState,
    graph: &ObjectSnapshot,
    target: f64,
) -> Result<Option<f64>, ManimAxesQueryError> {
    let mut left: f64 = 0.0;
    let mut right: f64 = 1.0;
    let mut middle: f64 = 0.5;
    while (right - left).abs() > MANIM_GRAPH_X_SEARCH_TOLERANCE {
        middle = (left + right) * 0.5;
        let left_x = graph_x_at_proportion(axes, graph, left)?;
        let middle_x = graph_x_at_proportion(axes, graph, middle)?;
        let right_x = graph_x_at_proportion(axes, graph, right)?;
        if left_x == target {
            return Ok(Some(left));
        }
        if right_x == target {
            return Ok(Some(right));
        }

        if left_x <= target && target <= right_x {
            if middle_x > target {
                right = middle;
            } else {
                left = middle;
            }
        } else if left_x > target && target > right_x {
            std::mem::swap(&mut left, &mut right);
        } else {
            return Ok(None);
        }
    }
    Ok(Some(middle))
}

fn parse_axis_snapshot(
    snapshot_json: &str,
    axis: &'static str,
) -> Result<ObjectSnapshot, ManimAxesQueryError> {
    let snapshot: ObjectSnapshot = serde_json::from_str(snapshot_json).map_err(|error| {
        ManimAxesQueryError::InvalidAxisSnapshot {
            axis,
            error: error.to_string(),
        }
    })?;
    if !matches!(snapshot.geometry, GeometryRef::Line { .. }) {
        return Err(ManimAxesQueryError::InvalidAxisGeometry(axis));
    }
    Ok(snapshot)
}

fn serialize_pair(point: Vec2) -> Result<String, ManimAxesQueryError> {
    serde_json::to_string(&[f64::from(point.x), f64::from(point.y)])
        .map_err(|error| ManimAxesQueryError::Serialization(error.to_string()))
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManimAxesQueryError {
    InvalidRequest(String),
    InvalidLineGraphValues(String),
    InvalidGraphSnapshot(String),
    InvalidAxisSnapshot { axis: &'static str, error: String },
    InvalidAxisGeometry(&'static str),
    NonFiniteGraphX(f64),
    GraphXOutOfRange { x: f64, start: f64, end: f64 },
    Coordinates(CoordinateSystemError),
    LineGraph(LineGraphAuthoringError),
    GeometryProportion(GeometryProportionError),
    Serialization(String),
}

impl std::fmt::Display for ManimAxesQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => write!(formatter, "invalid Axes query request: {error}"),
            Self::InvalidLineGraphValues(error) => {
                write!(formatter, "invalid Axes.plot_line_graph values: {error}")
            }
            Self::InvalidGraphSnapshot(error) => {
                write!(formatter, "invalid graph snapshot: {error}")
            }
            Self::InvalidAxisSnapshot { axis, error } => {
                write!(formatter, "invalid Axes {axis}-axis snapshot: {error}")
            }
            Self::InvalidAxisGeometry(axis) => {
                write!(
                    formatter,
                    "Axes {axis}-axis snapshot must contain line geometry"
                )
            }
            Self::NonFiniteGraphX(x) => {
                write!(formatter, "graph x lookup requires a finite value: {x}")
            }
            Self::GraphXOutOfRange { x, start, end } => write!(
                formatter,
                "x={x} not located in the range of the graph ([{start}, {end}])"
            ),
            Self::Coordinates(error) => error.fmt(formatter),
            Self::LineGraph(error) => error.fmt(formatter),
            Self::GeometryProportion(error) => error.fmt(formatter),
            Self::Serialization(error) => {
                write!(formatter, "unable to serialize Axes query result: {error}")
            }
        }
    }
}

impl std::error::Error for ManimAxesQueryError {}

impl From<CoordinateSystemError> for ManimAxesQueryError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl From<LineGraphAuthoringError> for ManimAxesQueryError {
    fn from(value: LineGraphAuthoringError) -> Self {
        Self::LineGraph(value)
    }
}

impl From<GeometryProportionError> for ManimAxesQueryError {
    fn from(value: GeometryProportionError) -> Self {
        Self::GeometryProportion(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::AxesQueryPlan;

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    #[wasm_bindgen]
    pub struct WasmAxesQueryPlan(AxesQueryPlan);

    #[wasm_bindgen]
    impl WasmAxesQueryPlan {
        #[wasm_bindgen(constructor)]
        pub fn new(request_json: &str) -> Result<Self, JsValue> {
            AxesQueryPlan::from_json(request_json)
                .map(Self)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = coordsToPointJson)]
        pub fn coords_to_point_json(
            &self,
            x: f64,
            y: f64,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .coords_to_point_json(x, y, x_axis_snapshot_json, y_axis_snapshot_json)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = pointToCoordsJson)]
        pub fn point_to_coords_json(
            &self,
            x: f32,
            y: f32,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .point_to_coords_json(x, y, x_axis_snapshot_json, y_axis_snapshot_json)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = lineGraphSnapshotJson)]
        pub fn line_graph_snapshot_json(
            &self,
            values_json: &str,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .line_graph_snapshot_json(values_json, x_axis_snapshot_json, y_axis_snapshot_json)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = graphPointForXJson)]
        pub fn graph_point_for_x_json(
            &self,
            x: f64,
            graph_snapshot_json: &str,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .graph_point_for_x_json(
                    x,
                    graph_snapshot_json,
                    x_axis_snapshot_json,
                    y_axis_snapshot_json,
                )
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmAxesQueryPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use noon::{IntoSnapshot, Line};
    use noon_core::{PathCommand, Transform2D};

    fn request_json() -> &'static str {
        r#"{"x_range":[-2,2,1],"y_range":[-2,2,1],"x_length":4,"y_length":4}"#
    }

    fn axis_snapshot(axis: noon::NumberLineState, transform: Transform2D) -> String {
        let mut snapshot = Line::new(axis.start(), axis.end()).into_snapshot();
        snapshot.transform = transform;
        serde_json::to_string(&snapshot).unwrap()
    }

    fn identity_axes(plan: &AxesQueryPlan) -> (String, String) {
        (
            axis_snapshot(plan.axes.x_axis(), Transform2D::IDENTITY),
            axis_snapshot(plan.axes.y_axis(), Transform2D::IDENTITY),
        )
    }

    #[test]
    fn current_retained_transforms_drive_round_trip_queries() {
        let plan = AxesQueryPlan::from_json(request_json()).unwrap();
        let transform = Transform2D {
            translation: Vec2::new(3.0, -2.0),
            rotation: 0.45,
            scale: Vec2::new(1.25, 1.25),
        };
        let x_axis = axis_snapshot(plan.axes.x_axis(), transform);
        let y_axis = axis_snapshot(plan.axes.y_axis(), transform);
        let point: [f64; 2] = serde_json::from_str(
            &plan
                .coords_to_point_json(1.0, -0.5, &x_axis, &y_axis)
                .unwrap(),
        )
        .unwrap();
        let coords: [f64; 2] = serde_json::from_str(
            &plan
                .point_to_coords_json(point[0] as f32, point[1] as f32, &x_axis, &y_axis)
                .unwrap(),
        )
        .unwrap();
        assert!((coords[0] - 1.0).abs() <= 1.0e-5);
        assert!((coords[1] + 0.5).abs() <= 1.0e-5);
    }

    #[test]
    fn line_graph_snapshot_uses_current_retained_axis_state() {
        let plan = AxesQueryPlan::from_json(request_json()).unwrap();
        let transform = Transform2D {
            translation: Vec2::new(-1.0, 2.0),
            rotation: -0.3,
            scale: Vec2::new(0.75, 0.75),
        };
        let x_axis = axis_snapshot(plan.axes.x_axis(), transform);
        let y_axis = axis_snapshot(plan.axes.y_axis(), transform);
        let snapshot: ObjectSnapshot = serde_json::from_str(
            &plan
                .line_graph_snapshot_json("[[-1.0,0.0,1.0],[1.0,0.0,-1.0]]", &x_axis, &y_axis)
                .unwrap(),
        )
        .unwrap();
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("line graph must lower to ordinary VectorPath geometry");
        };
        assert_eq!(path.commands().len(), 3);
        let expected = [
            transform.transform_point(plan.axes.coords_to_point(-1.0, 1.0).unwrap()),
            transform.transform_point(plan.axes.coords_to_point(0.0, 0.0).unwrap()),
            transform.transform_point(plan.axes.coords_to_point(1.0, -1.0).unwrap()),
        ];
        for (command, expected) in path.commands().iter().zip(expected) {
            match command {
                PathCommand::MoveTo { to } | PathCommand::LineTo { to } => {
                    assert!((*to - expected).length() <= 1.0e-5)
                }
                other => panic!("expected corner path command, got {other:?}"),
            }
        }
    }

    #[test]
    fn generic_graph_lookup_matches_manim_binary_search_for_ascending_and_descending_x() {
        let plan = AxesQueryPlan::from_json(request_json()).unwrap();
        let (x_axis, y_axis) = identity_axes(&plan);

        for values in [
            "[[-1.0,0.0,1.0],[1.0,0.0,-1.0]]",
            "[[1.0,0.0,-1.0],[1.0,0.0,-1.0]]",
        ] {
            let graph = plan
                .line_graph_snapshot_json(values, &x_axis, &y_axis)
                .unwrap();
            let point: [f64; 2] = serde_json::from_str(
                &plan
                    .graph_point_for_x_json(0.5, &graph, &x_axis, &y_axis)
                    .unwrap(),
            )
            .unwrap();
            let coords: [f64; 2] = serde_json::from_str(
                &plan
                    .point_to_coords_json(point[0] as f32, point[1] as f32, &x_axis, &y_axis)
                    .unwrap(),
            )
            .unwrap();
            assert!((coords[0] - 0.5).abs() <= MANIM_GRAPH_X_SEARCH_TOLERANCE);
        }
    }

    #[test]
    fn generic_graph_lookup_uses_current_graph_and_axes_transforms() {
        let plan = AxesQueryPlan::from_json(request_json()).unwrap();
        let axes_transform = Transform2D {
            translation: Vec2::new(2.0, -1.0),
            rotation: 0.2,
            scale: Vec2::new(1.1, 1.1),
        };
        let x_axis = axis_snapshot(plan.axes.x_axis(), axes_transform);
        let y_axis = axis_snapshot(plan.axes.y_axis(), axes_transform);
        let mut graph: ObjectSnapshot = serde_json::from_str(
            &plan
                .line_graph_snapshot_json("[[-1.0,0.0,1.0],[0.0,0.0,0.0]]", &x_axis, &y_axis)
                .unwrap(),
        )
        .unwrap();
        graph.transform.translation = Vec2::new(0.2, 0.0);
        let point: [f64; 2] = serde_json::from_str(
            &plan
                .graph_point_for_x_json(
                    0.5,
                    &serde_json::to_string(&graph).unwrap(),
                    &x_axis,
                    &y_axis,
                )
                .unwrap(),
        )
        .unwrap();
        let coords = TransformedAxes2DState::new(plan.axes, axes_transform, axes_transform)
            .point_to_coords(Vec2::new(point[0] as f32, point[1] as f32))
            .unwrap();
        assert!((coords.0 - 0.5).abs() <= MANIM_GRAPH_X_SEARCH_TOLERANCE);
    }

    #[test]
    fn generic_graph_lookup_rejects_out_of_range_and_non_path_geometry() {
        let plan = AxesQueryPlan::from_json(request_json()).unwrap();
        let (x_axis, y_axis) = identity_axes(&plan);
        let graph = plan
            .line_graph_snapshot_json("[[-1.0,0.0,1.0],[0.0,0.0,0.0]]", &x_axis, &y_axis)
            .unwrap();
        assert!(matches!(
            plan.graph_point_for_x_json(3.0, &graph, &x_axis, &y_axis),
            Err(ManimAxesQueryError::GraphXOutOfRange { .. })
        ));

        let rectangle =
            serde_json::to_string(&ObjectSnapshot::new(GeometryRef::rectangle(1.0, 1.0))).unwrap();
        assert!(matches!(
            plan.graph_point_for_x_json(0.0, &rectangle, &x_axis, &y_axis),
            Err(ManimAxesQueryError::GeometryProportion(
                GeometryProportionError::UnsupportedGeometry
            ))
        ));
    }

    #[test]
    fn malformed_and_mismatched_line_graph_values_fail_closed() {
        let plan = AxesQueryPlan::from_json(request_json()).unwrap();
        let (x_axis, y_axis) = identity_axes(&plan);
        assert!(matches!(
            plan.line_graph_snapshot_json("not json", &x_axis, &y_axis),
            Err(ManimAxesQueryError::InvalidLineGraphValues(_))
        ));
        assert!(matches!(
            plan.line_graph_snapshot_json("[[0.0,1.0],[2.0]]", &x_axis, &y_axis),
            Err(ManimAxesQueryError::LineGraph(
                LineGraphAuthoringError::CoordinateCountMismatch { .. }
            ))
        ));
    }

    #[test]
    fn non_line_snapshot_fails_closed() {
        let plan = AxesQueryPlan::from_json(request_json()).unwrap();
        let bad = serde_json::to_string(&ObjectSnapshot::new(GeometryRef::circle(1.0))).unwrap();
        let good = axis_snapshot(plan.axes.y_axis(), Transform2D::IDENTITY);
        assert_eq!(
            plan.coords_to_point_json(0.0, 0.0, &bad, &good)
                .unwrap_err(),
            ManimAxesQueryError::InvalidAxisGeometry("x")
        );
    }
}
