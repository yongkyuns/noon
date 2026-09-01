use noon::{
    parametric_vector_path, IntoSnapshot, ParametricSamplePlan, Path, PlotGeometryError,
    PlotSamplingError, SampleRange, MANIM_DEFAULT_DISCONTINUITY_DT,
};
use noon_core::{ObjectSnapshot, Vec2};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct ParametricFunctionPlanRequest {
    t_range: Vec<f64>,
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

/// Two-phase authoring plan for ManimCE v0.21 scene-space `ParametricFunction`.
///
/// Rust owns deterministic parameter generation, discontinuity splitting,
/// callback-result cardinality, smoothing, and retained `VectorPath` construction.
/// The Python host only evaluates arbitrary user callbacks at Rust-requested values.
#[derive(Clone, Debug, PartialEq)]
pub struct ParametricFunctionAuthoringPlan {
    samples: ParametricSamplePlan,
    use_smoothing: bool,
}

impl ParametricFunctionAuthoringPlan {
    pub fn from_json(request_json: &str) -> Result<Self, ManimParametricBridgeError> {
        let request: ParametricFunctionPlanRequest = serde_json::from_str(request_json)
            .map_err(|error| ManimParametricBridgeError::InvalidRequest(error.to_string()))?;
        Self::new(request)
    }

    fn new(request: ParametricFunctionPlanRequest) -> Result<Self, ManimParametricBridgeError> {
        let sample_range = match request.t_range.as_slice() {
            [min, max] => SampleRange::parametric_bounds(*min, *max)?,
            [min, max, step] => SampleRange::new(*min, *max, *step)?,
            values => return Err(ManimParametricBridgeError::InvalidRangeLength(values.len())),
        };
        let samples = match request.discontinuities {
            Some(discontinuities) => ParametricSamplePlan::new(
                sample_range,
                &discontinuities,
                request.discontinuity_dt,
            )?,
            None => ParametricSamplePlan::without_discontinuities(sample_range),
        };
        Ok(Self {
            samples,
            use_smoothing: request.use_smoothing,
        })
    }

    pub fn parameter_subpaths(&self) -> Result<Vec<Vec<f64>>, ManimParametricBridgeError> {
        Ok(self.samples.parameter_subpaths()?)
    }

    pub fn parameters_json(&self) -> Result<String, ManimParametricBridgeError> {
        serde_json::to_string(&self.parameter_subpaths()?)
            .map_err(|error| ManimParametricBridgeError::Serialization(error.to_string()))
    }

    pub fn finish_values(
        &self,
        values: &[Vec<[f64; 2]>],
    ) -> Result<ObjectSnapshot, ManimParametricBridgeError> {
        let parameters = self.samples.parameter_subpaths()?;
        if values.len() != parameters.len() {
            return Err(PlotGeometryError::SampleSubpathCountMismatch {
                expected: parameters.len(),
                actual: values.len(),
            }
            .into());
        }

        let mut points = Vec::new();
        for (subpath, (parameter_values, coordinates)) in parameters.iter().zip(values).enumerate() {
            if coordinates.len() != parameter_values.len() {
                return Err(PlotGeometryError::SampleValueCountMismatch {
                    subpath,
                    expected: parameter_values.len(),
                    actual: coordinates.len(),
                }
                .into());
            }
            points.try_reserve(coordinates.len()).map_err(|_| {
                ManimParametricBridgeError::PointAllocationFailed(coordinates.len())
            })?;
            for (&parameter, &[x, y]) in parameter_values.iter().zip(coordinates) {
                if !x.is_finite() || !y.is_finite() {
                    return Err(PlotGeometryError::NonFinitePoint {
                        parameter,
                        point: Vec2::new(x as f32, y as f32),
                    }
                    .into());
                }
                points.push(Vec2::new(x as f32, y as f32));
            }
        }

        let mut points = points.into_iter();
        let path = parametric_vector_path(
            &self.samples,
            |_| points.next().expect("validated parametric callback cardinality"),
            self.use_smoothing,
        )?;
        debug_assert!(points.next().is_none());
        Ok(Path::new(path).into_snapshot())
    }

    pub fn finish_snapshot_json(
        &self,
        values_json: &str,
    ) -> Result<String, ManimParametricBridgeError> {
        let values: Vec<Vec<[f64; 2]>> = serde_json::from_str(values_json)
            .map_err(|error| ManimParametricBridgeError::InvalidCallbackValues(error.to_string()))?;
        serde_json::to_string(&self.finish_values(&values)?)
            .map_err(|error| ManimParametricBridgeError::Serialization(error.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManimParametricBridgeError {
    InvalidRequest(String),
    InvalidCallbackValues(String),
    InvalidRangeLength(usize),
    PointAllocationFailed(usize),
    Sampling(PlotSamplingError),
    Geometry(PlotGeometryError),
    Serialization(String),
}

impl std::fmt::Display for ManimParametricBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => {
                write!(formatter, "invalid ParametricFunction request: {error}")
            }
            Self::InvalidCallbackValues(error) => {
                write!(formatter, "invalid ParametricFunction callback values: {error}")
            }
            Self::InvalidRangeLength(length) => write!(
                formatter,
                "ParametricFunction t_range must contain 2 or 3 values, got {length}"
            ),
            Self::PointAllocationFailed(count) => {
                write!(formatter, "unable to allocate {count} parametric points")
            }
            Self::Sampling(error) => error.fmt(formatter),
            Self::Geometry(error) => error.fmt(formatter),
            Self::Serialization(error) => {
                write!(formatter, "unable to serialize ParametricFunction state: {error}")
            }
        }
    }
}

impl std::error::Error for ManimParametricBridgeError {}

impl From<PlotSamplingError> for ManimParametricBridgeError {
    fn from(value: PlotSamplingError) -> Self {
        Self::Sampling(value)
    }
}

impl From<PlotGeometryError> for ManimParametricBridgeError {
    fn from(value: PlotGeometryError) -> Self {
        Self::Geometry(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::ParametricFunctionAuthoringPlan;

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    #[wasm_bindgen]
    pub struct WasmParametricFunctionPlan(ParametricFunctionAuthoringPlan);

    #[wasm_bindgen]
    impl WasmParametricFunctionPlan {
        #[wasm_bindgen(constructor)]
        pub fn new(request_json: &str) -> Result<Self, JsValue> {
            ParametricFunctionAuthoringPlan::from_json(request_json)
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
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmParametricFunctionPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{GeometryRef, PathCommand};

    fn request_json(t_range: &str, discontinuities: &str, use_smoothing: bool) -> String {
        format!(
            r#"{{"t_range":{t_range},"discontinuities":{discontinuities},"use_smoothing":{use_smoothing}}}"#
        )
    }

    #[test]
    fn two_value_range_uses_manim_default_parametric_step() {
        let plan = ParametricFunctionAuthoringPlan::from_json(&request_json(
            "[0.0,1.0]",
            "null",
            false,
        ))
        .unwrap();
        let parameters = plan.parameter_subpaths().unwrap();
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].first(), Some(&0.0));
        assert_eq!(parameters[0].last(), Some(&1.0));
        assert_eq!(parameters[0].len(), 101);
    }

    #[test]
    fn direct_values_lower_to_scene_space_vector_path() {
        let plan = ParametricFunctionAuthoringPlan::from_json(&request_json(
            "[-1.0,1.0,1.0]",
            "null",
            false,
        ))
        .unwrap();
        let snapshot = plan
            .finish_values(&[vec![[-1.0, 0.5], [0.0, -0.5], [1.0, 0.5]]])
            .unwrap();
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("ParametricFunction must lower to ordinary VectorPath geometry");
        };
        let expected = [
            Vec2::new(-1.0, 0.5),
            Vec2::new(0.0, -0.5),
            Vec2::new(1.0, 0.5),
        ];
        assert_eq!(path.commands().len(), 3);
        for (command, expected) in path.commands().iter().zip(expected) {
            match command {
                PathCommand::MoveTo { to } | PathCommand::LineTo { to } => {
                    assert!((*to - expected).length() <= 1.0e-6)
                }
                other => panic!("expected corner path command, got {other:?}"),
            }
        }
    }

    #[test]
    fn discontinuity_branch_remains_shared_with_axes_sampling() {
        let plan = ParametricFunctionAuthoringPlan::from_json(&request_json(
            "[-1.0,1.0,1.0]",
            "[0.0]",
            false,
        ))
        .unwrap();
        let parameters = plan.parameter_subpaths().unwrap();
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].first(), Some(&-1.0));
        assert_eq!(parameters[1].last(), Some(&1.0));
    }

    #[test]
    fn callback_cardinality_and_finiteness_are_rust_validated() {
        let plan = ParametricFunctionAuthoringPlan::from_json(&request_json(
            "[-1.0,1.0,1.0]",
            "null",
            false,
        ))
        .unwrap();
        assert!(matches!(
            plan.finish_values(&[vec![[0.0, 0.0]]]).unwrap_err(),
            ManimParametricBridgeError::Geometry(
                PlotGeometryError::SampleValueCountMismatch {
                    subpath: 0,
                    expected: 3,
                    actual: 1,
                }
            )
        ));
        assert!(matches!(
            plan.finish_values(&[vec![
                [-1.0, 0.0],
                [0.0, f64::NAN],
                [1.0, 0.0],
            ]])
            .unwrap_err(),
            ManimParametricBridgeError::Geometry(PlotGeometryError::NonFinitePoint { .. })
        ));
    }
}
