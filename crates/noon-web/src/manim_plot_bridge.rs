use noon::{
    axes_sampled_values_vector_path, parametric_vector_path,
    transformed_axes_sampled_values_vector_path, Axes2DState, CoordinateSystemError, IntoSnapshot,
    NumberRange, ParametricSamplePlan, Path, PlotGeometryError, PlotRangeRequest,
    PlotSamplingError, SampleRange, TransformedAxes2DState, MANIM_DEFAULT_DISCONTINUITY_DT,
};
use noon_core::{GeometryRef, ObjectSnapshot, Vec2};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AxesPlotPlanRequest {
    x_range: [f64; 3],
    y_range: [f64; 3],
    x_length: f32,
    y_length: f32,
    #[serde(default)]
    plot_range: Option<Vec<f64>>,
    #[serde(default)]
    discontinuities: Option<Vec<f64>>,
    #[serde(default = "default_discontinuity_dt")]
    discontinuity_dt: f64,
    #[serde(default = "default_true")]
    use_smoothing: bool,
}

const fn default_discontinuity_dt() -> f64 {
    MANIM_DEFAULT_DISCONTINUITY_DT
}

const fn default_true() -> bool {
    true
}

/// Two-phase browser authoring plan for ManimCE v0.21 Axes graphing.
///
/// Rust owns every deterministic semantic rule: axis construction, parameter-range
/// interpretation, parameter generation, discontinuity splitting, callback-result
/// cardinality, coordinate mapping, smoothing, and final `ObjectSnapshot` creation.
/// The host receives only parameters to evaluate and returns either one scalar y value
/// (`Axes.plot`) or one axis-coordinate pair (`Axes.plot_parametric_curve`) per parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct AxesPlotAuthoringPlan {
    axes: Axes2DState,
    samples: ParametricSamplePlan,
    use_smoothing: bool,
}

impl AxesPlotAuthoringPlan {
    pub fn from_json(request_json: &str) -> Result<Self, ManimPlotBridgeError> {
        let request: AxesPlotPlanRequest = serde_json::from_str(request_json)
            .map_err(|error| ManimPlotBridgeError::InvalidRequest(error.to_string()))?;
        Self::new(request)
    }

    fn new(request: AxesPlotPlanRequest) -> Result<Self, ManimPlotBridgeError> {
        let x_range = NumberRange::new(request.x_range[0], request.x_range[1], request.x_range[2])?;
        let y_range = NumberRange::new(request.y_range[0], request.y_range[1], request.y_range[2])?;
        let axes = Axes2DState::new(x_range, y_range, request.x_length, request.y_length)?;
        let range_request = match request.plot_range.as_deref() {
            None => PlotRangeRequest::AxisDefault,
            Some([min, max]) => PlotRangeRequest::Bounds {
                min: *min,
                max: *max,
            },
            Some([min, max, step]) => PlotRangeRequest::Explicit {
                min: *min,
                max: *max,
                step: *step,
            },
            Some(values) => {
                return Err(ManimPlotBridgeError::InvalidPlotRangeLength(values.len()));
            }
        };
        let sample_range = SampleRange::for_axes_plot(x_range, range_request)?;
        let samples = match request.discontinuities {
            Some(discontinuities) => {
                ParametricSamplePlan::new(sample_range, &discontinuities, request.discontinuity_dt)?
            }
            None => ParametricSamplePlan::without_discontinuities(sample_range),
        };

        Ok(Self {
            axes,
            samples,
            use_smoothing: request.use_smoothing,
        })
    }

    pub fn parameter_subpaths(&self) -> Result<Vec<Vec<f64>>, ManimPlotBridgeError> {
        Ok(self.samples.parameter_subpaths()?)
    }

    pub fn parameters_json(&self) -> Result<String, ManimPlotBridgeError> {
        serde_json::to_string(&self.parameter_subpaths()?)
            .map_err(|error| ManimPlotBridgeError::Serialization(error.to_string()))
    }

