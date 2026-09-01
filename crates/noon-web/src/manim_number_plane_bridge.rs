use noon::{
    Axes2DState, CoordinateSystemError, NumberPlaneAuthoringError, NumberPlaneGridLine,
    NumberPlaneGridPlan, NumberPlaneLineStyle, NumberRange,
};
use noon_core::{Color, ObjectSnapshot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct GridStyleRequest {
    color: [f64; 4],
    stroke_width: f64,
    stroke_opacity: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct NumberPlaneGridRequest {
    x_range: [f64; 3],
    y_range: [f64; 3],
    x_length: f32,
    y_length: f32,
    faded_line_ratio: usize,
    background_style: GridStyleRequest,
    faded_style: GridStyleRequest,
}

#[derive(Serialize)]
struct GridLineWire<'a> {
    offset: f64,
    snapshot: &'a ObjectSnapshot,
}

#[derive(Serialize)]
struct NumberPlaneGridWire<'a> {
    x_lines: Vec<GridLineWire<'a>>,
    y_lines: Vec<GridLineWire<'a>>,
    faded_x_lines: Vec<GridLineWire<'a>>,
    faded_y_lines: Vec<GridLineWire<'a>>,
}

/// Thin serialization adapter over the shared NumberPlane retained grid planner.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberPlaneGridAuthoringPlan {
    grid: NumberPlaneGridPlan,
}

impl NumberPlaneGridAuthoringPlan {
    pub fn from_json(request_json: &str) -> Result<Self, ManimNumberPlaneBridgeError> {
        let request: NumberPlaneGridRequest = serde_json::from_str(request_json)
            .map_err(|error| ManimNumberPlaneBridgeError::InvalidRequest(error.to_string()))?;
        let x_range = NumberRange::new(request.x_range[0], request.x_range[1], request.x_range[2])?;
        let y_range = NumberRange::new(request.y_range[0], request.y_range[1], request.y_range[2])?;
        let axes = Axes2DState::new(x_range, y_range, request.x_length, request.y_length)?;
        let background_style = line_style(request.background_style)?;
        let faded_style = line_style(request.faded_style)?;
        let grid = NumberPlaneGridPlan::new(
            axes,
            request.faded_line_ratio,
            background_style,
            faded_style,
        )?;
        Ok(Self { grid })
    }

    pub fn geometry_json(&self) -> Result<String, ManimNumberPlaneBridgeError> {
        let wire = NumberPlaneGridWire {
            x_lines: grid_lines_wire(self.grid.x_lines()),
            y_lines: grid_lines_wire(self.grid.y_lines()),
            faded_x_lines: grid_lines_wire(self.grid.faded_x_lines()),
            faded_y_lines: grid_lines_wire(self.grid.faded_y_lines()),
        };
        serde_json::to_string(&wire)
            .map_err(|error| ManimNumberPlaneBridgeError::Serialization(error.to_string()))
    }
}

fn grid_lines_wire(lines: &[NumberPlaneGridLine]) -> Vec<GridLineWire<'_>> {
    lines
        .iter()
        .map(|line| GridLineWire {
            offset: line.offset(),
            snapshot: line.snapshot(),
        })
        .collect()
}

fn line_style(request: GridStyleRequest) -> Result<NumberPlaneLineStyle, ManimNumberPlaneBridgeError> {
    Ok(NumberPlaneLineStyle::new(
        rgba(request.color)?,
        request.stroke_width,
        request.stroke_opacity,
    ))
}

fn rgba(value: [f64; 4]) -> Result<Color, ManimNumberPlaneBridgeError> {
    if value
        .iter()
        .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
    {
        return Err(ManimNumberPlaneBridgeError::InvalidColor(value));
    }
    Ok(Color::rgba(
        value[0] as f32,
        value[1] as f32,
        value[2] as f32,
        value[3] as f32,
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManimNumberPlaneBridgeError {
    InvalidRequest(String),
    InvalidColor([f64; 4]),
    Coordinates(CoordinateSystemError),
    Geometry(NumberPlaneAuthoringError),
    Serialization(String),
}

impl std::fmt::Display for ManimNumberPlaneBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => write!(formatter, "invalid NumberPlane request: {error}"),
            Self::InvalidColor(value) => write!(
                formatter,
                "NumberPlane color must contain finite RGBA components in [0, 1], got {value:?}"
            ),
            Self::Coordinates(error) => error.fmt(formatter),
            Self::Geometry(error) => error.fmt(formatter),
            Self::Serialization(error) => {
                write!(formatter, "unable to serialize NumberPlane grid: {error}")
            }
        }
    }
}

impl std::error::Error for ManimNumberPlaneBridgeError {}

impl From<CoordinateSystemError> for ManimNumberPlaneBridgeError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl From<NumberPlaneAuthoringError> for ManimNumberPlaneBridgeError {
    fn from(value: NumberPlaneAuthoringError) -> Self {
        Self::Geometry(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::NumberPlaneGridAuthoringPlan;

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    #[wasm_bindgen]
    pub struct WasmNumberPlaneGridPlan(NumberPlaneGridAuthoringPlan);

    #[wasm_bindgen]
    impl WasmNumberPlaneGridPlan {
        #[wasm_bindgen(constructor)]
        pub fn new(request_json: &str) -> Result<Self, JsValue> {
            NumberPlaneGridAuthoringPlan::from_json(request_json)
                .map(Self)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = geometryJson)]
        pub fn geometry_json(&self) -> Result<String, JsValue> {
            self.0.geometry_json().map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmNumberPlaneGridPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn request(ratio: usize) -> String {
        format!(
            r#"{{"x_range":[-2.0,2.0,1.0],"y_range":[-2.0,2.0,1.0],"x_length":8.0,"y_length":12.0,"faded_line_ratio":{ratio},"background_style":{{"color":[0.1,0.2,0.3,1.0],"stroke_width":2.0,"stroke_opacity":1.0}},"faded_style":{{"color":[0.1,0.2,0.3,1.0],"stroke_width":1.0,"stroke_opacity":0.5}}}}"#
        )
    }

    #[test]
    fn bridge_serializes_shared_grid_families_without_recomputing_geometry() {
        let plan = NumberPlaneGridAuthoringPlan::from_json(&request(2)).unwrap();
        let wire: Value = serde_json::from_str(&plan.geometry_json().unwrap()).unwrap();
        assert_eq!(wire["x_lines"].as_array().unwrap().len(), 3);
        assert_eq!(wire["y_lines"].as_array().unwrap().len(), 3);
        assert_eq!(wire["faded_x_lines"].as_array().unwrap().len(), 4);
        assert_eq!(wire["faded_y_lines"].as_array().unwrap().len(), 4);
        assert_eq!(wire["x_lines"][1]["offset"], 1.0);
        assert_eq!(
            wire["faded_y_lines"][0]["snapshot"]["style"]["stroke_width"],
            0.01
        );
    }

    #[test]
    fn bridge_rejects_non_rgba_style_inputs() {
        let invalid = request(1).replace("[0.1,0.2,0.3,1.0]", "[2.0,0.2,0.3,1.0]");
        assert!(matches!(
            NumberPlaneGridAuthoringPlan::from_json(&invalid),
            Err(ManimNumberPlaneBridgeError::InvalidColor(_))
        ));
    }
}
