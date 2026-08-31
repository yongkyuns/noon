use noon::{
    Axes2DState, AxisTickError, CoordinateSystemError, NumberLineGeometryPlan,
    NumberLineTickOptions, NumberRange,
};
use noon_core::{Color, ObjectSnapshot, Vec2};
use serde::Deserialize;

use crate::{AxesPlotAuthoringPlan, ManimPlotBridgeError};

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AxesAuthoringRequest {
    x_range: [f64; 3],
    y_range: [f64; 3],
    x_length: f32,
    y_length: f32,
    #[serde(default = "default_true")]
    tips: bool,
    #[serde(default)]
    axis_config: AxisVisualOverride,
    #[serde(default)]
    x_axis_config: AxisVisualOverride,
    #[serde(default)]
    y_axis_config: AxisVisualOverride,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
struct AxisVisualOverride {
    include_ticks: Option<bool>,
    tick_size: Option<f64>,
    numbers_with_elongated_ticks: Option<Vec<f64>>,
    longer_tick_multiple: Option<usize>,
    stroke_width: Option<f64>,
    color: Option<[f64; 4]>,
    include_tip: Option<bool>,
    include_numbers: Option<bool>,
    numbers_to_include: Option<Vec<f64>>,
}

const fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq)]
struct ResolvedAxisVisuals {
    ticks: NumberLineTickOptions,
    include_tip: bool,
    include_numbers: bool,
    numbers_to_include: Option<Vec<f64>>,
}

impl ResolvedAxisVisuals {
    fn new(tips: bool) -> Self {
        Self {
            ticks: NumberLineTickOptions {
                exclude_origin_tick: true,
                ..NumberLineTickOptions::default()
            },
            include_tip: tips,
            include_numbers: false,
            numbers_to_include: None,
        }
    }

    fn apply(&mut self, overrides: &AxisVisualOverride) -> Result<(), ManimAxesBridgeError> {
        if let Some(value) = overrides.include_ticks {
            self.ticks.include_ticks = value;
        }
        if let Some(value) = overrides.tick_size {
            self.ticks.tick_size = value;
        }
        if let Some(values) = &overrides.numbers_with_elongated_ticks {
            self.ticks.elongated_values = values.clone();
        }
        if let Some(value) = overrides.longer_tick_multiple {
            self.ticks.longer_tick_multiple = value;
        }
        if let Some(value) = overrides.stroke_width {
            self.ticks.stroke_width = value;
        }
        if let Some(value) = overrides.color {
            self.ticks.color = decode_color(value)?;
        }
        if let Some(value) = overrides.include_tip {
            self.include_tip = value;
        }
        if let Some(value) = overrides.include_numbers {
            self.include_numbers = value;
        }
        if let Some(values) = &overrides.numbers_to_include {
            self.numbers_to_include = Some(values.clone());
        }
        Ok(())
    }

    fn validate_supported(&self) -> Result<(), ManimAxesBridgeError> {
        if self.include_tip {
            return Err(ManimAxesBridgeError::UnsupportedTips);
        }
        if self.include_numbers || self.numbers_to_include.is_some() {
            return Err(ManimAxesBridgeError::UnsupportedNumbers);
        }
        Ok(())
    }
}

/// Shared browser plan for the initial linear ManimCE v0.21 `Axes` subset.
///
/// One retained `Axes2DState` owns coordinate conversion, axis placement, tick
/// geometry, and all plots created from this plan. Frontends only coerce host
/// values and compose the returned ordinary retained line/path snapshots.
#[derive(Clone, Debug, PartialEq)]
pub struct AxesAuthoringPlan {
    axes: Axes2DState,
    x_range: NumberRange,
    children: Vec<ObjectSnapshot>,
    x_child_count: usize,
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

        let mut x_visuals = ResolvedAxisVisuals::new(request.tips);
        x_visuals.apply(&request.axis_config)?;
        x_visuals.apply(&request.x_axis_config)?;
        x_visuals.validate_supported()?;

        let mut y_visuals = ResolvedAxisVisuals::new(request.tips);
        y_visuals.apply(&request.axis_config)?;
        y_visuals.apply(&request.y_axis_config)?;
        y_visuals.validate_supported()?;

        let x_geometry = NumberLineGeometryPlan::new(axes.x_axis(), &x_visuals.ticks)?;
        let y_geometry = NumberLineGeometryPlan::new(axes.y_axis(), &y_visuals.ticks)?;
        let x_child_count = 1 + x_geometry.ticks().len();
        let children = flatten_axis_children(&x_geometry, &y_geometry);

