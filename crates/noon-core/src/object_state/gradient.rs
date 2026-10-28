//! Shared checked stop values and encoded-premultiplied interpolation.
use crate::Color;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientError {
    InvalidColor,
    InvalidStopPosition,
    InvalidEndpoints,
    DegenerateAxis,
    TooFewStops,
    UnorderedStops,
    InvalidSamplePoint,
    InvalidCenter,
    InvalidRadii,
}

impl std::fmt::Display for GradientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidColor => "gradient colors require finite RGBA channels in [0, 1]",
            Self::InvalidStopPosition => "gradient stop positions must be finite and in [0, 1]",
            Self::InvalidEndpoints => "gradient endpoints must be finite normalized local points",
            Self::DegenerateAxis => "gradient endpoints must be distinct",
            Self::TooFewStops => "a gradient requires at least two stops",
            Self::UnorderedStops => "gradient stops must be supplied in nondecreasing order",
            Self::InvalidCenter => "gradient center requires finite normalized local coordinates",
            Self::InvalidRadii => "radial gradient radii must be positive and finite",
            Self::InvalidSamplePoint => "gradient sample point must be finite",
        })
    }
}

impl std::error::Error for GradientError {}

/// One immutable straight encoded-sRGB color and normalized stop position.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "GradientStopInput")]
pub struct GradientStop {
    position: f64,
    color: Color,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GradientStopInput {
    position: f64,
    color: Color,
}

impl TryFrom<GradientStopInput> for GradientStop {
    type Error = GradientError;

    fn try_from(value: GradientStopInput) -> Result<Self, Self::Error> {
        Self::new(value.position, value.color)
    }
}

impl GradientStop {
    pub fn new(position: f64, color: Color) -> Result<Self, GradientError> {
        if !unit(position) {
            return Err(GradientError::InvalidStopPosition);
        }
        if ![color.red, color.green, color.blue, color.alpha]
            .into_iter()
            .all(|channel| unit(f64::from(channel)))
        {
            return Err(GradientError::InvalidColor);
        }
        Ok(Self { position, color })
    }

    pub const fn position(self) -> f64 {
        self.position
    }

    pub const fn color(self) -> Color {
        self.color
    }

    pub(super) fn premultiplied(self) -> [f64; 4] {
        let alpha = f64::from(self.color.alpha);
        [
            f64::from(self.color.red) * alpha,
            f64::from(self.color.green) * alpha,
            f64::from(self.color.blue) * alpha,
            alpha,
        ]
    }
}

pub(super) fn checked_stops(
    stops: impl IntoIterator<Item = GradientStop>,
) -> Result<Arc<[GradientStop]>, GradientError> {
    let stops: Arc<[GradientStop]> = stops.into_iter().collect();
    if stops.len() < 2 {
        return Err(GradientError::TooFewStops);
    }
    if stops
        .windows(2)
        .any(|pair| pair[0].position > pair[1].position)
    {
        return Err(GradientError::UnorderedStops);
    }
    Ok(stops)
}

pub(super) fn sample_stops(stops: &[GradientStop], progress: f64) -> [f64; 4] {
    let right = stops.partition_point(|stop| stop.position <= progress);
    if right == 0 {
        return stops[0].premultiplied();
    }
    let left = stops[right - 1];
    if right == stops.len() || left.position == progress {
        return left.premultiplied();
    }
    let right = stops[right];
    let alpha = (progress - left.position) / (right.position - left.position);
    let from = left.premultiplied();
    let to = right.premultiplied();
    std::array::from_fn(|i| from[i] + (to[i] - from[i]) * alpha)
}

pub(super) fn unit(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}
