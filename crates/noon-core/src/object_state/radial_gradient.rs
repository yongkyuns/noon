//! Checked elliptical radial paint in the normalized local geometry box.
use super::gradient::{checked_stops, sample_stops, unit};
use crate::{GradientError, GradientStop};
use std::sync::Arc;

/// An inert, immutable radial-gradient value, with pad extension.
///
/// `(0, 0)` is the local box's lower-left corner. `radii` are positive lengths
/// along its normalized x/y axes; `[0.5, 0.5]` reaches each edge from the center.
/// The field coordinate is `hypot((x-cx)/rx, (y-cy)/ry)`. Thus a square box
/// produces circular contours for equal radii, while a nonsquare box produces
/// ellipses. Geometry transforms carry the paint with the object.
///
/// Stops use the same right-continuous hard edges and encoded-premultiplied
/// interpolation as [`crate::LinearGradient`]. Sampling includes no geometry
/// coverage, fill opacity or final scope opacity. Construction is inert;
/// shared authoring/lowering decides where a paint may be attached.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "RadialGradientInput")]
pub struct RadialGradient {
    center: [f64; 2],
    radii: [f64; 2],
    stops: Arc<[GradientStop]>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RadialGradientInput {
    center: [f64; 2],
    radii: [f64; 2],
    stops: Vec<GradientStop>,
}

impl TryFrom<RadialGradientInput> for RadialGradient {
    type Error = GradientError;

    fn try_from(value: RadialGradientInput) -> Result<Self, Self::Error> {
        Self::new(value.center, value.radii, value.stops)
    }
}

impl RadialGradient {
    pub fn new(
        center: [f64; 2],
        radii: [f64; 2],
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Result<Self, GradientError> {
        if !center.into_iter().all(unit) {
            return Err(GradientError::InvalidCenter);
        }
        if radii.into_iter().any(|v| !v.is_finite() || v <= 0.0) {
            return Err(GradientError::InvalidRadii);
        }
        Ok(Self {
            center,
            radii,
            stops: checked_stops(stops)?,
        })
    }

    pub const fn center(&self) -> [f64; 2] {
        self.center
    }

    pub const fn radii(&self) -> [f64; 2] {
        self.radii
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    pub fn sample_premultiplied(&self, point: [f64; 2]) -> Result<[f64; 4], GradientError> {
        if !point.into_iter().all(f64::is_finite) {
            return Err(GradientError::InvalidSamplePoint);
        }
        // hypot avoids overflow/underflow from squaring. Positive-infinite
        // distance from a finite extreme point has the well-defined pad color.
        let distance = ((point[0] - self.center[0]) / self.radii[0])
            .hypot((point[1] - self.center[1]) / self.radii[1]);
        Ok(sample_stops(&self.stops, distance))
    }
}

#[cfg(test)]
mod tests;
