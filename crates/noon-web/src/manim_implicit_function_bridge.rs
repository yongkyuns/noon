use noon::{
    Axes2DState, CoordinateSystemError, ImplicitFunctionAuthoringError, ImplicitFunctionPlan,
    IntoSnapshot, NumberRange, Path, TransformedAxes2DState,
};
use noon_core::ObjectSnapshot;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ImplicitFunctionRequest {
    x_range: [f64; 2],
    y_range: [f64; 2],
    #[serde(default = "default_min_depth")]
    min_depth: usize,
    #[serde(default = "default_max_quads")]
    max_quads: usize,
    #[serde(default = "default_true")]
    use_smoothing: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AxesMappingRequest {
    x_range: [f64; 3],
    y_range: [f64; 3],
    x_length: f32,
    y_length: f32,
}

const fn default_min_depth() -> usize {
    5
}

const fn default_max_quads() -> usize {
    1500
}

const fn default_true() -> bool {
    true
}

/// Browser-neutral request wrapper over the shared adaptive implicit contour core.
///
/// The scalar evaluator is supplied only while finishing the immutable geometry.
/// Subdivision, zero refinement, curve topology, smoothing, and retained path
/// construction remain owned by `ImplicitFunctionPlan`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImplicitFunctionAuthoringPlan {
    plan: ImplicitFunctionPlan,
}

impl ImplicitFunctionAuthoringPlan {
    pub fn from_json(request_json: &str) -> Result<Self, ManimImplicitFunctionBridgeError> {
        let request: ImplicitFunctionRequest = serde_json::from_str(request_json)
            .map_err(|error| ManimImplicitFunctionBridgeError::InvalidRequest(error.to_string()))?;
        Ok(Self {
            plan: ImplicitFunctionPlan::new(
                request.x_range,
                request.y_range,
                request.min_depth,
                request.max_quads,
                request.use_smoothing,
            )?,
        })
    }

    pub fn finish_with_evaluator<F>(
        &self,
        evaluator: F,
    ) -> Result<ObjectSnapshot, ManimImplicitFunctionBridgeError>
    where
        F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
    {
        Ok(Path::new(self.plan.vector_path_with_evaluator(evaluator)?).into_snapshot())
    }

    pub fn finish_with_evaluator_and_axes<F>(
        &self,
        evaluator: F,
        axes_request_json: &str,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<ObjectSnapshot, ManimImplicitFunctionBridgeError>
    where
        F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
    {
        let request: AxesMappingRequest =
            serde_json::from_str(axes_request_json).map_err(|error| {
                ManimImplicitFunctionBridgeError::InvalidAxesRequest(error.to_string())
            })?;
        let x_range = NumberRange::new(request.x_range[0], request.x_range[1], request.x_range[2])?;
        let y_range = NumberRange::new(request.y_range[0], request.y_range[1], request.y_range[2])?;
        let axes = Axes2DState::new(x_range, y_range, request.x_length, request.y_length)?;
        let x_axis = parse_axis_snapshot(x_axis_snapshot_json, "x")?;
        let y_axis = parse_axis_snapshot(y_axis_snapshot_json, "y")?;
        let transformed = TransformedAxes2DState::new(axes, x_axis.transform, y_axis.transform);
        let path = self
            .plan
            .vector_path_with_evaluator_and_mapper(evaluator, |x, y| {
                transformed.coords_to_point(x, y)
            })?;
        Ok(Path::new(path).into_snapshot())
    }

    pub fn finish_snapshot_json_with_evaluator<F>(
        &self,
        evaluator: F,
    ) -> Result<String, ManimImplicitFunctionBridgeError>
    where
        F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
    {
        serialize_snapshot(&self.finish_with_evaluator(evaluator)?)
    }

    pub fn finish_snapshot_json_with_evaluator_and_axes<F>(
        &self,
        evaluator: F,
        axes_request_json: &str,
        x_axis_snapshot_json: &str,
        y_axis_snapshot_json: &str,
    ) -> Result<String, ManimImplicitFunctionBridgeError>
    where
        F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
    {
        serialize_snapshot(&self.finish_with_evaluator_and_axes(
            evaluator,
            axes_request_json,
            x_axis_snapshot_json,
            y_axis_snapshot_json,
        )?)
    }
}

fn parse_axis_snapshot(
    snapshot_json: &str,
    axis: &'static str,
) -> Result<ObjectSnapshot, ManimImplicitFunctionBridgeError> {
    serde_json::from_str(snapshot_json).map_err(|error| {
        ManimImplicitFunctionBridgeError::InvalidAxisSnapshot {
            axis,
            message: error.to_string(),
        }
    })
}

fn serialize_snapshot(
    snapshot: &ObjectSnapshot,
) -> Result<String, ManimImplicitFunctionBridgeError> {
    serde_json::to_string(snapshot)
        .map_err(|error| ManimImplicitFunctionBridgeError::Serialization(error.to_string()))
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManimImplicitFunctionBridgeError {
    InvalidRequest(String),
    InvalidAxesRequest(String),
    InvalidAxisSnapshot { axis: &'static str, message: String },
    Coordinates(CoordinateSystemError),
    Geometry(ImplicitFunctionAuthoringError),
    Serialization(String),
}

impl std::fmt::Display for ManimImplicitFunctionBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => {
                write!(formatter, "invalid ImplicitFunction request: {error}")
            }
            Self::InvalidAxesRequest(error) => {
                write!(formatter, "invalid ImplicitFunction Axes request: {error}")
            }
            Self::InvalidAxisSnapshot { axis, message } => write!(
                formatter,
                "invalid ImplicitFunction {axis}-axis snapshot: {message}"
            ),
            Self::Coordinates(error) => error.fmt(formatter),
            Self::Geometry(error) => error.fmt(formatter),
            Self::Serialization(error) => {
                write!(
                    formatter,
                    "unable to serialize ImplicitFunction geometry: {error}"
                )
            }
        }
    }
}

