use noon::{
    transformed_axes_line_graph_vector_path, transformed_graph_point_for_x, AreaAuthoringError,
    AreaSamplePlan, Axes2DState, CoordinateSystemError, GraphParameterRange, GraphQueryError,
    IntoSnapshot, LineGraphAuthoringError, NumberRange, Path, RiemannAuthoringError,
    RiemannSamplePlan, RiemannSampleType, TransformedAxes2DState,
};
use noon_core::{GeometryRef, ObjectSnapshot, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AxesQueryRequest {
    x_range: [f64; 3],
    y_range: [f64; 3],
    x_length: f32,
    y_length: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct RiemannQueryRequest {
    graph_range: [f64; 2],
    #[serde(default)]
    bounded_graph_range: Option<[f64; 2]>,
    #[serde(default)]
    x_range: Option<[f64; 2]>,
    dx: f64,
    input_sample_type: String,
    width_scale_factor: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AreaQueryRequest {
    graph_range: [f64; 2],
    #[serde(default)]
    bounded_graph_range: Option<[f64; 2]>,
    #[serde(default)]
    x_range: Option<[f64; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AreaFinishInput<'a> {
    graph_snapshot_json: &'a str,
    bounded_graph_snapshot_json: &'a str,
    graph_endpoint_y_values_json: &'a str,
    bounded_graph_endpoint_y_values_json: &'a str,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct RiemannSampleValues {
    graph: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bounded_graph: Option<Vec<f64>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct RiemannRectangleResult {
    snapshot: ObjectSnapshot,
    negative_signed_area: bool,
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
    /// callback directly. Generic path proportion, graph/Axes transforms, and Manim's binary
    /// search are delegated to the reusable shared `noon` semantic owner.
    pub fn graph_point_for_x_json(
        &self,
        x: f64,
        graph_snapshot_json: &str,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimAxesQueryError> {
        let graph = parse_graph_snapshot(graph_snapshot_json)?;
        let transformed = self.transformed_axes(x_axis_snapshot_json, y_axis_snapshot_json)?;
        serialize_pair(transformed_graph_point_for_x(transformed, &graph, x)?)
    }

    /// Phase one of ManimCE v0.21 `Axes.get_riemann_rectangles`.
    ///
    /// Rust resolves the half-open partition and returns only the x values at which the
    /// Python authored callbacks must be evaluated. No geometry or coordinate loop is
    /// owned by Python.
    pub fn riemann_sample_values_json(
        &self,
        request_json: &str,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimAxesQueryError> {
        let request = parse_riemann_request(request_json)?;
        let transformed = self.transformed_axes(x_axis_snapshot_json, y_axis_snapshot_json)?;
        let plan = riemann_plan(transformed, &request)?;
        serde_json::to_string(&RiemannSampleValues {
            graph: plan.graph_sample_x_values(),
            bounded_graph: request
                .bounded_graph_range
                .map(|_| plan.baseline_x_values()),
        })
        .map_err(|error| ManimAxesQueryError::Serialization(error.to_string()))
    }

    /// Phase two of ManimCE v0.21 `Axes.get_riemann_rectangles`.
    ///
    /// The frontend returns authored callback scalar values. Rust rebuilds the exact
    /// deterministic plan, validates cardinality, and emits ordinary retained rectangles
    /// plus signed-area metadata used only for Manim-facing color adaptation.
    pub fn riemann_rectangles_json(
        &self,
        request_json: &str,
        graph_y_values_json: &str,
        bounded_graph_y_values_json: &str,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimAxesQueryError> {
        let request = parse_riemann_request(request_json)?;
        let graph_y_values: Vec<f64> = serde_json::from_str(graph_y_values_json)
            .map_err(|error| ManimAxesQueryError::InvalidRiemannValues(error.to_string()))?;
        let bounded_graph_y_values: Option<Vec<f64>> =
            parse_optional_json(bounded_graph_y_values_json)
                .map_err(|error| ManimAxesQueryError::InvalidRiemannValues(error.to_string()))?;
        if request.bounded_graph_range.is_some() != bounded_graph_y_values.is_some() {
            return Err(ManimAxesQueryError::InvalidRiemannRequest(
                "bounded graph range/value presence must match".to_owned(),
            ));
        }

        let transformed = self.transformed_axes(x_axis_snapshot_json, y_axis_snapshot_json)?;
        let plan = riemann_plan(transformed, &request)?;
        let rectangles = plan.finish(&graph_y_values, bounded_graph_y_values.as_deref())?;
        let result: Vec<_> = rectangles
            .into_iter()
            .map(|rectangle| RiemannRectangleResult {
                negative_signed_area: rectangle.is_negative_signed_area(),
                snapshot: rectangle.into_snapshot(),
            })
            .collect();
        serde_json::to_string(&result)
            .map_err(|error| ManimAxesQueryError::Serialization(error.to_string()))
    }

    /// Resolve the clipped endpoint x values for ManimCE v0.21 `Axes.get_area`.
    pub fn area_endpoint_x_values_json(
        &self,
        request_json: &str,
        graph_snapshot_json: &str,
        bounded_graph_snapshot_json: &str,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimAxesQueryError> {
        let request = parse_area_request(request_json)?;
        let graph = parse_graph_snapshot(graph_snapshot_json)?;
        let bounded_graph = parse_optional_graph_snapshot(bounded_graph_snapshot_json)?;
        validate_bounded_presence(
            request.bounded_graph_range.is_some(),
            bounded_graph.is_some(),
            "area",
        )?;
        let transformed = self.transformed_axes(x_axis_snapshot_json, y_axis_snapshot_json)?;
        let plan = area_plan(transformed, &request, &graph, bounded_graph.as_ref())?;
        serde_json::to_string(&plan.graph_endpoint_x_values())
            .map_err(|error| ManimAxesQueryError::Serialization(error.to_string()))
    }

    /// Finish one ManimCE v0.21 `Axes.get_area` retained polygon snapshot.
    fn area_snapshot_json(
        &self,
        request_json: &str,
        finish: AreaFinishInput<'_>,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimAxesQueryError> {
        let request = parse_area_request(request_json)?;
        let graph = parse_graph_snapshot(finish.graph_snapshot_json)?;
        let bounded_graph = parse_optional_graph_snapshot(finish.bounded_graph_snapshot_json)?;
        validate_bounded_presence(
            request.bounded_graph_range.is_some(),
            bounded_graph.is_some(),
            "area",
        )?;
        let graph_endpoint_y_values: [f64; 2] =
            serde_json::from_str(finish.graph_endpoint_y_values_json)
                .map_err(|error| ManimAxesQueryError::InvalidAreaValues(error.to_string()))?;
        let bounded_graph_endpoint_y_values: Option<[f64; 2]> =
            parse_optional_json(finish.bounded_graph_endpoint_y_values_json)
                .map_err(|error| ManimAxesQueryError::InvalidAreaValues(error.to_string()))?;
        validate_bounded_presence(
            bounded_graph.is_some(),
            bounded_graph_endpoint_y_values.is_some(),
            "area endpoint values",
        )?;

        let transformed = self.transformed_axes(x_axis_snapshot_json, y_axis_snapshot_json)?;
        let plan = area_plan(transformed, &request, &graph, bounded_graph.as_ref())?;
        let snapshot = plan.finish(graph_endpoint_y_values, bounded_graph_endpoint_y_values)?;
        serde_json::to_string(&snapshot)
            .map_err(|error| ManimAxesQueryError::Serialization(error.to_string()))
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

fn parse_riemann_request(request_json: &str) -> Result<RiemannQueryRequest, ManimAxesQueryError> {
    let request: RiemannQueryRequest = serde_json::from_str(request_json)
        .map_err(|error| ManimAxesQueryError::InvalidRiemannRequest(error.to_string()))?;
    validate_range(request.graph_range, "graph")?;
    if let Some(range) = request.bounded_graph_range {
        validate_range(range, "bounded graph")?;
    }
    if let Some(range) = request.x_range {
        validate_range(range, "x_range")?;
    }
    Ok(request)
}

fn parse_area_request(request_json: &str) -> Result<AreaQueryRequest, ManimAxesQueryError> {
    let request: AreaQueryRequest = serde_json::from_str(request_json)
        .map_err(|error| ManimAxesQueryError::InvalidAreaRequest(error.to_string()))?;
    validate_range(request.graph_range, "graph")?;
    if let Some(range) = request.bounded_graph_range {
        validate_range(range, "bounded graph")?;
    }
    if let Some(range) = request.x_range {
        validate_range(range, "x_range")?;
    }
    Ok(request)
}

fn validate_range(range: [f64; 2], name: &str) -> Result<(), ManimAxesQueryError> {
    if range.into_iter().all(f64::is_finite) {
        Ok(())
    } else {
        Err(ManimAxesQueryError::InvalidCalculusRange(name.to_owned()))
    }
}

fn riemann_plan(
    axes: TransformedAxes2DState,
    request: &RiemannQueryRequest,
) -> Result<RiemannSamplePlan, ManimAxesQueryError> {
    let [x_min, x_max] = request.x_range.unwrap_or_else(|| {
        request
            .bounded_graph_range
            .map_or(request.graph_range, |bounded| {
                [
                    request.graph_range[0].max(bounded[0]),
                    request.graph_range[1].min(bounded[1]),
                ]
            })
    });
    Ok(RiemannSamplePlan::new(
        axes,
        x_min,
        x_max,
        request.dx,
        RiemannSampleType::try_from(request.input_sample_type.as_str())?,
        request.width_scale_factor,
    )?)
}

fn area_plan(
    axes: TransformedAxes2DState,
    request: &AreaQueryRequest,
    graph: &ObjectSnapshot,
    bounded_graph: Option<&ObjectSnapshot>,
) -> Result<AreaSamplePlan, ManimAxesQueryError> {
    let graph_range = GraphParameterRange::new(request.graph_range[0], request.graph_range[1])?;
    let bounded = bounded_graph
        .zip(request.bounded_graph_range)
        .map(|(graph, range)| {
            GraphParameterRange::new(range[0], range[1]).map(|range| (graph, range))
        });
    let bounded = match bounded {
        Some(result) => Some(result?),
        None => None,
    };
    Ok(AreaSamplePlan::new(
        axes,
        graph,
        graph_range,
        request.x_range,
        bounded,
    )?)
}

fn parse_graph_snapshot(snapshot_json: &str) -> Result<ObjectSnapshot, ManimAxesQueryError> {
    serde_json::from_str(snapshot_json)
        .map_err(|error| ManimAxesQueryError::InvalidGraphSnapshot(error.to_string()))
}

fn parse_optional_graph_snapshot(
    snapshot_json: &str,
) -> Result<Option<ObjectSnapshot>, ManimAxesQueryError> {
    if snapshot_json.is_empty() {
        Ok(None)
    } else {
        parse_graph_snapshot(snapshot_json).map(Some)
    }
}

fn parse_optional_json<T>(value: &str) -> Result<Option<T>, serde_json::Error>
where
    T: for<'de> Deserialize<'de>,
{
    if value.is_empty() {
        Ok(None)
    } else {
        serde_json::from_str(value).map(Some)
    }
}

fn validate_bounded_presence(
    expected: bool,
    actual: bool,
    context: &str,
) -> Result<(), ManimAxesQueryError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ManimAxesQueryError::InvalidAreaRequest(format!(
            "{context} bounded graph presence must match request"
        )))
    }
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
    InvalidRiemannRequest(String),
    InvalidRiemannValues(String),
    InvalidAreaRequest(String),
    InvalidAreaValues(String),
    InvalidCalculusRange(String),
    InvalidAxisSnapshot { axis: &'static str, error: String },
    InvalidAxisGeometry(&'static str),
    Coordinates(CoordinateSystemError),
    LineGraph(LineGraphAuthoringError),
    GraphQuery(GraphQueryError),
    Riemann(RiemannAuthoringError),
    Area(AreaAuthoringError),
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
            Self::InvalidRiemannRequest(error) => {
                write!(formatter, "invalid Axes Riemann request: {error}")
            }
            Self::InvalidRiemannValues(error) => {
                write!(formatter, "invalid Axes Riemann values: {error}")
            }
            Self::InvalidAreaRequest(error) => {
                write!(formatter, "invalid Axes area request: {error}")
            }
            Self::InvalidAreaValues(error) => {
                write!(formatter, "invalid Axes area values: {error}")
            }
            Self::InvalidCalculusRange(name) => {
                write!(formatter, "Axes calculus {name} range must be finite")
            }
            Self::InvalidAxisSnapshot { axis, error } => {
                write!(formatter, "invalid Axes {axis}-axis snapshot: {error}")
            }
            Self::InvalidAxisGeometry(axis) => write!(
                formatter,
                "Axes {axis}-axis snapshot must contain line geometry"
            ),
            Self::Coordinates(error) => error.fmt(formatter),
            Self::LineGraph(error) => error.fmt(formatter),
            Self::GraphQuery(error) => error.fmt(formatter),
            Self::Riemann(error) => error.fmt(formatter),
            Self::Area(error) => error.fmt(formatter),
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

impl From<GraphQueryError> for ManimAxesQueryError {
    fn from(value: GraphQueryError) -> Self {
        Self::GraphQuery(value)
    }
}

impl From<RiemannAuthoringError> for ManimAxesQueryError {
    fn from(value: RiemannAuthoringError) -> Self {
        Self::Riemann(value)
    }
}

impl From<AreaAuthoringError> for ManimAxesQueryError {
    fn from(value: AreaAuthoringError) -> Self {
        Self::Area(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{AreaFinishInput, AxesQueryPlan};

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

        #[wasm_bindgen(js_name = riemannSampleValuesJson)]
        pub fn riemann_sample_values_json(
            &self,
            request_json: &str,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .riemann_sample_values_json(
                    request_json,
                    x_axis_snapshot_json,
                    y_axis_snapshot_json,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = riemannRectanglesJson)]
        pub fn riemann_rectangles_json(
            &self,
            request_json: &str,
            graph_y_values_json: &str,
            bounded_graph_y_values_json: &str,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .riemann_rectangles_json(
                    request_json,
                    graph_y_values_json,
                    bounded_graph_y_values_json,
                    x_axis_snapshot_json,
                    y_axis_snapshot_json,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = areaEndpointXValuesJson)]
        pub fn area_endpoint_x_values_json(
            &self,
            request_json: &str,
            graph_snapshot_json: &str,
            bounded_graph_snapshot_json: &str,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .area_endpoint_x_values_json(
                    request_json,
                    graph_snapshot_json,
                    bounded_graph_snapshot_json,
                    x_axis_snapshot_json,
                    y_axis_snapshot_json,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = areaSnapshotJson)]
        pub fn area_snapshot_json(
            &self,
            request_json: &str,
            graph_snapshot_json: &str,
            bounded_graph_snapshot_json: &str,
            graph_endpoint_y_values_json: &str,
            bounded_graph_endpoint_y_values_json: &str,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .area_snapshot_json(
                    request_json,
                    AreaFinishInput {
                        graph_snapshot_json,
                        bounded_graph_snapshot_json,
                        graph_endpoint_y_values_json,
                        bounded_graph_endpoint_y_values_json,
                    },
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
    fn generic_graph_lookup_matches_shared_manim_binary_search() {
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
            assert!((coords[0] - 0.5).abs() <= noon::MANIM_GRAPH_X_SEARCH_TOLERANCE);
        }
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
            Err(ManimAxesQueryError::GraphQuery(
                GraphQueryError::GraphXOutOfRange { .. }
            ))
        ));

        let rectangle =
            serde_json::to_string(&ObjectSnapshot::new(GeometryRef::rectangle(1.0, 1.0))).unwrap();
        assert!(matches!(
            plan.graph_point_for_x_json(0.0, &rectangle, &x_axis, &y_axis),
            Err(ManimAxesQueryError::GraphQuery(
                GraphQueryError::GeometryProportion(_)
            ))
        ));
    }

    #[test]
    fn riemann_bridge_preserves_two_phase_callback_contract() {
        let plan = AxesQueryPlan::from_json(request_json()).unwrap();
        let (x_axis, y_axis) = identity_axes(&plan);
        let request = r#"{"graph_range":[-1,1],"bounded_graph_range":null,"x_range":[0,1],"dx":0.5,"input_sample_type":"center","width_scale_factor":1.001}"#;
        let values: RiemannSampleValues = serde_json::from_str(
            &plan
                .riemann_sample_values_json(request, &x_axis, &y_axis)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(values.graph, vec![0.25, 0.75]);
        assert_eq!(values.bounded_graph, None);

        let rectangles: Vec<RiemannRectangleResult> = serde_json::from_str(
            &plan
                .riemann_rectangles_json(request, "[1.0,-0.5]", "", &x_axis, &y_axis)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(rectangles.len(), 2);
        assert!(!rectangles[0].negative_signed_area);
        assert!(rectangles[1].negative_signed_area);
        assert!(rectangles
            .iter()
            .all(|result| matches!(result.snapshot.geometry, GeometryRef::Rectangle { .. })));
    }

    #[test]
    fn area_bridge_resolves_callbacks_then_returns_one_retained_polygon() {
        let plan = AxesQueryPlan::from_json(request_json()).unwrap();
        let (x_axis, y_axis) = identity_axes(&plan);
        let graph = plan
            .line_graph_snapshot_json("[[0.0,1.0,2.0],[0.0,1.0,0.0]]", &x_axis, &y_axis)
            .unwrap();
        let request = r#"{"graph_range":[0,2],"bounded_graph_range":null,"x_range":[0.5,1.5]}"#;
        let endpoints: [f64; 2] = serde_json::from_str(
            &plan
                .area_endpoint_x_values_json(request, &graph, "", &x_axis, &y_axis)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(endpoints, [0.5, 1.5]);

        let snapshot: ObjectSnapshot = serde_json::from_str(
            &plan
                .area_snapshot_json(
                    request,
                    AreaFinishInput {
                        graph_snapshot_json: &graph,
                        bounded_graph_snapshot_json: "",
                        graph_endpoint_y_values_json: "[0.5,0.5]",
                        bounded_graph_endpoint_y_values_json: "",
                    },
                    &x_axis,
                    &y_axis,
                )
                .unwrap(),
        )
        .unwrap();
        assert!(matches!(snapshot.geometry, GeometryRef::VectorPath(_)));
    }
}
