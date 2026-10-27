//! Checked object-local linear paint values for #1897.
//!
//! Coordinates are normalized to the source's local geometry bounds: (0, 0) is
//! the lower-left corner, (1, 1) the upper-right. Moving or scaling an object does
//! not rewrite its paint. Stops interpolate premultiplied, encoded-sRGB channels;
//! sampling does not apply coverage, fill opacity, or final object opacity.
//!
//! These inert values define paint, not scene identity, playback or GPU state.
//! Shared authoring/lowering decides where a paint may be attached; constructing
//! a value does not allocate scene identity or GPU state.

use super::gradient::{checked_stops, sample_stops, unit};
use crate::{GradientError, GradientStop};
use std::sync::Arc;

/// Immutable, shareable linear-gradient definition with pad extension.
///
/// Project the normalized local point onto the start/end axis. Values before
/// the first stop use its color; values after the last use the last color.
/// Repeated stop positions are hard edges: exactly at an edge, the last stop at
/// that position wins. Stops are never silently sorted or deduplicated.
///
/// ```
/// use noon_core::{Color, GradientStop, LinearGradient, GradientError};
/// # fn main() -> Result<(), GradientError> {
/// let paint = LinearGradient::new([0.0, 0.5], [1.0, 0.5], [
///     GradientStop::new(0.0, Color::rgb(1.0, 0.0, 0.0))?,
///     GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0))?,
/// ])?;
/// assert_eq!(paint.sample_premultiplied([0.5, 0.2])?, [0.5, 0.0, 0.5, 1.0]);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "LinearGradientInput")]
pub struct LinearGradient {
    start: [f64; 2],
    end: [f64; 2],
    stops: Arc<[GradientStop]>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LinearGradientInput {
    start: [f64; 2],
    end: [f64; 2],
    stops: Vec<GradientStop>,
}

impl TryFrom<LinearGradientInput> for LinearGradient {
    type Error = GradientError;

    fn try_from(value: LinearGradientInput) -> Result<Self, Self::Error> {
        Self::new(value.start, value.end, value.stops)
    }
}

impl LinearGradient {
    pub fn new(
        start: [f64; 2],
        end: [f64; 2],
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Result<Self, GradientError> {
        if !start.into_iter().chain(end).all(unit) {
            return Err(GradientError::InvalidEndpoints);
        }
        if start == end {
            return Err(GradientError::DegenerateAxis);
        }
        let stops = checked_stops(stops)?;
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
    pub fn sample_premultiplied(&self, point: [f64; 2]) -> Result<[f64; 4], GradientError> {
        if !point.into_iter().all(f64::is_finite) {
            return Err(GradientError::InvalidSamplePoint);
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
            return Err(GradientError::InvalidSamplePoint);
        }
        Ok(sample_stops(&self.stops, progress))
    }
}

#[cfg(test)]
mod tests;
