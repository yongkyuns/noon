use crate::{CoordinateSystemError, TransformedAxes2DState};
use noon_core::{ObjectSnapshot, Vec2};
use noon_geometry::{point_from_geometry_proportion, GeometryProportionError};

pub const MANIM_GRAPH_X_SEARCH_TOLERANCE: f64 = 1.0e-4;

/// Resolve a scene-space point on retained path-like graph geometry for an Axes x value.
///
/// This is the shared ManimCE v0.21 generic `input_to_graph_point` fallback. The
/// entire proportion search stays in Rust so frontends do not loop across a host/WASM
/// boundary. Both the graph's current retained transform and the current Axes transforms
/// participate in every candidate evaluation.
pub fn transformed_graph_point_for_x(
    axes: TransformedAxes2DState,
    graph: &ObjectSnapshot,
    target: f64,
) -> Result<Vec2, GraphQueryError> {
    if !target.is_finite() {
        return Err(GraphQueryError::NonFiniteGraphX(target));
    }

    if let Some(alpha) = binary_search_graph_x(axes, graph, target)? {
        return graph_point_from_proportion(graph, alpha);
    }

    let start = graph_point_from_proportion(graph, 0.0)?;
    let end = graph_point_from_proportion(graph, 1.0)?;
    let start_x = axes.point_to_coords(start)?.0;
    let end_x = axes.point_to_coords(end)?.0;
    Err(GraphQueryError::GraphXOutOfRange {
        x: target,
        start: start_x,
        end: end_x,
    })
}

pub fn transformed_graph_point_from_proportion(
    graph: &ObjectSnapshot,
    alpha: f64,
) -> Result<Vec2, GraphQueryError> {
    graph_point_from_proportion(graph, alpha)
}

fn graph_point_from_proportion(
    graph: &ObjectSnapshot,
    alpha: f64,
) -> Result<Vec2, GraphQueryError> {
    let local = point_from_geometry_proportion(&graph.geometry, alpha as f32)?;
    Ok(graph.transform.transform_point(local))
}

fn graph_x_at_proportion(
    axes: TransformedAxes2DState,
    graph: &ObjectSnapshot,
    alpha: f64,
) -> Result<f64, GraphQueryError> {
    Ok(axes
        .point_to_coords(graph_point_from_proportion(graph, alpha)?)?
        .0)
}

/// Exact control flow of ManimCE v0.21 `binary_search` for graph-x lookup.
fn binary_search_graph_x(
    axes: TransformedAxes2DState,
    graph: &ObjectSnapshot,
    target: f64,
) -> Result<Option<f64>, GraphQueryError> {
    let mut left: f64 = 0.0;
    let mut right: f64 = 1.0;
    let mut middle: f64 = 0.5;
    while (right - left).abs() > MANIM_GRAPH_X_SEARCH_TOLERANCE {
        middle = (left + right) * 0.5;
        let left_x = graph_x_at_proportion(axes, graph, left)?;
        let middle_x = graph_x_at_proportion(axes, graph, middle)?;
        let right_x = graph_x_at_proportion(axes, graph, right)?;
        if left_x == target {
            return Ok(Some(left));
        }
        if right_x == target {
            return Ok(Some(right));
        }

        if left_x <= target && target <= right_x {
            if middle_x > target {
                right = middle;
            } else {
                left = middle;
            }
        } else if left_x > target && target > right_x {
            std::mem::swap(&mut left, &mut right);
        } else {
            return Ok(None);
        }
    }
    Ok(Some(middle))
}

#[derive(Clone, Debug, PartialEq)]
pub enum GraphQueryError {
    NonFiniteGraphX(f64),
    GraphXOutOfRange { x: f64, start: f64, end: f64 },
    Coordinates(CoordinateSystemError),
    GeometryProportion(GeometryProportionError),
}

impl std::fmt::Display for GraphQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteGraphX(x) => {
                write!(formatter, "graph x lookup requires a finite value: {x}")
            }
            Self::GraphXOutOfRange { x, start, end } => write!(
                formatter,
                "x={x} not located in the range of the graph ([{start}, {end}])"
            ),
            Self::Coordinates(error) => error.fmt(formatter),
            Self::GeometryProportion(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for GraphQueryError {}

impl From<CoordinateSystemError> for GraphQueryError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl From<GeometryProportionError> for GraphQueryError {
    fn from(value: GeometryProportionError) -> Self {
        Self::GeometryProportion(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        transformed_axes_line_graph_vector_path, Axes2DState, IntoSnapshot, NumberRange, Path,
    };
    use noon_core::{GeometryRef, Transform2D};

    fn axes() -> TransformedAxes2DState {
        let axes = Axes2DState::new(
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            4.0,
            4.0,
        )
        .unwrap();
        TransformedAxes2DState::new(axes, Transform2D::IDENTITY, Transform2D::IDENTITY)
    }

    fn graph(axes: TransformedAxes2DState, xs: &[f64], ys: &[f64]) -> ObjectSnapshot {
        Path::new(transformed_axes_line_graph_vector_path(axes, xs, ys).unwrap()).into_snapshot()
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= MANIM_GRAPH_X_SEARCH_TOLERANCE,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn ascending_and_descending_paths_follow_manim_binary_search() {
        let axes = axes();
        for xs in [&[-1.0, 0.0, 1.0][..], &[1.0, 0.0, -1.0][..]] {
            let graph = graph(axes, xs, &[1.0, 0.0, -1.0]);
            let point = transformed_graph_point_for_x(axes, &graph, 0.5).unwrap();
            let coords = axes.point_to_coords(point).unwrap();
            assert_close(coords.0, 0.5);
        }
    }

    #[test]
    fn current_graph_and_axes_transforms_participate() {
        let base = Axes2DState::new(
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            4.0,
            4.0,
        )
        .unwrap();
        let transform = Transform2D {
            translation: Vec2::new(2.0, -1.0),
            rotation: 0.2,
            scale: Vec2::new(1.1, 1.1),
        };
        let axes = TransformedAxes2DState::new(base, transform, transform);
        let mut graph = graph(axes, &[-1.0, 0.0, 1.0], &[0.0, 0.0, 0.0]);
        graph.transform.translation = Vec2::new(0.2, 0.0);

        let point = transformed_graph_point_for_x(axes, &graph, 0.5).unwrap();
        assert_close(axes.point_to_coords(point).unwrap().0, 0.5);
    }

    #[test]
    fn out_of_range_and_non_path_geometry_fail_closed() {
        let axes = axes();
        let graph = graph(axes, &[-1.0, 0.0, 1.0], &[0.0, 0.0, 0.0]);
        assert!(matches!(
            transformed_graph_point_for_x(axes, &graph, 3.0),
            Err(GraphQueryError::GraphXOutOfRange { .. })
        ));

        let rectangle = ObjectSnapshot::new(GeometryRef::rectangle(1.0, 1.0));
        assert!(matches!(
            transformed_graph_point_for_x(axes, &rectangle, 0.0),
            Err(GraphQueryError::GeometryProportion(
                GeometryProportionError::UnsupportedGeometry
            ))
        ));
    }
}
