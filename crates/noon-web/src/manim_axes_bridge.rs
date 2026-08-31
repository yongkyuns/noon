use noon::{
    Axes2DState, AxisTickError, CoordinateSystemError, NumberLineGeometryPlan,
    NumberLineTickOptions, NumberRange,
};
use noon_core::{Color, ObjectSnapshot, WHITE};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AxisGeometryRequest {
    #[serde(default)]
    include_tip: Option<bool>,
    #[serde(default)]
    include_ticks: Option<bool>,
    #[serde(default)]
    tick_size: Option<f64>,
    #[serde(default)]
    numbers_with_elongated_ticks: Option<Vec<f64>>,
    #[serde(default)]
    longer_tick_multiple: Option<usize>,
    #[serde(default)]
    exclude_origin_tick: Option<bool>,
    #[serde(default)]
    stroke_width: Option<f64>,
    #[serde(default)]
    color: Option<[f64; 4]>,
}

impl AxisGeometryRequest {
    fn overlay(&mut self, other: Self) {
        if other.include_tip.is_some() {
            self.include_tip = other.include_tip;
        }
        if other.include_ticks.is_some() {
            self.include_ticks = other.include_ticks;
        }
        if other.tick_size.is_some() {
            self.tick_size = other.tick_size;
        }
        if other.numbers_with_elongated_ticks.is_some() {
            self.numbers_with_elongated_ticks = other.numbers_with_elongated_ticks;
        }
        if other.longer_tick_multiple.is_some() {
            self.longer_tick_multiple = other.longer_tick_multiple;
        }
        if other.exclude_origin_tick.is_some() {
            self.exclude_origin_tick = other.exclude_origin_tick;
        }
        if other.stroke_width.is_some() {
            self.stroke_width = other.stroke_width;
        }
        if other.color.is_some() {
            self.color = other.color;
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AxesAuthoringRequest {
    x_range: [f64; 3],
    y_range: [f64; 3],
    x_length: f32,
    y_length: f32,
    #[serde(default = "default_true")]
    tips: bool,
    #[serde(default)]
    axis_config: AxisGeometryRequest,
    #[serde(default)]
    x_axis_config: AxisGeometryRequest,
    #[serde(default)]
    y_axis_config: AxisGeometryRequest,
}

const fn default_true() -> bool {
    true
}

#[derive(Serialize)]
struct TickWire<'a> {
    value: f64,
    size: f64,
    snapshot: &'a ObjectSnapshot,
}

#[derive(Serialize)]
struct NumberLineGeometryWire<'a> {
    line: &'a ObjectSnapshot,
    ticks: Vec<TickWire<'a>>,
}

#[derive(Serialize)]
struct AxesGeometryWire<'a> {
    x_axis: NumberLineGeometryWire<'a>,
    y_axis: NumberLineGeometryWire<'a>,
}

/// Shared semantic/geometry plan for the initial linear ManimCE v0.21 `Axes` subset.
///
/// This object owns the coordinate mapping and all retained line/tick geometry. Host
/// frontends may serialize the resulting snapshots into their normal semantic-handle
/// path, but they do not calculate axis placement, tick positions, tick orientation,
/// config precedence, or coordinate conversion themselves.
#[derive(Clone, Debug, PartialEq)]
pub struct AxesAuthoringPlan {
    axes: Axes2DState,
    x_geometry: NumberLineGeometryPlan,
    y_geometry: NumberLineGeometryPlan,
}

impl AxesAuthoringPlan {
    pub fn from_json(request_json: &str) -> Result<Self, ManimAxesBridgeError> {
        let request: AxesAuthoringRequest = serde_json::from_str(request_json)
            .map_err(|error| ManimAxesBridgeError::InvalidRequest(error.to_string()))?;
        Self::new(request)
    }

    fn new(request: AxesAuthoringRequest) -> Result<Self, ManimAxesBridgeError> {
        let x_range = NumberRange::new(request.x_range[0], request.x_range[1], request.x_range[2])?;
        let y_range = NumberRange::new(request.y_range[0], request.y_range[1], request.y_range[2])?;
        let axes = Axes2DState::new(x_range, y_range, request.x_length, request.y_length)?;

        let mut shared = AxisGeometryRequest {
            include_tip: Some(request.tips),
            ..AxisGeometryRequest::default()
        };
        shared.overlay(request.axis_config);

        let mut x_request = shared.clone();
        x_request.overlay(request.x_axis_config);
        let mut y_request = shared;
        y_request.overlay(request.y_axis_config);

        let x_options = resolve_axis_options(x_request, Axis::X)?;
        let y_options = resolve_axis_options(y_request, Axis::Y)?;
        let x_geometry = NumberLineGeometryPlan::new(axes.x_axis(), &x_options)?;
        let y_geometry = NumberLineGeometryPlan::new(axes.y_axis(), &y_options)?;

        Ok(Self {
            axes,
            x_geometry,
            y_geometry,
        })
    }

