use crate::{Axes2DState, CoordinateSystemError, NumberLineState};
use noon_core::{Transform2D, Vec2};

/// A NumberLine coordinate view whose affine placement comes from the current
/// retained mobject transform rather than from duplicated frontend state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformedNumberLineState {
    line: NumberLineState,
    transform: Transform2D,
}

impl TransformedNumberLineState {
    pub const fn new(line: NumberLineState, transform: Transform2D) -> Self {
        Self { line, transform }
    }

    pub const fn line(self) -> NumberLineState {
        self.line
    }

    pub const fn transform(self) -> Transform2D {
        self.transform
    }

    pub fn number_to_point(self, number: f64) -> Result<Vec2, CoordinateSystemError> {
        let point = self
            .transform
            .transform_point(self.line.number_to_point(number)?);
        if vec2_is_finite(point) {
            Ok(point)
        } else {
            Err(CoordinateSystemError::NonFinitePoint(point))
        }
    }

    pub fn point_to_number(self, point: Vec2) -> Result<f64, CoordinateSystemError> {
        if !vec2_is_finite(point) {
            return Err(CoordinateSystemError::NonFinitePoint(point));
        }

        let start = self.transform.transform_point(self.line.start());
        let end = self.transform.transform_point(self.line.end());
        if !vec2_is_finite(start) {
            return Err(CoordinateSystemError::NonFinitePoint(start));
        }
        if !vec2_is_finite(end) {
            return Err(CoordinateSystemError::NonFinitePoint(end));
        }

        let delta = end - start;
        let length_squared =
            f64::from(delta.x) * f64::from(delta.x) + f64::from(delta.y) * f64::from(delta.y);
        if !length_squared.is_finite() || length_squared <= 0.0 {
            return Err(CoordinateSystemError::DegenerateLine);
        }

        let from_start = point - start;
        let projection = f64::from(from_start.x) * f64::from(delta.x)
            + f64::from(from_start.y) * f64::from(delta.y);
        let alpha = projection / length_squared;
        let range = self.line.range();
        Ok(range.min() + alpha * range.span())
    }
}

/// Transform-aware view of the same two retained axis lines that render an Axes.
///
/// Manim composes coordinates vectorially: start from `x_axis.n2p(x)` and add
/// `y_axis.n2p(y) - origin`, where origin is the x-axis origin-shift point. Keeping
/// each current retained transform explicit makes c2p/p2c remain correct after
/// group shifts, rotations, and scales without mirroring mutable coordinate state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformedAxes2DState {
    axes: Axes2DState,
    x_transform: Transform2D,
    y_transform: Transform2D,
}

impl TransformedAxes2DState {
    pub const fn new(
        axes: Axes2DState,
        x_transform: Transform2D,
        y_transform: Transform2D,
    ) -> Self {
        Self {
            axes,
            x_transform,
            y_transform,
        }
    }

    pub const fn axes(self) -> Axes2DState {
        self.axes
    }

    pub fn x_axis(self) -> TransformedNumberLineState {
        TransformedNumberLineState::new(self.axes.x_axis(), self.x_transform)
    }

    pub fn y_axis(self) -> TransformedNumberLineState {
        TransformedNumberLineState::new(self.axes.y_axis(), self.y_transform)
    }

    pub fn coords_to_point(self, x: f64, y: f64) -> Result<Vec2, CoordinateSystemError> {
        let x_axis = self.x_axis();
        let y_axis = self.y_axis();
        let origin = x_axis.number_to_point(self.axes.x_axis().range().origin_shift())?;
        let point = x_axis.number_to_point(x)? + y_axis.number_to_point(y)? - origin;
        if vec2_is_finite(point) {
            Ok(point)
        } else {
            Err(CoordinateSystemError::NonFinitePoint(point))
        }
    }

    pub fn point_to_coords(self, point: Vec2) -> Result<(f64, f64), CoordinateSystemError> {
        Ok((
            self.x_axis().point_to_number(point)?,
            self.y_axis().point_to_number(point)?,
        ))
    }

    pub fn origin(self) -> Result<Vec2, CoordinateSystemError> {
        self.coords_to_point(0.0, 0.0)
    }
}

fn vec2_is_finite(value: Vec2) -> bool {
    value.x.is_finite() && value.y.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NumberRange;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_point(actual: Vec2, expected: Vec2) {
        assert_close(f64::from(actual.x), f64::from(expected.x));
        assert_close(f64::from(actual.y), f64::from(expected.y));
    }

    #[test]
    fn shared_affine_transform_preserves_vector_coordinate_composition() {
        let axes = Axes2DState::new(
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            4.0,
            4.0,
        )
        .unwrap();
        let transform = Transform2D {
            translation: Vec2::new(3.0, -1.0),
            rotation: 0.6,
            scale: Vec2::new(1.5, 1.5),
        };
        let transformed = TransformedAxes2DState::new(axes, transform, transform);

        let expected = transform.transform_point(Vec2::new(1.0, -0.5));
        let actual = transformed.coords_to_point(1.0, -0.5).unwrap();
        assert_point(actual, expected);
        let round_trip = transformed.point_to_coords(actual).unwrap();
        assert_close(round_trip.0, 1.0);
        assert_close(round_trip.1, -0.5);
    }

    #[test]
    fn independent_axis_scales_use_each_retained_line_transform() {
        let axes = Axes2DState::new(
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            4.0,
            4.0,
        )
        .unwrap();
        let x_transform = Transform2D {
            translation: Vec2::new(2.0, 3.0),
            rotation: 0.0,
            scale: Vec2::new(2.0, 1.0),
        };
        let y_transform = Transform2D {
            translation: Vec2::new(2.0, 3.0),
            rotation: 0.0,
            scale: Vec2::new(1.0, 3.0),
        };
        let transformed = TransformedAxes2DState::new(axes, x_transform, y_transform);

        let point = transformed.coords_to_point(1.0, 1.0).unwrap();
        assert_point(point, Vec2::new(4.0, 6.0));
        let coords = transformed.point_to_coords(point).unwrap();
        assert_close(coords.0, 1.0);
        assert_close(coords.1, 1.0);
    }

    #[test]
    fn positive_only_axis_origin_shift_matches_manim_after_transform() {
        let axes = Axes2DState::new(
            NumberRange::new(2.0, 6.0, 1.0).unwrap(),
            NumberRange::new(-1.0, 3.0, 1.0).unwrap(),
            8.0,
            4.0,
        )
        .unwrap();
        let transform = Transform2D {
            translation: Vec2::new(-1.0, 2.0),
            rotation: -0.3,
            scale: Vec2::new(0.75, 0.75),
        };
        let transformed = TransformedAxes2DState::new(axes, transform, transform);
        let base_point = axes.coords_to_point(4.0, 1.0).unwrap();
        assert_point(
            transformed.coords_to_point(4.0, 1.0).unwrap(),
            transform.transform_point(base_point),
        );
    }

    #[test]
    fn collapsed_retained_axis_rejects_projection() {
        let axes = Axes2DState::new(
            NumberRange::new(-1.0, 1.0, 1.0).unwrap(),
            NumberRange::new(-1.0, 1.0, 1.0).unwrap(),
            2.0,
            2.0,
        )
        .unwrap();
        let collapsed = Transform2D {
            translation: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ZERO,
        };
        let transformed = TransformedAxes2DState::new(axes, collapsed, Transform2D::IDENTITY);
        assert_eq!(
            transformed.point_to_coords(Vec2::ZERO),
            Err(CoordinateSystemError::DegenerateLine)
        );
    }
}
