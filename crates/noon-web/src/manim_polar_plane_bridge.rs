use noon::{
    Axes2DState, CoordinateSystemError, NumberPlaneLineStyle, NumberRange,
    PolarPlaneAuthoringError, PolarPlaneGridPlan, PolarPlaneRadialLine, PolarPlaneRadiusCircle,
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
struct PolarPlaneGridRequest {
    radius_max: f64,
    radius_step: f64,
    size: f32,
    azimuth_step: f64,
    azimuth_offset: f64,
    faded_line_ratio: usize,
    background_style: GridStyleRequest,
    faded_style: GridStyleRequest,
}

#[derive(Serialize)]
struct RadialLineWire<'a> {
    angle: f64,
    snapshot: &'a ObjectSnapshot,
}

#[derive(Serialize)]
struct RadiusCircleWire<'a> {
    radius: f64,
    snapshot: &'a ObjectSnapshot,
}

#[derive(Serialize)]
struct PolarPlaneGridWire<'a> {
    radial_lines: Vec<RadialLineWire<'a>>,
    circles: Vec<RadiusCircleWire<'a>>,
    faded_radial_lines: Vec<RadialLineWire<'a>>,
    faded_circles: Vec<RadiusCircleWire<'a>>,
}

/// Thin serialization adapter over the shared retained PolarPlane grid planner.
#[derive(Clone, Debug, PartialEq)]
pub struct PolarPlaneGridAuthoringPlan {
    grid: PolarPlaneGridPlan,
}

impl PolarPlaneGridAuthoringPlan {
    pub fn from_json(request_json: &str) -> Result<Self, ManimPolarPlaneBridgeError> {
        let request: PolarPlaneGridRequest = serde_json::from_str(request_json)
            .map_err(|error| ManimPolarPlaneBridgeError::InvalidRequest(error.to_string()))?;
        let range = NumberRange::new(
            -request.radius_max,
            request.radius_max,
            request.radius_step,
        )?;
        let axes = Axes2DState::new(range, range, request.size, request.size)?;
        let grid = PolarPlaneGridPlan::new(
            axes,
            request.azimuth_step,
            request.azimuth_offset,
            request.faded_line_ratio,
            line_style(request.background_style)?,
            line_style(request.faded_style)?,
        )?;
        Ok(Self { grid })
    }

    pub fn geometry_json(&self) -> Result<String, ManimPolarPlaneBridgeError> {
        let wire = PolarPlaneGridWire {
            radial_lines: radial_lines_wire(self.grid.radial_lines()),
            circles: radius_circles_wire(self.grid.circles()),
            faded_radial_lines: radial_lines_wire(self.grid.faded_radial_lines()),
            faded_circles: radius_circles_wire(self.grid.faded_circles()),
        };
        serde_json::to_string(&wire)
            .map_err(|error| ManimPolarPlaneBridgeError::Serialization(error.to_string()))
    }
}

fn radial_lines_wire(lines: &[PolarPlaneRadialLine]) -> Vec<RadialLineWire<'_>> {
    lines
        .iter()
        .map(|line| RadialLineWire {
            angle: line.angle(),
            snapshot: line.snapshot(),
        })
        .collect()
}

fn radius_circles_wire(circles: &[PolarPlaneRadiusCircle]) -> Vec<RadiusCircleWire<'_>> {
    circles
        .iter()
        .map(|circle| RadiusCircleWire {
            radius: circle.radius(),
            snapshot: circle.snapshot(),
        })
        .collect()
}

fn line_style(
    request: GridStyleRequest,
) -> Result<NumberPlaneLineStyle, ManimPolarPlaneBridgeError> {
    Ok(NumberPlaneLineStyle::new(
        rgba(request.color)?,
        request.stroke_width,
        request.stroke_opacity,
    ))
}

fn rgba(value: [f64; 4]) -> Result<Color, ManimPolarPlaneBridgeError> {
    if value
        .iter()
        .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
    {
        return Err(ManimPolarPlaneBridgeError::InvalidColor(value));
    }
    Ok(Color::rgba(
        value[0] as f32,
        value[1] as f32,
        value[2] as f32,
        value[3] as f32,
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManimPolarPlaneBridgeError {
    InvalidRequest(String),
    InvalidColor([f64; 4]),
    Coordinates(CoordinateSystemError),
    Geometry(PolarPlaneAuthoringError),
    Serialization(String),
}

impl std::fmt::Display for ManimPolarPlaneBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => {
                write!(formatter, "invalid PolarPlane request: {error}")
            }
            Self::InvalidColor(value) => write!(
                formatter,
                "PolarPlane color must contain finite RGBA components in [0, 1], got {value:?}"
            ),
            Self::Coordinates(error) => error.fmt(formatter),
            Self::Geometry(error) => error.fmt(formatter),
            Self::Serialization(error) => {
                write!(formatter, "unable to serialize PolarPlane grid: {error}")
            }
        }
    }
}

impl std::error::Error for ManimPolarPlaneBridgeError {}

impl From<CoordinateSystemError> for ManimPolarPlaneBridgeError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl From<PolarPlaneAuthoringError> for ManimPolarPlaneBridgeError {
    fn from(value: PolarPlaneAuthoringError) -> Self {
        Self::Geometry(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::PolarPlaneGridAuthoringPlan;

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    #[wasm_bindgen]
    pub struct WasmPolarPlaneGridPlan(PolarPlaneGridAuthoringPlan);

    #[wasm_bindgen]
    impl WasmPolarPlaneGridPlan {
        #[wasm_bindgen(constructor)]
        pub fn new(request_json: &str) -> Result<Self, JsValue> {
            PolarPlaneGridAuthoringPlan::from_json(request_json)
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
pub use wasm::WasmPolarPlaneGridPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn request(ratio: usize) -> String {
        format!(
            r#"{{"radius_max":2.0,"radius_step":1.0,"size":4.0,"azimuth_step":4.0,"azimuth_offset":0.0,"faded_line_ratio":{ratio},"background_style":{{"color":[0.1,0.2,0.3,1.0],"stroke_width":2.0,"stroke_opacity":1.0}},"faded_style":{{"color":[0.1,0.2,0.3,1.0],"stroke_width":1.0,"stroke_opacity":0.5}}}}"#
        )
    }

    #[test]
    fn bridge_serializes_shared_polar_families_without_recomputing_geometry() {
        let plan = PolarPlaneGridAuthoringPlan::from_json(&request(2)).unwrap();
        let wire: Value = serde_json::from_str(&plan.geometry_json().unwrap()).unwrap();
        assert_eq!(wire["radial_lines"].as_array().unwrap().len(), 4);
        assert_eq!(wire["circles"].as_array().unwrap().len(), 3);
        assert_eq!(wire["faded_radial_lines"].as_array().unwrap().len(), 4);
        assert_eq!(wire["faded_circles"].as_array().unwrap().len(), 2);
        assert_eq!(wire["circles"][1]["radius"], 1.0);
        assert_eq!(
            wire["faded_radial_lines"][0]["snapshot"]["style"]["stroke_width"],
            0.01
        );
    }

    #[test]
    fn bridge_rejects_non_rgba_style_inputs() {
        let invalid = request(1).replace("[0.1,0.2,0.3,1.0]", "[2.0,0.2,0.3,1.0]");
        assert!(matches!(
            PolarPlaneGridAuthoringPlan::from_json(&invalid),
            Err(ManimPolarPlaneBridgeError::InvalidColor(_))
        ));
    }
}