    pub const fn axes(&self) -> Axes2DState {
        self.axes
    }

    pub fn geometry_json(&self) -> Result<String, ManimAxesBridgeError> {
        let wire = AxesGeometryWire {
            x_axis: number_line_wire(&self.x_geometry),
            y_axis: number_line_wire(&self.y_geometry),
        };
        serde_json::to_string(&wire)
            .map_err(|error| ManimAxesBridgeError::Serialization(error.to_string()))
    }

    pub fn coords_to_point_json(&self, x: f64, y: f64) -> Result<String, ManimAxesBridgeError> {
        let point = self.axes.coords_to_point(x, y)?;
        serde_json::to_string(&[f64::from(point.x), f64::from(point.y)])
            .map_err(|error| ManimAxesBridgeError::Serialization(error.to_string()))
    }

    pub fn point_to_coords_json(
        &self,
        x: f32,
        y: f32,
    ) -> Result<String, ManimAxesBridgeError> {
        let (x, y) = self.axes.point_to_coords(noon_core::Vec2::new(x, y))?;
        serde_json::to_string(&[x, y])
            .map_err(|error| ManimAxesBridgeError::Serialization(error.to_string()))
    }
}

fn number_line_wire(plan: &NumberLineGeometryPlan) -> NumberLineGeometryWire<'_> {
    NumberLineGeometryWire {
        line: plan.line(),
        ticks: plan
            .ticks()
            .iter()
            .map(|tick| TickWire {
                value: tick.value(),
                size: tick.size(),
                snapshot: tick.snapshot(),
            })
            .collect(),
    }
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

fn resolve_axis_options(
    request: AxisGeometryRequest,
    axis: Axis,
) -> Result<NumberLineTickOptions, ManimAxesBridgeError> {
    if request.include_tip.unwrap_or(false) {
        return Err(ManimAxesBridgeError::UnsupportedTips(match axis {
            Axis::X => "x",
            Axis::Y => "y",
        }));
    }

    let mut options = NumberLineTickOptions::default();
    if let Some(value) = request.include_ticks {
        options.include_ticks = value;
    }
    if let Some(value) = request.tick_size {
        options.tick_size = value;
    }
    if let Some(values) = request.numbers_with_elongated_ticks {
        options.elongated_values = values;
    }
    if let Some(value) = request.longer_tick_multiple {
        options.longer_tick_multiple = value;
    }
    if let Some(value) = request.stroke_width {
        options.stroke_width = value;
    }
    if let Some(value) = request.color {
        options.color = rgba(value)?;
    }

    // Manim's Axes constructor overwrites this after config merging for linear
    // scaling. User-provided `exclude_origin_tick` therefore does not win here.
    let _ = request.exclude_origin_tick;
    options.exclude_origin_tick = true;
    Ok(options)
}