    pub fn finish_values(
        &self,
        values: &[Vec<f64>],
    ) -> Result<ObjectSnapshot, ManimPlotBridgeError> {
        let path =
            axes_sampled_values_vector_path(self.axes, &self.samples, values, self.use_smoothing)?;
        Ok(Path::new(path).into_snapshot())
    }

    /// Finish scalar `Axes.plot` callback values against current retained axis state.
    pub fn finish_values_with_axes(
        &self,
        values: &[Vec<f64>],
        x_axis: &ObjectSnapshot,
        y_axis: &ObjectSnapshot,
    ) -> Result<ObjectSnapshot, ManimPlotBridgeError> {
        let transformed = self.transformed_axes(x_axis, y_axis)?;
        let path = transformed_axes_sampled_values_vector_path(
            transformed,
            &self.samples,
            values,
            self.use_smoothing,
        )?;
        Ok(Path::new(path).into_snapshot())
    }

    /// Finish vector-valued parametric callback samples against current Axes state.
    pub fn finish_parametric_values_with_axes(
        &self,
        values: &[Vec<[f64; 2]>],
        x_axis: &ObjectSnapshot,
        y_axis: &ObjectSnapshot,
    ) -> Result<ObjectSnapshot, ManimPlotBridgeError> {
        let transformed = self.transformed_axes(x_axis, y_axis)?;
        let parameter_subpaths = self.samples.parameter_subpaths()?;
        if values.len() != parameter_subpaths.len() {
            return Err(PlotGeometryError::SampleSubpathCountMismatch {
                expected: parameter_subpaths.len(),
                actual: values.len(),
            }
            .into());
        }

        let mut scene_subpaths = Vec::with_capacity(parameter_subpaths.len());
        for (subpath, (parameters, coordinates)) in
            parameter_subpaths.iter().zip(values).enumerate()
        {
            if coordinates.len() != parameters.len() {
                return Err(PlotGeometryError::SampleValueCountMismatch {
                    subpath,
                    expected: parameters.len(),
                    actual: coordinates.len(),
                }
                .into());
            }
            let mut scene_points = Vec::with_capacity(parameters.len());
            for (&parameter, &[x, y]) in parameters.iter().zip(coordinates) {
                if !x.is_finite() || !y.is_finite() {
                    return Err(PlotGeometryError::NonFinitePoint {
                        parameter,
                        point: Vec2::new(x as f32, y as f32),
                    }
                    .into());
                }
                scene_points.push(transformed.coords_to_point(x, y)?);
            }
            scene_subpaths.push(scene_points);
        }

        // `parametric_vector_path` remains the single shared owner of subpath construction
        // and smoothing. Cardinality is validated above, so this iterator is exact by design.
        let mut scene_points = scene_subpaths.iter().flatten().copied();
        let path = parametric_vector_path(
            &self.samples,
            |_| {
                scene_points
                    .next()
                    .expect("validated parametric callback cardinality")
            },
            self.use_smoothing,
        )?;
        debug_assert!(scene_points.next().is_none());
        Ok(Path::new(path).into_snapshot())
    }

    pub fn finish_snapshot_json(&self, values_json: &str) -> Result<String, ManimPlotBridgeError> {
        let values = parse_callback_values(values_json)?;
        serialize_snapshot(&self.finish_values(&values)?)
    }

