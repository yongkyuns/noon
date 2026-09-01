use crate::{Axes2DState, CoordinateSystemError, TransformedAxes2DState};
use noon_core::{Vec2, VectorPath};

/// Build ManimCE-style `Axes.plot_line_graph` corner geometry in canonical axis state.
///
/// The caller owns only host-language iterable normalization and styling. Coordinate
/// mapping and retained path construction stay in shared Rust semantics, and the
/// resulting `VectorPath` is immutable steady-state geometry.
pub fn axes_line_graph_vector_path(
    axes: Axes2DState,
    x_values: &[f64],
    y_values: &[f64],
) -> Result<VectorPath, LineGraphAuthoringError> {
    line_graph_vector_path_with_mapper(x_values, y_values, |x, y| axes.coords_to_point(x, y))
}

/// Build `Axes.plot_line_graph` against the current retained x/y-axis transforms.
///
/// This is the transform-safe path used after an Axes family has been shifted,
/// rotated, or scaled. It preserves the exact input order, including repeated x-values,
/// matching Manim's `set_points_as_corners` behavior.
pub fn transformed_axes_line_graph_vector_path(
    axes: TransformedAxes2DState,
    x_values: &[f64],
    y_values: &[f64],
) -> Result<VectorPath, LineGraphAuthoringError> {
    line_graph_vector_path_with_mapper(x_values, y_values, |x, y| axes.coords_to_point(x, y))
}

fn line_graph_vector_path_with_mapper<M>(
    x_values: &[f64],
    y_values: &[f64],
    mut coords_to_point: M,
) -> Result<VectorPath, LineGraphAuthoringError>
where
    M: FnMut(f64, f64) -> Result<Vec2, CoordinateSystemError>,
{
    if x_values.len() != y_values.len() {
        return Err(LineGraphAuthoringError::CoordinateCountMismatch {
            x_values: x_values.len(),
            y_values: y_values.len(),
        });
    }

    let mut path = VectorPath::new();
    for (index, (&x, &y)) in x_values.iter().zip(y_values).enumerate() {
        if !x.is_finite() || !y.is_finite() {
            return Err(LineGraphAuthoringError::NonFiniteCoordinate { index, x, y });
        }
        let point = coords_to_point(x, y)?;
        path = if index == 0 {
            path.move_to(point)
        } else {
            path.line_to(point)
        };
    }
    Ok(path)
}

#[derive(Clone, Debug, PartialEq)]
pub enum LineGraphAuthoringError {
    CoordinateCountMismatch { x_values: usize, y_values: usize },
    NonFiniteCoordinate { index: usize, x: f64, y: f64 },
    Coordinates(CoordinateSystemError),
}

impl std::fmt::Display for LineGraphAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CoordinateCountMismatch { x_values, y_values } => write!(
                formatter,
                "Axes.plot_line_graph requires equal x/y coordinate counts: {x_values} != {y_values}"
            ),
            Self::NonFiniteCoordinate { index, x, y } => write!(
                formatter,
                "Axes.plot_line_graph coordinate {index} must be finite: ({x}, {y})"
            ),
            Self::Coordinates(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for LineGraphAuthoringError {}

impl From<CoordinateSystemError> for LineGraphAuthoringError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NumberRange;
    use noon_core::{PathCommand, Transform2D};

    fn axes() -> Axes2DState {
        Axes2DState::new(
            NumberRange::new(0.0, 10.0, 1.0).unwrap(),
            NumberRange::new(-5.0, 5.0, 1.0).unwrap(),
            10.0,
            10.0,
        )
        .unwrap()
    }

    fn assert_point(actual: Vec2, expected: Vec2) {
        assert!(
            (actual - expected).length() <= 1.0e-5,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn preserves_input_order_as_corner_path() {
        let axes = axes();
        let path = axes_line_graph_vector_path(axes, &[0.0, 2.0, 2.0, 8.0], &[1.0, 3.0, -1.0, 4.0])
            .unwrap();

        assert_eq!(path.commands().len(), 4);
        let expected = [
            axes.coords_to_point(0.0, 1.0).unwrap(),
            axes.coords_to_point(2.0, 3.0).unwrap(),
            axes.coords_to_point(2.0, -1.0).unwrap(),
            axes.coords_to_point(8.0, 4.0).unwrap(),
        ];
        for (index, (command, expected)) in path.commands().iter().zip(expected).enumerate() {
            match command {
                PathCommand::MoveTo { to } if index == 0 => assert_point(*to, expected),
                PathCommand::LineTo { to } if index > 0 => assert_point(*to, expected),
                other => panic!("unexpected line graph command {index}: {other:?}"),
            }
        }
    }

    #[test]
    fn transformed_line_graph_uses_current_axis_affine_state() {
        let axes = axes();
        let transform = Transform2D {
            translation: Vec2::new(2.0, -1.0),
            rotation: 0.4,
            scale: Vec2::new(1.25, 1.25),
        };
        let transformed = TransformedAxes2DState::new(axes, transform, transform);
        let path = transformed_axes_line_graph_vector_path(transformed, &[1.0, 4.0], &[2.0, -3.0])
            .unwrap();

        let expected = [
            transform.transform_point(axes.coords_to_point(1.0, 2.0).unwrap()),
            transform.transform_point(axes.coords_to_point(4.0, -3.0).unwrap()),
        ];
        for (command, expected) in path.commands().iter().zip(expected) {
            match command {
                PathCommand::MoveTo { to } | PathCommand::LineTo { to } => {
                    assert_point(*to, expected)
                }
                other => panic!("expected corner path command, got {other:?}"),
            }
        }
    }

    #[test]
    fn empty_input_produces_empty_retained_path() {
        assert!(axes_line_graph_vector_path(axes(), &[], &[])
            .unwrap()
            .commands()
            .is_empty());
    }

    #[test]
    fn mismatched_and_nonfinite_inputs_fail_closed() {
        assert_eq!(
            axes_line_graph_vector_path(axes(), &[1.0], &[]).unwrap_err(),
            LineGraphAuthoringError::CoordinateCountMismatch {
                x_values: 1,
                y_values: 0,
            }
        );
        assert!(matches!(
            axes_line_graph_vector_path(axes(), &[1.0, f64::NAN], &[2.0, 3.0]),
            Err(LineGraphAuthoringError::NonFiniteCoordinate { index: 1, .. })
        ));
    }
}
