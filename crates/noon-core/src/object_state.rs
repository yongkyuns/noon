//! Typed appearance values and numeric validation for object/track state.
use crate::{Color, GeometryRef, ObjectId, Style, Transform2D, Vec2};

mod backdrop_blur;
mod bloom;
mod blur;
pub use backdrop_blur::BackdropBlur;
pub use bloom::Bloom;
mod shadow;
pub use shadow::Shadow;
pub(crate) mod effect_parameters;
mod glow;
mod gradient;
mod hatch_paint;
mod linear_gradient;
mod noise_dissolve;
mod pixelate;
pub use pixelate::Pixelate;
mod noise_paint;
mod paint_transition;
mod radial_gradient;
pub use blur::Blur;
pub(crate) use effect_parameters::sealed as effect_value_sealed;
pub use effect_parameters::{
    EffectOffset, EffectParameter, EffectParameterError, EffectRadius, EffectUpdate, EffectValue,
    Pixels, PreparedEffectUpdate,
};
pub use glow::{Glow, GlowSource};
pub use gradient::{GradientError, GradientStop};
pub use hatch_paint::{HatchPaint, HatchPaintError, HatchPaintOptions};
pub use linear_gradient::LinearGradient;
pub use noise_dissolve::{DissolveDirection, DissolveStyle, NoiseDissolve, NoiseDissolveError};
pub use noise_paint::{NoisePaint, NoisePaintError};

/// Checked paint callers supply unit progress and unit colors. Keep stop and
/// procedural-color interpolation on the same encoded-premultiplied operation.
pub(crate) fn interpolate_premultiplied_color(from: Color, to: Color, alpha: f64) -> Color {
    let premultiplied = |c: Color| {
        let a = f64::from(c.alpha);
        [
            f64::from(c.red) * a,
            f64::from(c.green) * a,
            f64::from(c.blue) * a,
            a,
        ]
    };
    let a = premultiplied(from);
    let b = premultiplied(to);
    let c: [f64; 4] = std::array::from_fn(|i| a[i] * (1.0 - alpha) + b[i] * alpha);
    if c[3] == 0.0 {
        return Color::TRANSPARENT;
    }
    Color::rgba(
        (c[0] / c[3]) as f32,
        (c[1] / c[3]) as f32,
        (c[2] / c[3]) as f32,
        c[3] as f32,
    )
}
pub use paint_transition::{PaintTransition, PaintTransitionError};
pub use radial_gradient::RadialGradient;
mod image_paint;
pub use image_paint::{
    ImageAddressMode, ImageFit, ImageMapping, ImagePaint, ImagePaintError, ImageUvTransform,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectStateField {
    Geometry,
    Transform,
    Style,
    ViewColorProfile,
}

impl std::fmt::Display for ObjectStateField {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Geometry => "geometry",
            Self::Transform => "transform",
            Self::Style => "style",
            Self::ViewColorProfile => "view color profile",
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
        && style.background_stroke.is_none_or(color_is_finite)
        && style.stroke_width.is_finite()
        && style.background_stroke_width.is_finite()
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
