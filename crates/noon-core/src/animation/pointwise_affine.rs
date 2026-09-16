//! Checked endpoints for corner-linear affine motion on the existing TRS frame.
use crate::Transform2D;
use serde::{Deserialize, Serialize};

/// Immutable execution parameters, not a separate animation or mutable frame.
///
/// Linear interpolation of two rotated/scaled bases can introduce shear. The
/// current TRS execution frame can represent the interpolation only when the
/// cross-column term vanishes. Reject other pairs rather than silently replacing
/// corner correspondence with interpolation of rotation/scale parameters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PointwiseAffineEndpoints {
    from: Transform2D,
    to: Transform2D,
}

impl PointwiseAffineEndpoints {
    pub fn new(from: Transform2D, to: Transform2D) -> Option<Self> {
        if [from, to].into_iter().any(|t| {
            [
                t.translation.x,
                t.translation.y,
                t.rotation,
                t.scale.x,
                t.scale.y,
            ]
            .into_iter()
            .any(|v| !v.is_finite())
        }) {
            return None;
        }
        let cross = f64::from(from.scale.x) * f64::from(to.scale.y)
            - f64::from(from.scale.y) * f64::from(to.scale.x);
        let scale = f64::from(from.scale.x.abs().max(from.scale.y.abs()))
            * f64::from(to.scale.x.abs().max(to.scale.y.abs()));
        let angle = f64::from(to.rotation) - f64::from(from.rotation);
        // Allow only rounding at the precision of the actual execution values.
        // Both endpoint bases are orthogonal; the mixed term below is therefore
        // the coefficient of alpha*(1-alpha) in their interpolated column dot.
        let tolerance = 4.0 * f64::from(f32::EPSILON) * scale;
        (cross.abs() * angle.sin().abs() <= tolerance).then_some(Self { from, to })
    }

    pub const fn from(self) -> Transform2D {
        self.from
    }
    pub const fn to(self) -> Transform2D {
        self.to
    }

    /// Revalidate decoded external execution data before publishing a track.
    pub fn is_valid(self) -> bool {
        Self::new(self.from, self.to).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        validate_track_definition, CompositionTimeMap, ObjectId, Property, RateFunction,
        TimelineError, TrackDefinition, TrackId, TrackTiming, TrackValues, Vec2,
    };

    #[test]
    fn representability_is_checked_before_track_publication() {
        let from = Transform2D::IDENTITY;
        let rotated = Transform2D {
            rotation: 0.8,
            scale: Vec2::new(0.75, 0.75),
            ..from
        };
        assert!(PointwiseAffineEndpoints::new(from, rotated).is_some());
        let shear = Transform2D {
            scale: Vec2::new(0.75, 0.25),
            ..rotated
        };
        assert!(PointwiseAffineEndpoints::new(from, shear).is_none());
        assert!(PointwiseAffineEndpoints::new(
            from,
            Transform2D {
                rotation: 0.0,
                ..shear
            }
        )
        .is_some());
        assert!(PointwiseAffineEndpoints::new(
            from,
            Transform2D {
                rotation: f32::NAN,
                ..rotated
            }
        )
        .is_none());
        let endpoints = PointwiseAffineEndpoints::new(from, rotated).unwrap();
        let mut track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(0),
            property: Property::Rotation,
            values: TrackValues::PointwiseRotation(endpoints),
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        validate_track_definition(&track).unwrap();
        track.property = Property::Opacity;
        assert_eq!(
            validate_track_definition(&track),
            Err(TimelineError::InvalidPointwiseAffine(Property::Opacity))
        );
        track.property = Property::Scale;
        track.values = TrackValues::PointwiseScale(endpoints);
        validate_track_definition(&track).unwrap();
        track.property = Property::Position;
        assert_eq!(
            validate_track_definition(&track),
            Err(TimelineError::InvalidPointwiseAffine(Property::Position))
        );
        track.property = Property::Scale;
        // Serde can construct private fields; track validation must still reject them.
        track.values = TrackValues::PointwiseScale(PointwiseAffineEndpoints { from, to: shear });
        assert_eq!(
            validate_track_definition(&track),
            Err(TimelineError::InvalidPointwiseAffine(Property::Scale))
        );
    }
}
