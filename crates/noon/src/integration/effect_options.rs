//! Shared language argument projection before Rust effect validation.
use super::AuthoringFailure;
use crate::effects::{EffectParameterError, EffectRadius, EffectUpdate, GlowSource};
use crate::Color;

pub fn effect_color(color: &[f64]) -> Result<Option<Color>, AuthoringFailure> {
    match color {
        [] => Ok(None),
        [r, g, b, a]
            if color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)) =>
        {
            // Check before narrowing so an out-of-range f64 cannot round into
            // an accepted f32. The complete schema is validated in shared Rust.
            Ok(Some(Color::rgba(
                *r as f32, *g as f32, *b as f32, *a as f32,
            )))
        }
        _ => Err(crate::AuthoringError::from(EffectParameterError::InvalidColor).into()),
    }
}

pub fn effect_update(
    color: &[f64],
    radius: Option<f64>,
    pixels: bool,
    intensity: Option<f64>,
    source: Option<&str>,
    offset: &[f64],
    offset_pixels: bool,
) -> Result<EffectUpdate, AuthoringFailure> {
    let color = effect_color(color)?;
    let source = match source {
        None => None,
        Some("painted") => Some(GlowSource::Painted),
        Some("silhouette") => Some(GlowSource::Silhouette),
        Some(_) => {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "effect.invalid_source",
                "glow source must be painted or silhouette",
            ))
        }
    };
    if pixels && radius.is_none() {
        return Err(AuthoringFailure::new(
            "invalid_input",
            "effect.radius_unit_without_value",
            "a radius unit requires an explicit radius",
        ));
    }
    let offset = match offset {
        [] if !offset_pixels => None,
        [x, y] => Some(if offset_pixels {
            noon_core::EffectOffset::Pixels([*x, *y])
        } else {
            noon_core::EffectOffset::Scene([*x, *y])
        }),
        _ => return Err(crate::AuthoringError::from(EffectParameterError::InvalidOffset).into()),
    };
    let update = EffectUpdate {
        color,
        radius: radius.map(|v| {
            if pixels {
                EffectRadius::Pixels(v)
            } else {
                EffectRadius::Scene(v)
            }
        }),
        intensity,
        source,
        offset,
        ..EffectUpdate::default()
    };
    update.validate().map_err(crate::AuthoringError::from)?;
    Ok(update)
}

pub fn mask_parameters(
    mut update: EffectUpdate,
    progress: Option<f64>,
    softness: Option<f64>,
    seed: Option<f64>,
) -> Result<EffectUpdate, AuthoringFailure> {
    update.progress = progress;
    update.softness = softness;
    update.seed = match seed {
        None => None,
        Some(v)
            if v.is_finite() && v.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&v) =>
        {
            Some(v as u32)
        }
        _ => {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "effect.invalid_seed",
                "effect seed must be an integer in [0, 4294967295]",
            ))
        }
    };
    update.validate().map_err(crate::AuthoringError::from)?;
    Ok(update)
}
