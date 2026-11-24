//! Optional language coercion into the shared immutable Rust paint value.
use crate::integration::AuthoringFailure;
use crate::{
    Color, GradientStop, HatchPaint, HatchPaintOptions, LinearGradient, NoisePaint, RadialGradient,
};
use std::sync::Arc;

pub fn linear_gradient(
    start: &[f64],
    end: &[f64],
    stops: &[f64],
) -> Result<Arc<LinearGradient>, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_gradient", message);
    let start: [f64; 2] = start
        .try_into()
        .map_err(|_| invalid("gradient start requires two coordinates"))?;
    let end: [f64; 2] = end
        .try_into()
        .map_err(|_| invalid("gradient end requires two coordinates"))?;
    let stops = gradient_stops(stops)?;
    LinearGradient::new(start, end, stops)
        .map(Arc::new)
        .map_err(|error| invalid(&error.to_string()))
}

fn gradient_stops(stops: &[f64]) -> Result<Vec<GradientStop>, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_gradient", message);
    if !stops.len().is_multiple_of(5) {
        return Err(invalid(
            "gradient stops require position and four color channels",
        ));
    }
    stops
        .as_chunks::<5>()
        .0
        .iter()
        .map(|stop| {
            if !stop[1..]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            {
                return Err(invalid(
                    "gradient colors require finite RGBA channels in [0, 1]",
                ));
            }
            GradientStop::new(
                stop[0],
                Color::rgba(
                    stop[1] as f32,
                    stop[2] as f32,
                    stop[3] as f32,
                    stop[4] as f32,
                ),
            )
            .map_err(|error| invalid(&error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()
}

pub fn radial_gradient(
    center: &[f64],
    radii: &[f64],
    stops: &[f64],
) -> Result<Arc<RadialGradient>, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_gradient", message);
    let center: [f64; 2] = center
        .try_into()
        .map_err(|_| invalid("radial center requires two coordinates"))?;
    let radii: [f64; 2] = radii
        .try_into()
        .map_err(|_| invalid("radial radii require two lengths"))?;
    RadialGradient::new(center, radii, gradient_stops(stops)?)
        .map(Arc::new)
        .map_err(|error| invalid(&error.to_string()))
}

pub fn noise_paint(
    seed: f64,
    frequency: &[f64],
    phase: &[f64],
    colors: &[f64],
) -> Result<Arc<NoisePaint>, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_noise", message);
    if !seed.is_finite() || seed < 0.0 || seed > f64::from(u32::MAX) || seed.fract() != 0.0 {
        return Err(invalid("noise seed requires an integer in [0, 4294967295]"));
    }
    let frequency: [f64; 2] = frequency
        .try_into()
        .map_err(|_| invalid("noise frequency requires two cell counts"))?;
    let phase: [f64; 2] = phase
        .try_into()
        .map_err(|_| invalid("noise phase requires two coordinates"))?;
    if colors.len() != 8
        || colors
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(invalid("noise requires two finite RGBA colors in [0, 1]"));
    }
    let colors = [
        Color::rgba(
            colors[0] as f32,
            colors[1] as f32,
            colors[2] as f32,
            colors[3] as f32,
        ),
        Color::rgba(
            colors[4] as f32,
            colors[5] as f32,
            colors[6] as f32,
            colors[7] as f32,
        ),
    ];
    NoisePaint::new(seed as u32, frequency, phase, colors)
        .map(Arc::new)
        .map_err(|e| invalid(&e.to_string()))
}

pub fn hatch_paint(
    spacing: f64,
    width: f64,
    angle: f64,
    phase: &[f64],
    cross: bool,
    colors: &[f64],
) -> Result<Arc<HatchPaint>, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_hatch", message);
    let phase: [f64; 2] = phase
        .try_into()
        .map_err(|_| invalid("hatch phase requires two period coordinates"))?;
    if colors.len() != 8
        || colors
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(invalid(
            "hatching requires foreground and background RGBA colors in [0, 1]",
        ));
    }
    let color = |offset| {
        Color::rgba(
            colors[offset] as f32,
            colors[offset + 1] as f32,
            colors[offset + 2] as f32,
            colors[offset + 3] as f32,
        )
    };
    HatchPaint::new(
        color(0),
        color(4),
        HatchPaintOptions {
            spacing,
            width,
            angle,
            phase,
            cross,
        },
    )
    .map(Arc::new)
    .map_err(|error| invalid(&error.to_string()))
}

/// Checked input projection for immutable bitmap fill mapping.
pub fn image_mapping(
    fit: &str,
    alignment: &[f64],
    address: &str,
    translation: &[f64],
    scale: &[f64],
    rotation: f64,
) -> Result<crate::ImageMapping, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.image_mapping", message);
    let fit = match fit {
        "cover" => crate::ImageFit::Cover,
        "contain" => crate::ImageFit::Contain,
        "stretch" => crate::ImageFit::Stretch,
        _ => return Err(invalid("image fit must be cover, contain, or stretch")),
    };
    let address = match address {
        "transparent" => crate::ImageAddressMode::Transparent,
        "clamp" => crate::ImageAddressMode::Clamp,
        "repeat" => crate::ImageAddressMode::Repeat,
        _ => {
            return Err(invalid(
                "image address must be transparent, clamp, or repeat",
            ))
        }
    };
    let pair = |value: &[f64]| -> Result<[f64; 2], AuthoringFailure> {
        value
            .try_into()
            .map_err(|_| invalid("image mapping requires two coordinates per vector"))
    };
    let transform = crate::ImageUvTransform::new(pair(translation)?, pair(scale)?, rotation)
        .map_err(|error| invalid(&error.to_string()))?;
    let value = crate::ImageMapping::new(fit, pair(alignment)?, address, transform)
        .map_err(|error| invalid(&error.to_string()))?;
    Ok(value)
}
