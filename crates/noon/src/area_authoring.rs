use crate::{CoordinateSystemError, IntoSnapshot, Polygon, TransformedAxes2DState};
use noon_core::{GeometryRef, ObjectSnapshot, PathCommand, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphParameterRange {
    t_min: f64,
    t_max: f64,
}

impl GraphParameterRange {
    pub fn new(t_min: f64, t_max: f64) -> Result<Self, AreaAuthoringError> {
        if !t_min.is_finite() || !t_max.is_finite() {
            return Err(AreaAuthoringError::NonFiniteGraphRange { t_min, t_max });
        }
        Ok(Self { t_min, t_max })
    }

    pub const fn t_min(self) -> f64 {
        self.t_min
    }

    pub const fn t_max(self) -> f64 {
        self.t_max
    }
}

/// Two-phase ManimCE v0.21 `Axes.get_area` geometry plan.
///
/// The retained graph owns the interior VMobject points, while the authored function
/// owns the exact endpoint values. Rust resolves/clips the x interval, filters the
/// current retained graph points through the current Axes transform, and asks the
/// frontend only for scalar callback values at the two endpoint x values. `finish`
/// then lowers the result to the same ordinary retained Polygon path used elsewhere.
#[derive(Clone, Debug, PartialEq)]
pub struct AreaSamplePlan {
    axes: TransformedAxes2DState,
    a: f64,
    b: f64,
    graph_points: Vec<Vec2>,
    bounded_graph_points: Option<Vec<Vec2>>,
}

impl AreaSamplePlan {
    pub fn new(
        axes: TransformedAxes2DState,
        graph: &ObjectSnapshot,
        graph_range: GraphParameterRange,
        x_range: Option<[f64; 2]>,
        bounded_graph: Option<(&ObjectSnapshot, GraphParameterRange)>,
    ) -> Result<Self, AreaAuthoringError> {
        let [mut a, mut b] = x_range.unwrap_or([graph_range.t_min(), graph_range.t_max()]);
        if !a.is_finite() || !b.is_finite() {
            return Err(AreaAuthoringError::NonFiniteAreaRange { a, b });
        }

        let bounded_graph_points = if let Some((bounded, bounded_range)) = bounded_graph {
            if bounded_range.t_min() > b {
                return Err(AreaAuthoringError::BoundedRangeStartsAfterArea {
                    bounded_min: bounded_range.t_min(),
                    area_max: b,
                });
            }
            if bounded_range.t_max() < a {
                return Err(AreaAuthoringError::BoundedRangeEndsBeforeArea {
                    bounded_max: bounded_range.t_max(),
                    area_min: a,
                });
            }
            a = a.max(bounded_range.t_min());
            b = b.min(bounded_range.t_max());
            Some(points_in_axis_x_range(axes, bounded, a, b)?)
        } else {
            None
        };

        let graph_points = points_in_axis_x_range(axes, graph, a, b)?;
        Ok(Self {
            axes,
            a,
            b,
            graph_points,
            bounded_graph_points,
        })
    }

    pub const fn x_range(&self) -> [f64; 2] {
        [self.a, self.b]
    }

    /// Endpoint x values at which the authored graph callback must be evaluated.
    pub const fn graph_endpoint_x_values(&self) -> [f64; 2] {
        [self.a, self.b]
    }

    /// Endpoint x values for an optional authored bounded-graph callback.
    pub fn bounded_graph_endpoint_x_values(&self) -> Option<[f64; 2]> {
        self.bounded_graph_points.as_ref().map(|_| [self.a, self.b])
    }

    pub fn graph_interior_points(&self) -> &[Vec2] {
        &self.graph_points
    }

    pub fn bounded_graph_interior_points(&self) -> Option<&[Vec2]> {
        self.bounded_graph_points.as_deref()
    }

    pub fn finish(
        &self,
        graph_endpoint_y_values: [f64; 2],
        bounded_graph_endpoint_y_values: Option<[f64; 2]>,
    ) -> Result<ObjectSnapshot, AreaAuthoringError> {
        validate_endpoint_values("graph", graph_endpoint_y_values)?;
        if let Some(values) = bounded_graph_endpoint_y_values {
            validate_endpoint_values("bounded graph", values)?;
        }

        let has_bounded_graph = self.bounded_graph_points.is_some();
        if has_bounded_graph && bounded_graph_endpoint_y_values.is_none() {
            return Err(AreaAuthoringError::MissingBoundedGraphEndpointValues);
        }
        if !has_bounded_graph && bounded_graph_endpoint_y_values.is_some() {
            return Err(AreaAuthoringError::UnexpectedBoundedGraphEndpointValues);
        }

        let [graph_a_y, graph_b_y] = graph_endpoint_y_values;
        let graph_a = self.axes.coords_to_point(self.a, graph_a_y)?;
        let graph_b = self.axes.coords_to_point(self.b, graph_b_y)?;
        let mut points = Vec::new();

        if let (Some(bounded_points), Some([bounded_a_y, bounded_b_y])) = (
            self.bounded_graph_points.as_ref(),
            bounded_graph_endpoint_y_values,
        ) {
            let bounded_a = self.axes.coords_to_point(self.a, bounded_a_y)?;
            let bounded_b = self.axes.coords_to_point(self.b, bounded_b_y)?;
            points
                .try_reserve_exact(self.graph_points.len() + bounded_points.len() + 4)
                .map_err(|_| {
                    AreaAuthoringError::PointAllocationFailed(
                        self.graph_points.len() + bounded_points.len() + 4,
                    )
                })?;
            points.push(graph_a);
            points.extend(self.graph_points.iter().copied());
            points.push(graph_b);
            points.push(bounded_b);
            points.extend(bounded_points.iter().rev().copied());
            points.push(bounded_a);
        } else {
            let baseline_y = self.axes.axes().y_axis().range().origin_shift();
            let baseline_a = self.axes.coords_to_point(self.a, baseline_y)?;
            let baseline_b = self.axes.coords_to_point(self.b, baseline_y)?;
            points
                .try_reserve_exact(self.graph_points.len() + 4)
                .map_err(|_| {
                    AreaAuthoringError::PointAllocationFailed(self.graph_points.len() + 4)
                })?;
            points.push(baseline_a);
            points.push(graph_a);
            points.extend(self.graph_points.iter().copied());
            points.push(graph_b);
            points.push(baseline_b);
        }

        Ok(Polygon::new(points).into_snapshot())
    }
}

fn validate_endpoint_values(
    source: &'static str,
    values: [f64; 2],
) -> Result<(), AreaAuthoringError> {
    for (index, value) in values.into_iter().enumerate() {
        if !value.is_finite() {
            return Err(AreaAuthoringError::NonFiniteEndpointValue {
                source,
                index,
                value,
            });
        }
    }
    Ok(())
}

fn points_in_axis_x_range(
    axes: TransformedAxes2DState,
    graph: &ObjectSnapshot,
    a: f64,
    b: f64,
) -> Result<Vec<Vec2>, AreaAuthoringError> {
    let mut result = Vec::new();
    for point in manim_vmobject_points(graph)? {
        let x = axes.point_to_coords(point)?.0;
        if a <= x && x <= b {
            result.push(point);
        }
    }
    Ok(result)
}

/// Reconstruct the point tuples that Manim stores for each cubic VMobject segment.
///
/// Noon's retained `VectorPath` keeps straight segments compact as `LineTo` and may
/// retain quadratic curves directly, whereas Manim's VMobject point array stores four
/// cubic points per segment. `get_area` consumes that raw point array as polygon
/// vertices, so this adapter expands only for that semantic read; persisted geometry
/// remains the canonical compact `VectorPath`.
fn manim_vmobject_points(graph: &ObjectSnapshot) -> Result<Vec<Vec2>, AreaAuthoringError> {
    let GeometryRef::VectorPath(path) = &graph.geometry else {
        return Err(AreaAuthoringError::UnsupportedGraphGeometry);
    };
    let mut points = Vec::new();
    let mut current = None;
    let mut subpath_start = None;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                current = Some(to);
                subpath_start = Some(to);
            }
            PathCommand::LineTo { to } => {
                let start = current.ok_or(AreaAuthoringError::DrawingBeforeMove)?;
                append_cubic_tuple(
                    &mut points,
                    start,
                    lerp(start, to, 1.0 / 3.0),
                    lerp(start, to, 2.0 / 3.0),
                    to,
                );
                current = Some(to);
            }
            PathCommand::QuadraticTo { control, to } => {
                let start = current.ok_or(AreaAuthoringError::DrawingBeforeMove)?;
                let control1 = start + (control - start) * (2.0 / 3.0);
                let control2 = to + (control - to) * (2.0 / 3.0);
                append_cubic_tuple(&mut points, start, control1, control2, to);
                current = Some(to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let start = current.ok_or(AreaAuthoringError::DrawingBeforeMove)?;
                append_cubic_tuple(&mut points, start, control1, control2, to);
                current = Some(to);
            }
            PathCommand::Close => {
                let start = current.ok_or(AreaAuthoringError::DrawingBeforeMove)?;
                let to = subpath_start.ok_or(AreaAuthoringError::DrawingBeforeMove)?;
                append_cubic_tuple(
                    &mut points,
                    start,
                    lerp(start, to, 1.0 / 3.0),
                    lerp(start, to, 2.0 / 3.0),
                    to,
                );
                current = Some(to);
            }
        }
    }

    for point in &mut points {
        *point = graph.transform.transform_point(*point);
    }
    Ok(points)
}

