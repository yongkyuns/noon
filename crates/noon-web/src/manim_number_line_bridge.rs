use noon::{
    AxisTickError, CoordinateSystemError, NumberLineGeometryPlan, NumberLineState,
    NumberLineTickOptions, NumberRange, TransformedNumberLineState,
};
use noon_core::{Color, GeometryRef, ObjectSnapshot, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct NumberLineRequest {
    x_range: [f64; 3],
    length: f32,
    rotation: f32,
    include_ticks: bool,
    tick_size: f64,
    numbers_with_elongated_ticks: Vec<f64>,
    longer_tick_multiple: usize,
    exclude_origin_tick: bool,
    stroke_width: f64,
    color: [f64; 4],
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

/// Thin browser adapter over the shared retained `NumberLineState` and tick planner.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberLineAuthoringPlan {
    range: NumberRange,
    geometry: NumberLineGeometryPlan,
}

impl NumberLineAuthoringPlan {
    pub fn from_json(request_json: &str) -> Result<Self, ManimNumberLineBridgeError> {
        let request: NumberLineRequest = serde_json::from_str(request_json)
            .map_err(|error| ManimNumberLineBridgeError::InvalidRequest(error.to_string()))?;
        let range = NumberRange::new(request.x_range[0], request.x_range[1], request.x_range[2])?;
        let state = NumberLineState::centered(range, request.length, request.rotation)?;
        let options = NumberLineTickOptions {
            include_ticks: request.include_ticks,
            tick_size: request.tick_size,
            elongated_values: request.numbers_with_elongated_ticks,
            longer_tick_multiple: request.longer_tick_multiple,
            exclude_origin_tick: request.exclude_origin_tick,
            color: rgba(request.color)?,
            stroke_width: request.stroke_width,
        };
        Ok(Self {
            range,
            geometry: NumberLineGeometryPlan::new(state, &options)?,
        })
    }

    pub fn geometry_json(&self) -> Result<String, ManimNumberLineBridgeError> {
        let wire = NumberLineGeometryWire {
            line: self.geometry.line(),
            ticks: self
                .geometry
                .ticks()
                .iter()
                .map(|tick| TickWire {
                    value: tick.value(),
                    size: tick.size(),
                    snapshot: tick.snapshot(),
                })
                .collect(),
        };
        serde_json::to_string(&wire)
            .map_err(|error| ManimNumberLineBridgeError::Serialization(error.to_string()))
    }

    pub fn number_to_point_json(
        &self,
        number: f64,
        line_snapshot_json: &str,
    ) -> Result<String, ManimNumberLineBridgeError> {
        let transformed = self.transformed_line(line_snapshot_json)?;
        serialize_point(transformed.number_to_point(number)?)
    }

    pub fn point_to_number(
        &self,
        x: f32,
        y: f32,
        line_snapshot_json: &str,
    ) -> Result<f64, ManimNumberLineBridgeError> {
        self.transformed_line(line_snapshot_json)?
            .point_to_number(Vec2::new(x, y))
            .map_err(Into::into)
    }

    fn transformed_line(
        &self,
        line_snapshot_json: &str,
    ) -> Result<TransformedNumberLineState, ManimNumberLineBridgeError> {
        let snapshot: ObjectSnapshot = serde_json::from_str(line_snapshot_json)
            .map_err(|error| ManimNumberLineBridgeError::InvalidSnapshot(error.to_string()))?;
        let GeometryRef::Line { start, end } = snapshot.geometry else {
            return Err(ManimNumberLineBridgeError::InvalidGeometry);
        };
        let delta = end - start;
        let length = delta.length();
        if !length.is_finite() || length <= 0.0 {
            return Err(CoordinateSystemError::DegenerateLine.into());
        }
        let rotation = delta.y.atan2(delta.x);
        let center = (start + end) * 0.5;
        let line = NumberLineState::centered(self.range, length, rotation)?.translated(center)?;
        Ok(TransformedNumberLineState::new(line, snapshot.transform))
    }
}

fn serialize_point(point: Vec2) -> Result<String, ManimNumberLineBridgeError> {
    serde_json::to_string(&[f64::from(point.x), f64::from(point.y)])
        .map_err(|error| ManimNumberLineBridgeError::Serialization(error.to_string()))
}

fn rgba(value: [f64; 4]) -> Result<Color, ManimNumberLineBridgeError> {
    if value
        .iter()
        .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
    {
        return Err(ManimNumberLineBridgeError::InvalidColor(value));
    }
    Ok(Color::rgba(
        value[0] as f32,
        value[1] as f32,
        value[2] as f32,
        value[3] as f32,
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManimNumberLineBridgeError {
    InvalidRequest(String),
    InvalidSnapshot(String),
    InvalidGeometry,
    InvalidColor([f64; 4]),
    Coordinates(CoordinateSystemError),
    Geometry(AxisTickError),
    Serialization(String),
}

impl std::fmt::Display for ManimNumberLineBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => write!(formatter, "invalid NumberLine request: {error}"),
            Self::InvalidSnapshot(error) => {
                write!(formatter, "invalid NumberLine snapshot: {error}")
            }
            Self::InvalidGeometry => {
                formatter.write_str("NumberLine query snapshot must contain line geometry")
            }
            Self::InvalidColor(value) => write!(
                formatter,
                "NumberLine color must contain finite RGBA components in [0, 1], got {value:?}"
            ),
            Self::Coordinates(error) => error.fmt(formatter),
            Self::Geometry(error) => error.fmt(formatter),
            Self::Serialization(error) => {
                write!(formatter, "unable to serialize NumberLine result: {error}")
            }
        }
    }
}

impl std::error::Error for ManimNumberLineBridgeError {}

impl From<CoordinateSystemError> for ManimNumberLineBridgeError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl From<AxisTickError> for ManimNumberLineBridgeError {
    fn from(value: AxisTickError) -> Self {
        Self::Geometry(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::NumberLineAuthoringPlan;

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    #[wasm_bindgen]
    pub struct WasmNumberLineAuthoringPlan(NumberLineAuthoringPlan);

    #[wasm_bindgen]
    impl WasmNumberLineAuthoringPlan {
        #[wasm_bindgen(constructor)]
        pub fn new(request_json: &str) -> Result<Self, JsValue> {
            NumberLineAuthoringPlan::from_json(request_json)
                .map(Self)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = geometryJson)]
        pub fn geometry_json(&self) -> Result<String, JsValue> {
            self.0.geometry_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = numberToPointJson)]
        pub fn number_to_point_json(
            &self,
            number: f64,
            line_snapshot_json: &str,
        ) -> Result<String, JsValue> {
            self.0
                .number_to_point_json(number, line_snapshot_json)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = pointToNumber)]
        pub fn point_to_number(
            &self,
            x: f32,
            y: f32,
            line_snapshot_json: &str,
        ) -> Result<f64, JsValue> {
            self.0
                .point_to_number(x, y, line_snapshot_json)
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmNumberLineAuthoringPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use noon::IntoSnapshot;
    use noon_core::{Transform2D, Vec2};
    use serde_json::Value;

    fn request() -> &'static str {
        r#"{"x_range":[-2.0,2.0,1.0],"length":8.0,"rotation":0.25,"include_ticks":true,"tick_size":0.1,"numbers_with_elongated_ticks":[-1.0,1.0],"longer_tick_multiple":2,"exclude_origin_tick":false,"stroke_width":2.0,"color":[1.0,1.0,1.0,1.0]}"#
    }

    #[test]
    fn bridge_emits_main_line_and_shared_tick_geometry() {
        let plan = NumberLineAuthoringPlan::from_json(request()).unwrap();
        let wire: Value = serde_json::from_str(&plan.geometry_json().unwrap()).unwrap();
        assert_eq!(wire["ticks"].as_array().unwrap().len(), 5);
        assert_eq!(wire["ticks"][1]["value"], -1.0);
        assert_eq!(wire["ticks"][1]["size"], 0.2);
        assert_eq!(wire["line"]["style"]["stroke_width"], 0.02);
    }

    #[test]
    fn scalar_queries_follow_current_snapshot_transform() {
        let plan = NumberLineAuthoringPlan::from_json(request()).unwrap();
        let wire: Value = serde_json::from_str(&plan.geometry_json().unwrap()).unwrap();
        let mut line: ObjectSnapshot = serde_json::from_value(wire["line"].clone()).unwrap();
        line.transform = Transform2D {
            translation: Vec2::new(3.0, -2.0),
            rotation: -0.4,
            scale: Vec2::new(0.75, 0.75),
        };
        let line_json = serde_json::to_string(&line).unwrap();
        let point: [f64; 2] =
            serde_json::from_str(&plan.number_to_point_json(1.25, &line_json).unwrap()).unwrap();
        let number = plan
            .point_to_number(point[0] as f32, point[1] as f32, &line_json)
            .unwrap();
        assert!((number - 1.25).abs() <= 1.0e-5);
    }

    #[test]
    fn non_line_snapshot_is_rejected() {
        let plan = NumberLineAuthoringPlan::from_json(request()).unwrap();
        let snapshot = noon::Circle::new(1.0).into_snapshot();
        assert_eq!(
            plan.number_to_point_json(0.0, &serde_json::to_string(&snapshot).unwrap()),
            Err(ManimNumberLineBridgeError::InvalidGeometry)
        );
    }
}