fn rgba(value: [f64; 4]) -> Result<Color, ManimAxesBridgeError> {
    if value.iter().any(|component| !component.is_finite() || !(0.0..=1.0).contains(component)) {
        return Err(ManimAxesBridgeError::InvalidColor(value));
    }
    Ok(Color::rgba(
        value[0] as f32,
        value[1] as f32,
        value[2] as f32,
        value[3] as f32,
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManimAxesBridgeError {
    InvalidRequest(String),
    UnsupportedTips(&'static str),
    InvalidColor([f64; 4]),
    Coordinates(CoordinateSystemError),
    Geometry(AxisTickError),
    Serialization(String),
}

impl std::fmt::Display for ManimAxesBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => write!(formatter, "invalid Axes request: {error}"),
            Self::UnsupportedTips(axis) => write!(
                formatter,
                "Axes {axis}-axis tips are not yet supported by the retained geometry plan"
            ),
            Self::InvalidColor(value) => write!(
                formatter,
                "Axes color must contain finite RGBA components in [0, 1], got {value:?}"
            ),
            Self::Coordinates(error) => error.fmt(formatter),
            Self::Geometry(error) => error.fmt(formatter),
            Self::Serialization(error) => write!(formatter, "unable to serialize Axes state: {error}"),
        }
    }
}

impl std::error::Error for ManimAxesBridgeError {}

impl From<CoordinateSystemError> for ManimAxesBridgeError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl From<AxisTickError> for ManimAxesBridgeError {
    fn from(value: AxisTickError) -> Self {
        Self::Geometry(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::AxesAuthoringPlan;

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    #[wasm_bindgen]
    pub struct WasmAxesAuthoringPlan(AxesAuthoringPlan);

    #[wasm_bindgen]
    impl WasmAxesAuthoringPlan {
        #[wasm_bindgen(constructor)]
        pub fn new(request_json: &str) -> Result<Self, JsValue> {
            AxesAuthoringPlan::from_json(request_json)
                .map(Self)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = geometryJson)]
        pub fn geometry_json(&self) -> Result<String, JsValue> {
            self.0.geometry_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = coordsToPointJson)]
        pub fn coords_to_point_json(&self, x: f64, y: f64) -> Result<String, JsValue> {
            self.0.coords_to_point_json(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = pointToCoordsJson)]
        pub fn point_to_coords_json(&self, x: f32, y: f32) -> Result<String, JsValue> {
            self.0.point_to_coords_json(x, y).map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmAxesAuthoringPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn request(extra: &str) -> String {
        format!(
            r#"{{"x_range":[-10.0,10.3,1.0],"y_range":[-1.5,1.5,1.0],"x_length":10.0,"y_length":6.0,"tips":false{extra}}}"#
        )
    }

    #[test]
    fn canonical_plot_axes_geometry_is_owned_by_shared_plan() {
        let plan = AxesAuthoringPlan::from_json(&request(
            r#", "axis_config":{"color":[0.0,1.0,0.0,1.0]}, "x_axis_config":{"numbers_with_elongated_ticks":[-10,-8,-6,-4,-2,0,2,4,6,8,10]}"#,
        ))
        .unwrap();
        let geometry: Value = serde_json::from_str(&plan.geometry_json().unwrap()).unwrap();
        let x_ticks = geometry["x_axis"]["ticks"].as_array().unwrap();
        let y_ticks = geometry["y_axis"]["ticks"].as_array().unwrap();
        assert_eq!(x_ticks.len(), 20);
        assert_eq!(y_ticks.len(), 2);
        assert!(x_ticks.iter().all(|tick| tick["value"] != 0.0));
        let elongated = x_ticks
            .iter()
            .find(|tick| tick["value"] == 2.0)
            .unwrap();
        assert_eq!(elongated["size"], 0.2);
        assert_eq!(geometry["x_axis"]["line"]["style"]["stroke_width"], 0.02);
    }

    #[test]
    fn axis_specific_config_overrides_shared_config() {
        let plan = AxesAuthoringPlan::from_json(&request(
            r#", "axis_config":{"tick_size":0.2}, "x_axis_config":{"tick_size":0.3}"#,
        ))
        .unwrap();
        let geometry: Value = serde_json::from_str(&plan.geometry_json().unwrap()).unwrap();
        assert_eq!(geometry["x_axis"]["ticks"][0]["size"], 0.3);
        assert_eq!(geometry["y_axis"]["ticks"][0]["size"], 0.2);
    }

    #[test]
    fn linear_axes_force_origin_tick_exclusion_after_config_merge() {
        let plan = AxesAuthoringPlan::from_json(&request(
            r#", "axis_config":{"exclude_origin_tick":false}"#,
        ))
        .unwrap();
        let geometry: Value = serde_json::from_str(&plan.geometry_json().unwrap()).unwrap();
        for axis in ["x_axis", "y_axis"] {
            assert!(geometry[axis]["ticks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|tick| tick["value"] != 0.0));
        }
    }

    #[test]
    fn coordinate_queries_round_trip_through_same_axes_state() {
        let plan = AxesAuthoringPlan::from_json(&request("")).unwrap();
        let point: [f64; 2] = serde_json::from_str(&plan.coords_to_point_json(2.0, 1.0).unwrap())
            .unwrap();
        let coords: [f64; 2] = serde_json::from_str(
            &plan
                .point_to_coords_json(point[0] as f32, point[1] as f32)
                .unwrap(),
        )
        .unwrap();
        assert!((coords[0] - 2.0).abs() <= 1.0e-5);
        assert!((coords[1] - 1.0).abs() <= 1.0e-5);
    }

    #[test]
    fn tips_are_rejected_instead_of_silently_omitted() {
        let request = r#"{"x_range":[-1,1,1],"y_range":[-1,1,1],"x_length":2,"y_length":2}"#;
        assert!(matches!(
            AxesAuthoringPlan::from_json(request),
            Err(ManimAxesBridgeError::UnsupportedTips("x"))
        ));
    }

    #[test]
    fn unsupported_axis_config_keys_fail_closed() {
        let error = AxesAuthoringPlan::from_json(&request(
            r#", "x_axis_config":{"numbers_to_include":[-2,0,2]}"#,
        ))
        .unwrap_err();
        assert!(matches!(error, ManimAxesBridgeError::InvalidRequest(_)));
    }
}
