//! Numeric validation shared by typed execution objects and track endpoints.
use crate::{Color, GeometryRef, ObjectId, Style, Transform2D, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectStateField {
    Geometry,
    Transform,
    Style,
}

impl std::fmt::Display for ObjectStateField {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Geometry => "geometry",
            Self::Transform => "transform",
            Self::Style => "style",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectStateError {
    pub object: ObjectId,
    pub field: ObjectStateField,
}

impl std::fmt::Display for ObjectStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "object {} contains non-finite {} state",
            self.object.get(),
            self.field
        )
    }
}
impl std::error::Error for ObjectStateError {}

pub fn validate_geometry(object: ObjectId, geometry: &GeometryRef) -> Result<(), ObjectStateError> {
    let valid = geometry.is_finite();
    valid.then_some(()).ok_or(ObjectStateError {
        object,
        field: ObjectStateField::Geometry,
    })
}

pub fn validate_transform(
    object: ObjectId,
    transform: Transform2D,
) -> Result<(), ObjectStateError> {
    let valid = vec2_is_finite(transform.translation)
        && transform.rotation.is_finite()
        && vec2_is_finite(transform.scale);
    valid.then_some(()).ok_or(ObjectStateError {
        object,
        field: ObjectStateField::Transform,
    })
}

pub fn validate_style(object: ObjectId, style: Style) -> Result<(), ObjectStateError> {
    let valid = style.fill.is_none_or(color_is_finite)
        && style.stroke.is_none_or(color_is_finite)
        && style.stroke_width.is_finite()
        && style.opacity.is_finite();
    valid.then_some(()).ok_or(ObjectStateError {
        object,
        field: ObjectStateField::Style,
    })
}

fn vec2_is_finite(value: Vec2) -> bool {
    value.x.is_finite() && value.y.is_finite()
}

fn color_is_finite(color: Color) -> bool {
    color.red.is_finite()
        && color.green.is_finite()
        && color.blue.is_finite()
        && color.alpha.is_finite()
}