        Ok(Self {
            axes,
            x_range,
            children,
            x_child_count,
        })
    }

    pub fn children(&self) -> &[ObjectSnapshot] {
        &self.children
    }

    pub const fn x_child_count(&self) -> usize {
        self.x_child_count
    }

    pub fn children_json(&self) -> Result<String, ManimAxesBridgeError> {
        serde_json::to_string(&self.children)
            .map_err(|error| ManimAxesBridgeError::Serialization(error.to_string()))
    }

    pub fn coords_to_point(&self, x: f64, y: f64) -> Result<Vec2, ManimAxesBridgeError> {
        Ok(self.axes.coords_to_point(x, y)?)
    }

    pub fn point_to_coords(&self, point: Vec2) -> Result<(f64, f64), ManimAxesBridgeError> {
        Ok(self.axes.point_to_coords(point)?)
    }

    pub fn coords_to_point_json(&self, x: f64, y: f64) -> Result<String, ManimAxesBridgeError> {
        let point = self.coords_to_point(x, y)?;
        serde_json::to_string(&[f64::from(point.x), f64::from(point.y)])
            .map_err(|error| ManimAxesBridgeError::Serialization(error.to_string()))
    }

    pub fn point_to_coords_json(&self, x: f64, y: f64) -> Result<String, ManimAxesBridgeError> {
        let point = Vec2::new(checked_f32("point.x", x)?, checked_f32("point.y", y)?);
        let (x, y) = self.point_to_coords(point)?;
        serde_json::to_string(&[x, y])
            .map_err(|error| ManimAxesBridgeError::Serialization(error.to_string()))
    }

    pub fn plot_parameters_json(&self, request_json: &str) -> Result<String, ManimAxesBridgeError> {
        Ok(self.plot_plan(request_json)?.parameters_json()?)
    }

    pub fn finish_plot_snapshot_json(
        &self,
        request_json: &str,
        values_json: &str,
    ) -> Result<String, ManimAxesBridgeError> {
        Ok(self
            .plot_plan(request_json)?
            .finish_snapshot_json(values_json)?)
    }

    fn plot_plan(&self, request_json: &str) -> Result<AxesPlotAuthoringPlan, ManimAxesBridgeError> {
        Ok(AxesPlotAuthoringPlan::from_axes_json(
            self.axes,
            self.x_range,
            request_json,
        )?)
    }
}

fn flatten_axis_children(
    x_geometry: &NumberLineGeometryPlan,
    y_geometry: &NumberLineGeometryPlan,
) -> Vec<ObjectSnapshot> {
    let mut children = Vec::with_capacity(2 + x_geometry.ticks().len() + y_geometry.ticks().len());
    children.push(x_geometry.line().clone());
    children.extend(
        x_geometry
            .ticks()
            .iter()
            .map(|tick| tick.snapshot().clone()),
    );
    children.push(y_geometry.line().clone());
    children.extend(
        y_geometry
            .ticks()
            .iter()
            .map(|tick| tick.snapshot().clone()),
    );
    children
}

fn decode_color(value: [f64; 4]) -> Result<Color, ManimAxesBridgeError> {
    let [red, green, blue, alpha] = value;
    for (name, channel) in [
        ("red", red),
        ("green", green),
        ("blue", blue),
        ("alpha", alpha),
    ] {
        if !channel.is_finite() || !(0.0..=1.0).contains(&channel) {
            return Err(ManimAxesBridgeError::InvalidColorChannel {
                name,
                value: channel,
            });
        }
    }
    Ok(Color::rgba(
        red as f32,
        green as f32,
        blue as f32,
        alpha as f32,
    ))
}