fn append_cubic_tuple(
    points: &mut Vec<Vec2>,
    start: Vec2,
    control1: Vec2,
    control2: Vec2,
    end: Vec2,
) {
    points.extend([start, control1, control2, end]);
}

fn lerp(from: Vec2, to: Vec2, t: f32) -> Vec2 {
    from + (to - from) * t
}

#[derive(Clone, Debug, PartialEq)]
pub enum AreaAuthoringError {
    NonFiniteGraphRange {
        t_min: f64,
        t_max: f64,
    },
    NonFiniteAreaRange {
        a: f64,
        b: f64,
    },
    BoundedRangeStartsAfterArea {
        bounded_min: f64,
        area_max: f64,
    },
    BoundedRangeEndsBeforeArea {
        bounded_max: f64,
        area_min: f64,
    },
    UnsupportedGraphGeometry,
    DrawingBeforeMove,
    NonFiniteEndpointValue {
        source: &'static str,
        index: usize,
        value: f64,
    },
    MissingBoundedGraphEndpointValues,
    UnexpectedBoundedGraphEndpointValues,
    PointAllocationFailed(usize),
    Coordinates(CoordinateSystemError),
}

impl std::fmt::Display for AreaAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteGraphRange { t_min, t_max } => {
                write!(formatter, "graph range must be finite: [{t_min}, {t_max}]")
            }
            Self::NonFiniteAreaRange { a, b } => {
                write!(formatter, "area x_range must be finite: [{a}, {b}]")
            }
            Self::BoundedRangeStartsAfterArea {
                bounded_min,
                area_max,
            } => write!(formatter, "Ranges not matching: {bounded_min} < {area_max}"),
            Self::BoundedRangeEndsBeforeArea {
                bounded_max,
                area_min,
            } => write!(formatter, "Ranges not matching: {bounded_max} > {area_min}"),
            Self::UnsupportedGraphGeometry => {
                formatter.write_str("Axes.get_area requires retained VectorPath graph geometry")
            }
            Self::DrawingBeforeMove => {
                formatter.write_str("graph path draws before its first MoveTo")
            }
            Self::NonFiniteEndpointValue {
                source,
                index,
                value,
            } => write!(
                formatter,
                "area {source} endpoint value {index} must be finite: {value}"
            ),
            Self::MissingBoundedGraphEndpointValues => {
                formatter.write_str("bounded graph endpoint values are required")
            }
            Self::UnexpectedBoundedGraphEndpointValues => {
                formatter.write_str("bounded graph endpoint values were not requested")
            }
            Self::PointAllocationFailed(count) => {
                write!(
                    formatter,
                    "area point allocation failed for {count} vertices"
                )
            }
            Self::Coordinates(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for AreaAuthoringError {}

impl From<CoordinateSystemError> for AreaAuthoringError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        transformed_axes_line_graph_vector_path, Axes2DState, IntoSnapshot, NumberRange, Path,
    };
    use noon_core::{Transform2D, VectorPath};

    fn graph_snapshot(axes: TransformedAxes2DState, xs: &[f64], ys: &[f64]) -> ObjectSnapshot {
        Path::new(transformed_axes_line_graph_vector_path(axes, xs, ys).unwrap()).into_snapshot()
    }

    fn axes(transform: Transform2D) -> TransformedAxes2DState {
        let axes = Axes2DState::new(
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
            4.0,
            4.0,
        )
        .unwrap();
        TransformedAxes2DState::new(axes, transform, transform)
    }

    fn path_points(snapshot: &ObjectSnapshot) -> Vec<Vec2> {
        let GeometryRef::VectorPath(path) = &snapshot.geometry else {
            panic!("area must lower to retained VectorPath geometry")
        };
        path.commands()
            .iter()
            .filter_map(|command| match command {
                PathCommand::MoveTo { to } | PathCommand::LineTo { to } => Some(*to),
                PathCommand::QuadraticTo { to, .. } | PathCommand::CubicTo { to, .. } => Some(*to),
                PathCommand::Close => None,
            })
            .collect()
    }

    fn assert_point(actual: Vec2, expected: Vec2) {
        assert!(
            (actual - expected).length() <= 1.0e-5,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn unbounded_area_uses_callback_endpoints_and_retained_interior_points() {
        let axes = axes(Transform2D::IDENTITY);
        let graph = graph_snapshot(axes, &[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
        let plan = AreaSamplePlan::new(
            axes,
            &graph,
            GraphParameterRange::new(0.0, 2.0).unwrap(),
            Some([0.5, 1.5]),
            None,
        )
        .unwrap();
        assert_eq!(plan.graph_endpoint_x_values(), [0.5, 1.5]);
        assert!(!plan.graph_interior_points().is_empty());

        let snapshot = plan.finish([0.5, 0.5], None).unwrap();
        let points = path_points(&snapshot);
        assert_point(points[0], axes.coords_to_point(0.5, 0.0).unwrap());
        assert_point(points[1], axes.coords_to_point(0.5, 0.5).unwrap());
        assert_point(
            *points.last().unwrap(),
            axes.coords_to_point(1.5, 0.0).unwrap(),
        );
    }

    #[test]
    fn bounded_area_clips_to_overlap_and_traces_second_graph_in_reverse() {
        let axes = axes(Transform2D::IDENTITY);
        let graph = graph_snapshot(
            axes,
            &[-1.0, 0.0, 1.0, 2.0],
            &[0.0, 1.0, 1.0, 0.0],
        );
        let bounded = graph_snapshot(
            axes,
            &[0.0, 1.0, 2.0, 3.0],
            &[-1.0, -1.0, -1.0, -1.0],
        );
        let plan = AreaSamplePlan::new(
            axes,
            &graph,
            GraphParameterRange::new(-1.0, 2.0).unwrap(),
            Some([-0.5, 2.5]),
            Some((&bounded, GraphParameterRange::new(0.0, 3.0).unwrap())),
        )
        .unwrap();
        assert_eq!(plan.x_range(), [0.0, 2.5]);
        assert_eq!(plan.bounded_graph_endpoint_x_values(), Some([0.0, 2.5]));

        let snapshot = plan.finish([1.0, 0.0], Some([-1.0, -1.0])).unwrap();
        let points = path_points(&snapshot);
        let bounded_start_index = 1 + plan.graph_interior_points().len() + 1;
        assert_point(
            points[bounded_start_index],
            axes.coords_to_point(2.5, -1.0).unwrap(),
        );
        assert_point(
            *points.last().unwrap(),
            axes.coords_to_point(0.0, -1.0).unwrap(),
        );
    }

    #[test]
    fn non_overlapping_bounded_ranges_match_upstream_errors() {
        let axes = axes(Transform2D::IDENTITY);
        let graph = graph_snapshot(axes, &[0.0, 1.0], &[0.0, 1.0]);
        let bounded = graph_snapshot(axes, &[2.0, 3.0], &[0.0, 1.0]);
        let error = AreaSamplePlan::new(
            axes,
            &graph,
            GraphParameterRange::new(0.0, 1.0).unwrap(),
            None,
            Some((&bounded, GraphParameterRange::new(2.0, 3.0).unwrap())),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Ranges not matching: 2 < 1");
    }

    #[test]
    fn retained_graph_transform_drives_interior_filtering() {
        let axes = axes(Transform2D::IDENTITY);
        let mut graph = graph_snapshot(axes, &[-1.0, 0.0, 1.0], &[0.0, 0.0, 0.0]);
        graph.transform.translation = Vec2::new(1.0, 0.0);
        let plan = AreaSamplePlan::new(
            axes,
            &graph,
            GraphParameterRange::new(-1.0, 1.0).unwrap(),
            Some([0.75, 1.25]),
            None,
        )
        .unwrap();
        assert!(!plan.graph_interior_points().is_empty());
        for point in plan.graph_interior_points() {
            let x = axes.point_to_coords(*point).unwrap().0;
            assert!((0.75..=1.25).contains(&x));
        }
    }

    #[test]
    fn compact_line_segments_expand_to_manim_cubic_point_tuples() {
        let mut snapshot = Path::new(
            VectorPath::new()
                .move_to(Vec2::new(0.0, 0.0))
                .line_to(Vec2::new(3.0, 0.0)),
        )
        .into_snapshot();
        snapshot.transform.translation = Vec2::new(1.0, 2.0);
        let points = manim_vmobject_points(&snapshot).unwrap();
        assert_eq!(points.len(), 4);
        for (actual, expected) in points.into_iter().zip([
            Vec2::new(1.0, 2.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(3.0, 2.0),
            Vec2::new(4.0, 2.0),
        ]) {
            assert_point(actual, expected);
        }
    }
}
