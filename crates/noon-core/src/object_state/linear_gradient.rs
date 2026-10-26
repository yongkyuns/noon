//! Checked object-local linear paint values for #1897.
//!
//! Coordinates are normalized to the source's local geometry bounds: (0, 0) is
//! the lower-left corner, (1, 1) the upper-right. Moving or scaling an object does
//! not rewrite its paint. Stops interpolate premultiplied, encoded-sRGB channels;
//! sampling does not apply coverage, fill opacity, or final object opacity.
//!
//! These inert values define paint, not scene identity, playback or GPU state.
//! Public Scene/Python paint setters require the subsequent end-to-end lowering,
//! animation and renderer integration; constructing a value advertises none of it.

use crate::Color;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinearGradientError {
    InvalidColor,
    InvalidStopPosition,
    InvalidEndpoints,
    DegenerateAxis,
    TooFewStops,
    UnorderedStops,
    InvalidSamplePoint,
}

impl std::fmt::Display for LinearGradientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidColor => "gradient colors require finite RGBA channels in [0, 1]",
            Self::InvalidStopPosition => "gradient stop positions must be finite and in [0, 1]",
            Self::InvalidEndpoints => "gradient endpoints must be finite normalized local points",
            Self::DegenerateAxis => "gradient endpoints must be distinct",
            Self::TooFewStops => "a linear gradient requires at least two stops",
            Self::UnorderedStops => "gradient stops must be supplied in nondecreasing order",
            Self::InvalidSamplePoint => "gradient sample point must be finite",
        })
    }
}

impl std::error::Error for LinearGradientError {}

/// One immutable straight encoded-sRGB color and normalized stop position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    position: f64,
    color: Color,
}

impl GradientStop {
    pub fn new(position: f64, color: Color) -> Result<Self, LinearGradientError> {
        if !unit(position) {
            return Err(LinearGradientError::InvalidStopPosition);
        }
        if ![color.red, color.green, color.blue, color.alpha]
            .into_iter()
            .all(|channel| unit(f64::from(channel)))
        {
            return Err(LinearGradientError::InvalidColor);
        }
        Ok(Self { position, color })
    }

    pub const fn position(self) -> f64 {
        self.position
    }

    pub const fn color(self) -> Color {
        self.color
    }

    fn premultiplied(self) -> [f64; 4] {
        let alpha = f64::from(self.color.alpha);
        [
            f64::from(self.color.red) * alpha,
            f64::from(self.color.green) * alpha,
            f64::from(self.color.blue) * alpha,
            alpha,
        ]
    }
}

/// Immutable, shareable linear-gradient definition with pad extension.
///
/// Project the normalized local point onto the start/end axis. Values before
/// the first stop use its color; values after the last use the last color.
/// Repeated stop positions are hard edges: exactly at an edge, the last stop at
/// that position wins. Stops are never silently sorted or deduplicated.
///
/// ```
/// use noon_core::{Color, GradientStop, LinearGradient, LinearGradientError};
/// # fn main() -> Result<(), LinearGradientError> {
/// let paint = LinearGradient::new([0.0, 0.5], [1.0, 0.5], [
///     GradientStop::new(0.0, Color::rgb(1.0, 0.0, 0.0))?,
///     GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0))?,
/// ])?;
/// assert_eq!(paint.sample_premultiplied([0.5, 0.2])?, [0.5, 0.0, 0.5, 1.0]);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    start: [f64; 2],
    end: [f64; 2],
    stops: Arc<[GradientStop]>,
}

impl LinearGradient {
    pub fn new(
        start: [f64; 2],
        end: [f64; 2],
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Result<Self, LinearGradientError> {
        if !start.into_iter().chain(end).all(unit) {
            return Err(LinearGradientError::InvalidEndpoints);
        }
        if start == end {
            return Err(LinearGradientError::DegenerateAxis);
        }
        let stops: Arc<[GradientStop]> = stops.into_iter().collect();
        if stops.len() < 2 {
            return Err(LinearGradientError::TooFewStops);
        }
        if stops
            .windows(2)
            .any(|pair| pair[0].position > pair[1].position)
        {
            return Err(LinearGradientError::UnorderedStops);
        }
        Ok(Self { start, end, stops })
    }

    pub const fn start(&self) -> [f64; 2] {
        self.start
    }

    pub const fn end(&self) -> [f64; 2] {
        self.end
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    /// Sample premultiplied encoded-sRGB without coverage or scope opacity.
    ///
    /// The implementation rescales the axis before computing the projection,
    /// so a valid subnormal-length axis cannot underflow its squared length.
    /// Antialiasing may sample just outside the local box; pad applies there too.
    pub fn sample_premultiplied(&self, point: [f64; 2]) -> Result<[f64; 4], LinearGradientError> {
        if !point.into_iter().all(f64::is_finite) {
            return Err(LinearGradientError::InvalidSamplePoint);
        }
        let delta = [self.end[0] - self.start[0], self.end[1] - self.start[1]];
        let scale = delta[0].abs().max(delta[1].abs());
        let axis = [delta[0] / scale, delta[1] / scale];
        let offset = [point[0] - self.start[0], point[1] - self.start[1]];
        let projection =
            (offset[0] * axis[0] + offset[1] * axis[1]) / (axis[0] * axis[0] + axis[1] * axis[1]);
        // An extreme finite sample can overflow the projection to infinity;
        // pad still has a well-defined endpoint. A non-representable cancelling
        // projection must fail rather than leak NaN into published paint.
        let progress = projection / scale;
        if progress.is_nan() {
            return Err(LinearGradientError::InvalidSamplePoint);
        }
        let right = self.stops.partition_point(|stop| stop.position <= progress);
        if right == 0 {
            return Ok(self.stops[0].premultiplied());
        }
        let left = self.stops[right - 1];
        if right == self.stops.len() || left.position == progress {
            return Ok(left.premultiplied());
        }
        let right = self.stops[right];
        let alpha = (progress - left.position) / (right.position - left.position);
        let from = left.premultiplied();
        let to = right.premultiplied();
        Ok(std::array::from_fn(|i| from[i] + (to[i] - from[i]) * alpha))
    }
}

fn unit(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

#[cfg(test)]
mod tests;