fn checked_f32(name: &'static str, value: f64) -> Result<f32, ManimAxesBridgeError> {
    let lowered = value as f32;
    if value.is_finite() && lowered.is_finite() {
        Ok(lowered)
    } else {
        Err(ManimAxesBridgeError::InvalidPoint { name, value })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManimAxesBridgeError {
    InvalidRequest(String),
    Coordinates(CoordinateSystemError),
    Ticks(AxisTickError),
    Plot(ManimPlotBridgeError),
    UnsupportedTips,
    UnsupportedNumbers,
    InvalidColorChannel { name: &'static str, value: f64 },
    InvalidPoint { name: &'static str, value: f64 },
    Serialization(String),
}

impl std::fmt::Display for ManimAxesBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => write!(formatter, "invalid Axes request: {error}"),
            Self::Coordinates(error) => error.fmt(formatter),
            Self::Ticks(error) => error.fmt(formatter),
            Self::Plot(error) => error.fmt(formatter),
            Self::UnsupportedTips => formatter.write_str(
                "Axes tips are not implemented in the retained 2D Axes subset; pass tips=False",
            ),
            Self::UnsupportedNumbers => formatter.write_str(
                "Axes numeric labels are not implemented in the retained 2D Axes subset",
            ),
            Self::InvalidColorChannel { name, value } => {
                write!(
                    formatter,
                    "Axes color {name} must be in [0, 1], got {value}"
                )
            }
            Self::InvalidPoint { name, value } => {
                write!(
                    formatter,
                    "Axes {name} must be a finite f32-compatible number: {value}"
                )
            }
            Self::Serialization(error) => {
                write!(
                    formatter,
                    "unable to serialize Axes authoring state: {error}"
                )
            }
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
        Self::Ticks(value)
    }
}

impl From<ManimPlotBridgeError> for ManimAxesBridgeError {
    fn from(value: ManimPlotBridgeError) -> Self {
        Self::Plot(value)
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

        #[wasm_bindgen(getter, js_name = xChildCount)]
        pub fn x_child_count(&self) -> usize {
            self.0.x_child_count()
        }

        #[wasm_bindgen(js_name = childrenJson)]
        pub fn children_json(&self) -> Result<String, JsValue> {
            self.0.children_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = coordsToPointJson)]
        pub fn coords_to_point_json(&self, x: f64, y: f64) -> Result<String, JsValue> {
            self.0.coords_to_point_json(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = pointToCoordsJson)]
        pub fn point_to_coords_json(&self, x: f64, y: f64) -> Result<String, JsValue> {
            self.0.point_to_coords_json(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = plotParametersJson)]
        pub fn plot_parameters_json(&self, request_json: &str) -> Result<String, JsValue> {
            self.0.plot_parameters_json(request_json).map_err(js_error)
        }

        #[wasm_bindgen(js_name = finishPlotSnapshotJson)]
        pub fn finish_plot_snapshot_json(
            &self,
            request_json: &str,
            values_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .finish_plot_snapshot_json(request_json, values_json)
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmAxesAuthoringPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{GeometryRef, PathCommand};

    fn request_json(extra: &str) -> String {
        format!(
            r#"{{"x_range":[-10.0,10.3,1.0],"y_range":[-1.5,1.5,1.0],"x_length":10.0,"y_length":6.0,"tips":false{extra}}}"#
        )
    }

    #[test]
    fn canonical_label_free_axes_build_real_retained_line_children() {
        let request = request_json(
            r#", "axis_config":{"color":[0.1,0.2,0.3,1.0]}, "x_axis_config":{"numbers_with_elongated_ticks":[-10,-8,-6,-4,-2,0,2,4,6,8,10]}"#,
        );
        let plan = AxesAuthoringPlan::from_json(&request).unwrap();
        assert_eq!(plan.children().len(), 24);
        assert_eq!(plan.x_child_count(), 21);
        assert!(plan
            .children()
            .iter()
            .all(|snapshot| matches!(&snapshot.geometry, GeometryRef::Line { .. })));
        let red = plan.children()[0].style.stroke.unwrap().red;
        assert!((red - 0.1_f32).abs() <= f32::EPSILON);
    }

    #[test]
    fn coordinate_round_trip_uses_the_same_stored_axes_state() {
        let plan = AxesAuthoringPlan::from_json(&request_json("")).unwrap();
        let point = plan.coords_to_point(3.25, -0.75).unwrap();
        let (x, y) = plan.point_to_coords(point).unwrap();
        assert!((x - 3.25).abs() <= 1.0e-5);
        assert!((y + 0.75).abs() <= 1.0e-5);
    }

    #[test]
    fn plot_plan_reuses_axes_state_and_lowers_to_vector_path() {
        let plan = AxesAuthoringPlan::from_json(&request_json("")).unwrap();
        let plot_request = r#"{"plot_range":[-1.0,1.0,1.0],"use_smoothing":false}"#;
        assert_eq!(
            plan.plot_parameters_json(plot_request).unwrap(),
            "[[-1.0,0.0,1.0]]"
        );
        let snapshot: ObjectSnapshot = serde_json::from_str(
            &plan
                .finish_plot_snapshot_json(plot_request, "[[0.0,1.0,0.0]]")
                .unwrap(),
        )
        .unwrap();
        let GeometryRef::VectorPath(path) = snapshot.geometry else {
            panic!("plot must remain ordinary retained VectorPath geometry");
        };
        assert!(matches!(path.commands()[0], PathCommand::MoveTo { .. }));
        assert!(matches!(path.commands()[1], PathCommand::LineTo { .. }));
        assert!(matches!(path.commands()[2], PathCommand::LineTo { .. }));
    }

    #[test]
    fn unsupported_visible_features_fail_instead_of_rendering_placeholders() {
        assert!(matches!(
            AxesAuthoringPlan::from_json(
                r#"{"x_range":[-1,1,1],"y_range":[-1,1,1],"x_length":2,"y_length":2}"#,
            )
            .unwrap_err(),
            ManimAxesBridgeError::UnsupportedTips
        ));
        assert!(matches!(
            AxesAuthoringPlan::from_json(&request_json(
                r#", "x_axis_config":{"numbers_to_include":[-1,0,1]}"#,
            ))
            .unwrap_err(),
            ManimAxesBridgeError::UnsupportedNumbers
        ));
    }
}