impl std::error::Error for ManimImplicitFunctionBridgeError {}

impl From<CoordinateSystemError> for ManimImplicitFunctionBridgeError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl From<ImplicitFunctionAuthoringError> for ManimImplicitFunctionBridgeError {
    fn from(value: ImplicitFunctionAuthoringError) -> Self {
        Self::Geometry(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use js_sys::Function;
    use wasm_bindgen::prelude::*;

    use noon::ImplicitFunctionAuthoringError;

    use super::ImplicitFunctionAuthoringPlan;

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn evaluate_callback(
        callback: &Function,
        x: f64,
        y: f64,
    ) -> Result<f64, ImplicitFunctionAuthoringError> {
        let value = callback
            .call2(
                &JsValue::UNDEFINED,
                &JsValue::from_f64(x),
                &JsValue::from_f64(y),
            )
            .map_err(|error| {
                ImplicitFunctionAuthoringError::Callback(
                    error
                        .as_string()
                        .unwrap_or_else(|| "JavaScript callback threw an exception".to_owned()),
                )
            })?;
        value.as_f64().ok_or_else(|| {
            ImplicitFunctionAuthoringError::Callback(
                "callback must return a Python/JavaScript numeric value".to_owned(),
            )
        })
    }

    #[wasm_bindgen]
    pub struct WasmImplicitFunctionPlan(ImplicitFunctionAuthoringPlan);

    #[wasm_bindgen]
    impl WasmImplicitFunctionPlan {
        #[wasm_bindgen(constructor)]
        pub fn new(request_json: &str) -> Result<Self, JsValue> {
            ImplicitFunctionAuthoringPlan::from_json(request_json)
                .map(Self)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = finishSnapshotJson)]
        pub fn finish_snapshot_json(&self, callback: &Function) -> Result<String, JsValue> {
            self.0
                .finish_snapshot_json_with_evaluator(|x, y| evaluate_callback(callback, x, y))
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = finishSnapshotJsonWithAxes)]
        pub fn finish_snapshot_json_with_axes(
            &self,
            callback: &Function,
            axes_request_json: &str,
            x_axis_snapshot_json: &str,
            y_axis_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .finish_snapshot_json_with_evaluator_and_axes(
                    |x, y| evaluate_callback(callback, x, y),
                    axes_request_json,
                    x_axis_snapshot_json,
                    y_axis_snapshot_json,
                )
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmImplicitFunctionPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{GeometryRef, PathCommand};

    fn request(use_smoothing: bool) -> String {
        format!(
            r#"{{"x_range":[-2.0,2.0],"y_range":[-2.0,2.0],"min_depth":3,"max_quads":500,"use_smoothing":{use_smoothing}}}"#
        )
    }

    #[test]
    fn bridge_finishes_direct_implicit_geometry_without_host_owned_contours() {
        let plan = ImplicitFunctionAuthoringPlan::from_json(&request(false)).unwrap();
        let snapshot = plan
            .finish_with_evaluator(|x, y| Ok(x * x + y * y - 1.0))
            .unwrap();
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("ImplicitFunction must lower to ordinary VectorPath geometry")
        };
        assert!(matches!(
            path.commands().first(),
            Some(PathCommand::MoveTo { .. })
        ));
        assert!(path
            .commands()
            .iter()
            .skip(1)
            .any(|command| matches!(command, PathCommand::LineTo { .. })));
    }

    #[test]
    fn axes_finish_maps_logical_contours_through_current_axis_transforms() {
        let plan = ImplicitFunctionAuthoringPlan::from_json(&request(false)).unwrap();
        let axes_request =
            r#"{"x_range":[-2.0,2.0,1.0],"y_range":[-2.0,2.0,1.0],"x_length":8.0,"y_length":4.0}"#;
        let axes = Axes2DState::new(
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            8.0,
            4.0,
        )
        .unwrap();
        let x_snapshot = noon::Line::new(axes.x_axis().start(), axes.x_axis().end())
            .shift(noon_core::Vec2::new(2.0, -1.0))
            .into_snapshot();
        let y_snapshot = noon::Line::new(axes.y_axis().start(), axes.y_axis().end())
            .shift(noon_core::Vec2::new(2.0, -1.0))
            .into_snapshot();
        let snapshot = plan
            .finish_with_evaluator_and_axes(
                |x, y| Ok(x * x + y * y - 1.0),
                axes_request,
                &serde_json::to_string(&x_snapshot).unwrap(),
                &serde_json::to_string(&y_snapshot).unwrap(),
            )
            .unwrap();
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("Axes implicit plot must remain ordinary VectorPath geometry")
        };
        let first = match path.commands().first().unwrap() {
            PathCommand::MoveTo { to } => *to,
            other => panic!("expected MoveTo, got {other:?}"),
        };
        assert!(first.x > -2.1 && first.x < 6.1);
        assert!(first.y > -3.1 && first.y < 1.1);
    }
}
