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

/// Project language-boundary arrays without owning defaults or mask semantics.
#[allow(clippy::too_many_arguments)]
pub fn radial_parameters(
    mut update: EffectUpdate,
    center: &[f64],
    axes: &[f64],
    roughness: Option<f64>,
    frequency: Option<f64>,
    rim_width: Option<f64>,
    inner_color: &[f64],
    outer_color: &[f64],
    hole: Option<bool>,
) -> Result<EffectUpdate, AuthoringFailure> {
    let pair = |values: &[f64], parameter| match values {
        [] => Ok(None),
        [x, y] => Ok(Some([*x, *y])),
        _ => Err(crate::AuthoringError::from(
            EffectParameterError::InvalidRadialParameter(parameter),
        )),
    };
    update.radial = noon_core::RadialUpdate {
        center: pair(center, noon_core::RadialParameter::Center)?,
        axes: pair(axes, noon_core::RadialParameter::Axes)?,
        roughness,
        frequency,
        rim_width,
        inner_color: effect_color(inner_color)?,
        outer_color: effect_color(outer_color)?,
        hole,
    };
    update.validate().map_err(crate::AuthoringError::from)?;
    Ok(update)
}

pub fn radial_edge(edge: Option<&str>) -> Result<Option<noon_core::RadialEdge>, AuthoringFailure> {
    match edge {
        None => Ok(None),
        Some("clean") => Ok(Some(noon_core::RadialEdge::Clean)),
        Some("burn") => Ok(Some(noon_core::RadialEdge::Burn)),
        Some(_) => Err(AuthoringFailure::new(
            "invalid_input",
            "effect.invalid_radial_edge",
            "radial edge must be clean or burn",
        )),
    }
}
