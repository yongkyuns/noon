use noon::{Axes2DState, CoordinateSystemError, NumberRange, TransformedAxes2DState};
use noon_core::{GeometryRef, ObjectSnapshot, Vec2};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AxesQueryRequest {
    x_range: [f64; 3],
    y_range: [f64; 3],
    x_length: f32,
    y_length: f32,
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
    InvalidAxisSnapshot { axis: &'static str, error: String },
    InvalidAxisGeometry(&'static str),
    Coordinates(CoordinateSystemError),
    Serialization(String),
}

impl std::fmt::Display for ManimAxesQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => write!(formatter, "invalid Axes query request: {error}"),
            Self::InvalidAxisSnapshot { axis, error } => {
                write!(formatter, "invalid Axes {axis}-axis snapshot: {error}")
            }
            Self::InvalidAxisGeometry(axis) => {
                write!(
                    formatter,
                    "Axes {axis}-axis snapshot must contain line geometry"
                )
            }
            Self::Coordinates(error) => error.fmt(formatter),
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

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::AxesQueryPlan;

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
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmAxesQueryPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use noon::{IntoSnapshot, Line};
    use noon_core::Transform2D;

    fn request_json() -> &'static str {
        r#"{"x_range":[-2,2,1],"y_range":[-2,2,1],"x_length":4,"y_length":4}"#
    }

    fn axis_snapshot(axis: noon::NumberLineState, transform: Transform2D) -> String {
        let mut snapshot = Line::new(axis.start(), axis.end()).into_snapshot();
        snapshot.transform = transform;
        serde_json::to_string(&snapshot).unwrap()
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
    fn non_line_snapshot_fails_closed() {
        let plan = AxesQueryPlan::from_json(request_json()).unwrap();
        let bad = serde_json::to_string(&ObjectSnapshot::new(GeometryRef::circle(1.0))).unwrap();
        let good = axis_snapshot(plan.axes.y_axis(), Transform2D::IDENTITY);
        assert_eq!(
            plan.coords_to_point_json(0.0, 0.0, &bad, &good)
                .unwrap_err(),
            ManimAxesQueryError::InvalidAxisGeometry("x")
        );
    }
}
