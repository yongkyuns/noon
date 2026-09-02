use noon::{IntoSnapshot, Path};
use noon_core::ObjectSnapshot;
use noon_geometry::{isoline_vector_path, IsolineError, IsolineOptions};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
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

const fn default_min_depth() -> usize {
    5
}

const fn default_max_quads() -> usize {
    1_500
}

const fn default_true() -> bool {
    true
}

/// Deterministic retained authoring plan for ManimCE v0.21 `ImplicitFunction`.
///
/// The adaptive quadtree, multiresolution topology, zero refinement, curve
/// tracing, smoothing, and retained path construction stay in Rust. Frontends
/// provide only a synchronous scalar evaluator during authoring.
#[derive(Clone, Debug, PartialEq)]
pub struct ImplicitFunctionAuthoringPlan {
    x_range: [f64; 2],
    y_range: [f64; 2],
    options: IsolineOptions,
    use_smoothing: bool,
}

impl ImplicitFunctionAuthoringPlan {
    pub fn from_json(request_json: &str) -> Result<Self, ManimImplicitBridgeError> {
        let request: ImplicitFunctionRequest = serde_json::from_str(request_json)
            .map_err(|error| ManimImplicitBridgeError::InvalidRequest(error.to_string()))?;
        Ok(Self {
            x_range: request.x_range,
            y_range: request.y_range,
            options: IsolineOptions {
                min_depth: request.min_depth,
                max_quads: request.max_quads,
                tolerance: None,
            },
            use_smoothing: request.use_smoothing,
        })
    }

    pub fn finish_with_field<F>(&self, field: F) -> Result<ObjectSnapshot, ManimImplicitBridgeError>
    where
        F: FnMut(f64, f64) -> f64,
    {
        let path = isoline_vector_path(
            [self.x_range[0], self.y_range[0]],
            [self.x_range[1], self.y_range[1]],
            self.options,
            self.use_smoothing,
            field,
        )?;
        Ok(Path::new(path).into_snapshot())
    }

    pub fn finish_snapshot_json<F>(&self, field: F) -> Result<String, ManimImplicitBridgeError>
    where
        F: FnMut(f64, f64) -> f64,
    {
        serde_json::to_string(&self.finish_with_field(field)?)
            .map_err(|error| ManimImplicitBridgeError::Serialization(error.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManimImplicitBridgeError {
    InvalidRequest(String),
    Geometry(IsolineError),
    Serialization(String),
}

impl std::fmt::Display for ManimImplicitBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => {
                write!(formatter, "invalid ImplicitFunction request: {error}")
            }
            Self::Geometry(error) => error.fmt(formatter),
            Self::Serialization(error) => {
                write!(formatter, "unable to serialize ImplicitFunction state: {error}")
            }
        }
    }
}

impl std::error::Error for ManimImplicitBridgeError {}

impl From<IsolineError> for ManimImplicitBridgeError {
    fn from(value: IsolineError) -> Self {
        Self::Geometry(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::cell::RefCell;

    use js_sys::Function;
    use wasm_bindgen::prelude::*;

    use super::ImplicitFunctionAuthoringPlan;

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    /// Evaluate a Python/Pyodide scalar callback synchronously during authoring
    /// while all adaptive contour traversal and topology remain Rust-owned.
    #[wasm_bindgen(js_name = manimImplicitFunctionSnapshotJson)]
    pub fn manim_implicit_function_snapshot_json(
        request_json: &str,
        callback: &Function,
    ) -> Result<String, JsValue> {
        let plan = ImplicitFunctionAuthoringPlan::from_json(request_json).map_err(js_error)?;
        let callback_error = RefCell::new(None::<JsValue>);
        let result = plan.finish_snapshot_json(|x, y| {
            if callback_error.borrow().is_some() {
                return f64::NAN;
            }
            let value = match callback.call2(
                &JsValue::NULL,
                &JsValue::from_f64(x),
                &JsValue::from_f64(y),
            ) {
                Ok(value) => value,
                Err(error) => {
                    *callback_error.borrow_mut() = Some(error);
                    return f64::NAN;
                }
            };
            match value.as_f64() {
                Some(value) => value,
                None => {
                    *callback_error.borrow_mut() = Some(JsValue::from_str(
                        "ImplicitFunction callback must return a real scalar",
                    ));
                    f64::NAN
                }
            }
        });
        if let Some(error) = callback_error.into_inner() {
            return Err(error);
        }
        result.map_err(js_error)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::manim_implicit_function_snapshot_json;

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{GeometryRef, PathCommand};

    fn request(use_smoothing: bool) -> String {
        format!(
            r#"{{"x_range":[-1.5,1.5],"y_range":[-1.5,1.5],"min_depth":4,"max_quads":512,"use_smoothing":{use_smoothing}}}"#
        )
    }

    #[test]
    fn direct_implicit_function_lowers_to_ordinary_retained_vector_path() {
        let plan = ImplicitFunctionAuthoringPlan::from_json(&request(false)).unwrap();
        let snapshot = plan
            .finish_with_field(|x, y| x * x + y * y - 1.0)
            .unwrap();
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("ImplicitFunction must lower to ordinary VectorPath geometry");
        };
        assert!(path.commands().len() > 8);
        assert!(path.commands().iter().any(|command| matches!(command, PathCommand::LineTo { .. })));
    }

    #[test]
    fn smoothing_uses_shared_manim_cubic_geometry() {
        let plan = ImplicitFunctionAuthoringPlan::from_json(&request(true)).unwrap();
        let snapshot = plan
            .finish_with_field(|x, y| x * x + y * y - 1.0)
            .unwrap();
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("ImplicitFunction must lower to ordinary VectorPath geometry");
        };
        assert!(path.commands().iter().any(|command| matches!(command, PathCommand::CubicTo { .. })));
    }
}