    pub fn finish_snapshot_json_with_axes(
        &self,
        values_json: &str,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimPlotBridgeError> {
        let values = parse_callback_values(values_json)?;
        let x_axis = parse_axis_snapshot(x_axis_snapshot_json, "x")?;
        let y_axis = parse_axis_snapshot(y_axis_snapshot_json, "y")?;
        serialize_snapshot(&self.finish_values_with_axes(&values, &x_axis, &y_axis)?)
    }

    pub fn finish_parametric_snapshot_json_with_axes(
        &self,
        values_json: &str,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimPlotBridgeError> {
        let values = parse_parametric_callback_values(values_json)?;
        let x_axis = parse_axis_snapshot(x_axis_snapshot_json, "x")?;
        let y_axis = parse_axis_snapshot(y_axis_snapshot_json, "y")?;
        serialize_snapshot(&self.finish_parametric_values_with_axes(
            &values,
            &x_axis,
            &y_axis,
        )?)
    }

    fn transformed_axes(
        &self,
        x_axis: &ObjectSnapshot,
        y_axis: &ObjectSnapshot,
    ) -> Result<TransformedAxes2DState, ManimPlotBridgeError> {
        ensure_axis_line(x_axis, "x")?;
        ensure_axis_line(y_axis, "y")?;
        Ok(TransformedAxes2DState::new(
            self.axes,
            x_axis.transform,
            y_axis.transform,
        ))
    }
}

fn parse_callback_values(values_json: &str) -> Result<Vec<Vec<f64>>, ManimPlotBridgeError> {
    serde_json::from_str(values_json)
        .map_err(|error| ManimPlotBridgeError::InvalidCallbackValues(error.to_string()))
}

fn parse_parametric_callback_values(
    values_json: &str,
) -> Result<Vec<Vec<[f64; 2]>>, ManimPlotBridgeError> {
    serde_json::from_str(values_json)
        .map_err(|error| ManimPlotBridgeError::InvalidCallbackValues(error.to_string()))
}

fn parse_axis_snapshot(
    snapshot_json: &str,
    axis: &'static str,
) -> Result<ObjectSnapshot, ManimPlotBridgeError> {
    serde_json::from_str(snapshot_json).map_err(|error| ManimPlotBridgeError::InvalidAxisSnapshot {
        axis,
        error: error.to_string(),
    })
}

fn ensure_axis_line(
    snapshot: &ObjectSnapshot,
    axis: &'static str,
) -> Result<(), ManimPlotBridgeError> {
    if matches!(snapshot.geometry, GeometryRef::Line { .. }) {
        Ok(())
    } else {
        Err(ManimPlotBridgeError::InvalidAxisGeometry(axis))
    }
}

fn serialize_snapshot(snapshot: &ObjectSnapshot) -> Result<String, ManimPlotBridgeError> {
    serde_json::to_string(snapshot)
        .map_err(|error| ManimPlotBridgeError::Serialization(error.to_string()))
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManimPlotBridgeError {
    InvalidRequest(String),
    InvalidCallbackValues(String),
    InvalidPlotRangeLength(usize),
    InvalidAxisSnapshot { axis: &'static str, error: String },
    InvalidAxisGeometry(&'static str),
    Coordinates(CoordinateSystemError),
    Sampling(PlotSamplingError),
    Geometry(PlotGeometryError),
    Serialization(String),
}

impl std::fmt::Display for ManimPlotBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => write!(formatter, "invalid Axes plot request: {error}"),
            Self::InvalidCallbackValues(error) => {
                write!(formatter, "invalid Axes plot callback values: {error}")
            }
            Self::InvalidPlotRangeLength(length) => write!(
                formatter,
                "Axes plot range must contain 2 or 3 values, got {length}"
            ),
            Self::InvalidAxisSnapshot { axis, error } => {
                write!(formatter, "invalid Axes plot {axis}-axis snapshot: {error}")
            }
            Self::InvalidAxisGeometry(axis) => {
                write!(
                    formatter,
                    "Axes plot {axis}-axis snapshot must contain line geometry"
                )
            }
            Self::Coordinates(error) => error.fmt(formatter),
            Self::Sampling(error) => error.fmt(formatter),
            Self::Geometry(error) => error.fmt(formatter),
            Self::Serialization(error) => {
                write!(formatter, "unable to serialize Axes plot state: {error}")
            }
        }
    }
}

impl std::error::Error for ManimPlotBridgeError {}

impl From<CoordinateSystemError> for ManimPlotBridgeError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl From<PlotSamplingError> for ManimPlotBridgeError {
    fn from(value: PlotSamplingError) -> Self {
        Self::Sampling(value)
    }
}

impl From<PlotGeometryError> for ManimPlotBridgeError {
    fn from(value: PlotGeometryError) -> Self {
        Self::Geometry(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::AxesPlotAuthoringPlan;

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    #[wasm_bindgen]
    pub struct WasmAxesPlotPlan(AxesPlotAuthoringPlan);

    #[wasm_bindgen]
    impl WasmAxesPlotPlan {
        #[wasm_bindgen(constructor)]
        pub fn new(request_json: &str) -> Result<Self, JsValue> {
            AxesPlotAuthoringPlan::from_json(request_json)
                .map(Self)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = parametersJson)]
        pub fn parameters_json(&self) -> Result<String, JsValue> {
            self.0.parameters_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = finishSnapshotJson)]
        pub fn finish_snapshot_json(&self, values_json: &str) -> Result<String, JsValue> {
            self.0.finish_snapshot_json(values_json).map_err(js_error)
        }

        #[wasm_bindgen(js_name = finishSnapshotJsonWithAxes)]
        pub fn finish_snapshot_json_with_axes(
            &self,
            values_json: &str,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .finish_snapshot_json_with_axes(
                    values_json,
                    x_axis_snapshot_json,
                    y_axis_snapshot_json,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = finishParametricSnapshotJsonWithAxes)]
        pub fn finish_parametric_snapshot_json_with_axes(
            &self,
            values_json: &str,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .finish_parametric_snapshot_json_with_axes(
                    values_json,
                    x_axis_snapshot_json,
                    y_axis_snapshot_json,
                )
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmAxesPlotPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use noon::Line;
    use noon_core::{GeometryRef, PathCommand, Transform2D, Vec2};

    fn request_json(plot_range: &str, discontinuities: &str, use_smoothing: bool) -> String {
        format!(
            r#"{{"x_range":[-2.0,2.0,1.0],"y_range":[-2.0,2.0,1.0],"x_length":4.0,"y_length":4.0,"plot_range":{plot_range},"discontinuities":{discontinuities},"use_smoothing":{use_smoothing}}}"#
        )
    }

    fn axis_snapshot(
        plan: &AxesPlotAuthoringPlan,
        x_axis: bool,
        transform: Transform2D,
    ) -> ObjectSnapshot {
        let axis = if x_axis {
            plan.axes.x_axis()
        } else {
            plan.axes.y_axis()
        };
        let mut snapshot = Line::new(axis.start(), axis.end()).into_snapshot();
        snapshot.transform = transform;
        snapshot
    }

    #[test]
    fn rust_plan_owns_axes_default_sample_frequency() {
        let plan = AxesPlotAuthoringPlan::from_json(&request_json("null", "null", false)).unwrap();
        let parameters = plan.parameter_subpaths().unwrap();
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].first(), Some(&-2.0));
        assert_eq!(parameters[0].last(), Some(&2.0));
        assert_eq!(parameters[0].len(), 41);
    }

    #[test]
    fn explicit_empty_discontinuities_preserve_manim_some_branch() {
        let plan = AxesPlotAuthoringPlan::from_json(&request_json("[2.0,-1.0,-0.5]", "[]", false))
            .unwrap();
        assert_eq!(plan.parameter_subpaths().unwrap(), vec![vec![2.0]]);
    }

    #[test]
    fn host_values_finish_as_ordinary_vector_path_snapshot() {
        let plan = AxesPlotAuthoringPlan::from_json(&request_json("[-1.0,1.0,1.0]", "null", true))
            .unwrap();
        let snapshot = plan.finish_values(&[vec![1.0, 0.0, 1.0]]).unwrap();
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("Axes.plot must lower to ordinary VectorPath geometry");
        };
        assert_eq!(path.commands().len(), 3);
        assert!(matches!(path.commands()[0], PathCommand::MoveTo { .. }));
        assert!(matches!(path.commands()[1], PathCommand::CubicTo { .. }));
        assert!(matches!(path.commands()[2], PathCommand::CubicTo { .. }));
    }

    #[test]
    fn current_axis_snapshots_drive_transformed_plot_geometry() {
        let plan = AxesPlotAuthoringPlan::from_json(&request_json("[-1.0,1.0,1.0]", "null", false))
            .unwrap();
        let transform = Transform2D {
            translation: Vec2::new(2.0, -1.0),
            rotation: 0.4,
            scale: Vec2::new(1.5, 1.5),
        };
        let x_axis = axis_snapshot(&plan, true, transform);
        let y_axis = axis_snapshot(&plan, false, transform);
        let snapshot = plan
            .finish_values_with_axes(&[vec![1.0, 0.0, 1.0]], &x_axis, &y_axis)
            .unwrap();
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("Axes.plot must lower to ordinary VectorPath geometry");
        };
        let expected = [
            transform.transform_point(Vec2::new(-1.0, 1.0)),
            transform.transform_point(Vec2::ZERO),
            transform.transform_point(Vec2::new(1.0, 1.0)),
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
    fn parametric_callback_values_use_current_axis_state() {
        let plan = AxesPlotAuthoringPlan::from_json(&request_json("[-1.0,1.0,1.0]", "null", false))
            .unwrap();
        let transform = Transform2D {
            translation: Vec2::new(-1.5, 2.0),
            rotation: -0.25,
            scale: Vec2::new(0.75, 0.75),
        };
        let x_axis = axis_snapshot(&plan, true, transform);
        let y_axis = axis_snapshot(&plan, false, transform);
        let snapshot = plan
            .finish_parametric_values_with_axes(
                &[vec![[-1.0, 0.5], [0.0, -0.5], [1.0, 0.5]]],
                &x_axis,
                &y_axis,
            )
            .unwrap();
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("Axes.plot_parametric_curve must lower to ordinary VectorPath geometry");
        };
        let expected = [
            transform.transform_point(Vec2::new(-1.0, 0.5)),
            transform.transform_point(Vec2::new(0.0, -0.5)),
            transform.transform_point(Vec2::new(1.0, 0.5)),
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
    fn non_line_axis_snapshot_is_rejected() {
        let plan = AxesPlotAuthoringPlan::from_json(&request_json("[-1.0,1.0,1.0]", "null", false))
            .unwrap();
        let mut x_axis = axis_snapshot(&plan, true, Transform2D::IDENTITY);
        x_axis.geometry = GeometryRef::circle(1.0);
        let y_axis = axis_snapshot(&plan, false, Transform2D::IDENTITY);
        assert_eq!(
            plan.finish_values_with_axes(&[vec![1.0, 0.0, 1.0]], &x_axis, &y_axis)
                .unwrap_err(),
            ManimPlotBridgeError::InvalidAxisGeometry("x")
        );
    }

    #[test]
    fn callback_value_shape_is_validated_in_rust() {
        let plan = AxesPlotAuthoringPlan::from_json(&request_json("[-1.0,1.0,1.0]", "null", false))
            .unwrap();
        let error = plan.finish_values(&[vec![0.0]]).unwrap_err();
        assert!(matches!(
            error,
            ManimPlotBridgeError::Geometry(PlotGeometryError::SampleValueCountMismatch {
                subpath: 0,
                expected: 3,
                actual: 1,
            })
        ));
        let error = plan
            .finish_parametric_values_with_axes(
                &[vec![[0.0, 0.0]]],
                &axis_snapshot(&plan, true, Transform2D::IDENTITY),
                &axis_snapshot(&plan, false, Transform2D::IDENTITY),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            ManimPlotBridgeError::Geometry(PlotGeometryError::SampleValueCountMismatch {
                subpath: 0,
                expected: 3,
                actual: 1,
            })
        ));
    }
}
