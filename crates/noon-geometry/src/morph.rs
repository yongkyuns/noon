use noon_core::{PathCommand, Vec2, VectorPath};

use crate::GeometryError;

const MAX_FLATTEN_DEPTH: u32 = 16;
const DEGENERATE_LENGTH_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphOptions {
    pub samples_per_contour: usize,
    pub flatten_tolerance: f32,
}

impl MorphOptions {
    pub const DEFAULT: Self = Self {
        samples_per_contour: 64,
        flatten_tolerance: 0.01,
    };
}

impl Default for MorphOptions {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MorphContourPlan {
    pub source_points: Vec<Vec2>,
    pub target_points: Vec<Vec2>,
    pub closed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MorphPlan {
    pub contours: Vec<MorphContourPlan>,
}

impl MorphPlan {
    pub fn point_count(&self) -> usize {
        self.contours
            .iter()
            .map(|contour| contour.source_points.len())
            .sum()
    }

    pub fn interpolate(&self, progress: f32) -> MorphFrame {
        let progress = if progress.is_finite() {
            progress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        MorphFrame {
            contours: self
                .contours
                .iter()
                .map(|contour| MorphFrameContour {
                    points: contour
                        .source_points
                        .iter()
                        .zip(&contour.target_points)
                        .map(|(source, target)| lerp_vec2(*source, *target, progress))
                        .collect(),
                    closed: contour.closed,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MorphFrameContour {
    pub points: Vec<Vec2>,
    pub closed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MorphFrame {
    pub contours: Vec<MorphFrameContour>,
}

impl MorphFrame {
    pub fn to_vector_path(&self) -> VectorPath {
        let mut path = VectorPath::new();
        for contour in &self.contours {
            let Some((&first, rest)) = contour.points.split_first() else {
                continue;
            };
            path = path.move_to(first);
            for &point in rest {
                path = path.line_to(point);
            }
            if contour.closed {
                path = path.close();
            }
        }
        path
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MorphError {
    Geometry(GeometryError),
    InvalidSampleCount(usize),
    InvalidFlattenTolerance(f32),
    ContourCountMismatch {
        source: usize,
        target: usize,
    },
    ClosureMismatch {
        contour: usize,
        source_closed: bool,
        target_closed: bool,
    },
    DegenerateContour {
        contour: usize,
        side: MorphSide,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MorphSide {
    Source,
    Target,
}

impl std::fmt::Display for MorphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Geometry(error) => write!(formatter, "invalid morph path: {error}"),
            Self::InvalidSampleCount(count) => write!(
                formatter,
                "morph samples per contour must be at least 3, got {count}"
            ),
            Self::InvalidFlattenTolerance(tolerance) => write!(
                formatter,
                "morph flatten tolerance must be finite and positive, got {tolerance}"
            ),
            Self::ContourCountMismatch { source, target } => write!(
                formatter,
                "morph contour count mismatch: source has {source}, target has {target}"
            ),
            Self::ClosureMismatch {
                contour,
                source_closed,
                target_closed,
            } => write!(
                formatter,
                "morph contour {contour} closure mismatch: source closed={source_closed}, target closed={target_closed}"
            ),
            Self::DegenerateContour { contour, side } => {
                write!(formatter, "morph {side:?} contour {contour} has zero length")
            }
        }
    }
}

impl std::error::Error for MorphError {}

impl From<GeometryError> for MorphError {
    fn from(value: GeometryError) -> Self {
        Self::Geometry(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FlattenedContour {
    points: Vec<Vec2>,
    closed: bool,
}

pub fn plan_morph(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
) -> Result<MorphPlan, MorphError> {
    validate_options(options)?;
    let source_contours = flatten_path(source, options.flatten_tolerance)?;
    let target_contours = flatten_path(target, options.flatten_tolerance)?;
    if source_contours.len() != target_contours.len() {
        return Err(MorphError::ContourCountMismatch {
            source: source_contours.len(),
            target: target_contours.len(),
        });
    }

    let mut contours = Vec::with_capacity(source_contours.len());
    for (index, (source, target)) in source_contours.into_iter().zip(target_contours).enumerate() {
        if source.closed != target.closed {
            return Err(MorphError::ClosureMismatch {
                contour: index,
                source_closed: source.closed,
                target_closed: target.closed,
            });
        }
        let source_points = resample_contour(
            &source,
            options.samples_per_contour,
            index,
            MorphSide::Source,
        )?;
        let mut target_points = resample_contour(
            &target,
            options.samples_per_contour,
            index,
            MorphSide::Target,
        )?;
        if source.closed {
            target_points = align_closed_contour(&source_points, &target_points);
        }
        contours.push(MorphContourPlan {
            source_points,
            target_points,
            closed: source.closed,
        });
    }
    Ok(MorphPlan { contours })
}

fn validate_options(options: MorphOptions) -> Result<(), MorphError> {
    if options.samples_per_contour < 3 {
        return Err(MorphError::InvalidSampleCount(options.samples_per_contour));
    }
    if !options.flatten_tolerance.is_finite() || options.flatten_tolerance <= 0.0 {
        return Err(MorphError::InvalidFlattenTolerance(
            options.flatten_tolerance,
        ));
    }
    Ok(())
}

fn flatten_path(path: &VectorPath, tolerance: f32) -> Result<Vec<FlattenedContour>, GeometryError> {
    let mut contours = Vec::new();
    let mut points = Vec::new();
    let mut current = Vec2::ZERO;
    let mut start = Vec2::ZERO;
    let mut active = false;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                finite(to)?;
                if active {
                    contours.push(FlattenedContour {
                        points: std::mem::take(&mut points),
                        closed: false,
                    });
                }
                points.push(to);
                current = to;
                start = to;
                active = true;
            }
            PathCommand::LineTo { to } => {
                require_active(active)?;
                finite(to)?;
                push_distinct(&mut points, to);
                current = to;
            }
            PathCommand::QuadraticTo { control, to } => {
                require_active(active)?;
                finite(control)?;
                finite(to)?;
                flatten_quadratic(current, control, to, tolerance, 0, &mut points);
                current = to;
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                require_active(active)?;
                finite(control1)?;
                finite(control2)?;
                finite(to)?;
                flatten_cubic(current, control1, control2, to, tolerance, 0, &mut points);
                current = to;
            }
            PathCommand::Close => {
                if !active {
                    return Err(GeometryError::CloseBeforeMove);
                }
                if points.last().copied() == Some(start) {
                    points.pop();
                }
                contours.push(FlattenedContour {
                    points: std::mem::take(&mut points),
                    closed: true,
                });
                current = start;
                active = false;
            }
        }
    }
    if active {
        contours.push(FlattenedContour {
            points,
            closed: false,
        });
    }
    Ok(contours)
}

fn flatten_quadratic(
    from: Vec2,
    control: Vec2,
    to: Vec2,
    tolerance: f32,
    depth: u32,
    points: &mut Vec<Vec2>,
) {
    if depth >= MAX_FLATTEN_DEPTH || point_line_distance(control, from, to) <= tolerance {
        push_distinct(points, to);
        return;
    }
    let from_control = midpoint(from, control);
    let control_to = midpoint(control, to);
    let middle = midpoint(from_control, control_to);
    flatten_quadratic(from, from_control, middle, tolerance, depth + 1, points);
    flatten_quadratic(middle, control_to, to, tolerance, depth + 1, points);
}

#[allow(clippy::too_many_arguments)]
fn flatten_cubic(
    from: Vec2,
    control1: Vec2,
    control2: Vec2,
    to: Vec2,
    tolerance: f32,
    depth: u32,
    points: &mut Vec<Vec2>,
) {
    let flatness =
        point_line_distance(control1, from, to).max(point_line_distance(control2, from, to));
    if depth >= MAX_FLATTEN_DEPTH || flatness <= tolerance {
        push_distinct(points, to);
        return;
    }
    let a = midpoint(from, control1);
    let b = midpoint(control1, control2);
    let c = midpoint(control2, to);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let middle = midpoint(d, e);
    flatten_cubic(from, a, d, middle, tolerance, depth + 1, points);
    flatten_cubic(middle, e, c, to, tolerance, depth + 1, points);
}

fn resample_contour(
    contour: &FlattenedContour,
    sample_count: usize,
    contour_index: usize,
    side: MorphSide,
) -> Result<Vec<Vec2>, MorphError> {
    if contour.points.len() < 2 {
        return Err(MorphError::DegenerateContour {
            contour: contour_index,
            side,
        });
    }
    let segment_count = if contour.closed {
        contour.points.len()
    } else {
        contour.points.len() - 1
    };
    let mut cumulative = Vec::with_capacity(segment_count + 1);
    cumulative.push(0.0_f32);
    let mut total = 0.0_f32;
    for segment in 0..segment_count {
        let next = if segment + 1 == contour.points.len() {
            0
        } else {
            segment + 1
        };
        total += distance(contour.points[segment], contour.points[next]);
        cumulative.push(total);
    }
    if total <= DEGENERATE_LENGTH_EPSILON {
        return Err(MorphError::DegenerateContour {
            contour: contour_index,
            side,
        });
    }
    let denominator = if contour.closed {
        sample_count as f32
    } else {
        (sample_count - 1) as f32
    };
    Ok((0..sample_count)
        .map(|sample| {
            let target_distance = total * sample as f32 / denominator;
            sample_polyline(contour, &cumulative, target_distance)
        })
        .collect())
}

fn sample_polyline(contour: &FlattenedContour, cumulative: &[f32], distance: f32) -> Vec2 {
    let segment_count = cumulative.len() - 1;
    if !contour.closed && distance >= cumulative[segment_count] {
        return *contour.points.last().expect("validated contour");
    }
    let segment = cumulative
        .partition_point(|candidate| *candidate <= distance)
        .saturating_sub(1)
        .min(segment_count - 1);
    let start_distance = cumulative[segment];
    let end_distance = cumulative[segment + 1];
    let progress = if end_distance > start_distance {
        (distance - start_distance) / (end_distance - start_distance)
    } else {
        0.0
    };
    let next = if segment + 1 == contour.points.len() {
        0
    } else {
        segment + 1
    };
    lerp_vec2(contour.points[segment], contour.points[next], progress)
}

fn align_closed_contour(source: &[Vec2], target: &[Vec2]) -> Vec<Vec2> {
    debug_assert_eq!(source.len(), target.len());
    let count = source.len();
    let mut best_cost = f64::INFINITY;
    let mut best_reversed = false;
    let mut best_shift = 0;
    for reversed in [false, true] {
        for shift in 0..count {
            let cost = (0..count)
                .map(|index| {
                    let target_index = correspondence_index(index, shift, count, reversed);
                    squared_distance(source[index], target[target_index]) as f64
                })
                .sum::<f64>();
            if cost < best_cost {
                best_cost = cost;
                best_reversed = reversed;
                best_shift = shift;
            }
        }
    }
    (0..count)
        .map(|index| target[correspondence_index(index, best_shift, count, best_reversed)])
        .collect()
}

fn correspondence_index(index: usize, shift: usize, count: usize, reversed: bool) -> usize {
    if reversed {
        (shift + count - index % count) % count
    } else {
        (index + shift) % count
    }
}

fn push_distinct(points: &mut Vec<Vec2>, point: Vec2) {
    if points.last().copied() != Some(point) {
        points.push(point);
    }
}

fn midpoint(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

fn point_line_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f32::EPSILON {
        return distance(point, start);
    }
    let cross = ((point.x - start.x) * dy - (point.y - start.y) * dx).abs();
    cross / length_squared.sqrt()
}

fn distance(a: Vec2, b: Vec2) -> f32 {
    squared_distance(a, b).sqrt()
}

fn squared_distance(a: Vec2, b: Vec2) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fn lerp_vec2(from: Vec2, to: Vec2, progress: f32) -> Vec2 {
    Vec2::new(
        from.x + (to.x - from.x) * progress,
        from.y + (to.y - from.y) * progress,
    )
}

fn finite(value: Vec2) -> Result<(), GeometryError> {
    if value.x.is_finite() && value.y.is_finite() {
        Ok(())
    } else {
        Err(GeometryError::NonFinitePoint)
    }
}

fn require_active(active: bool) -> Result<(), GeometryError> {
    if active {
        Ok(())
    } else {
        Err(GeometryError::DrawingBeforeMove)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_from(start: usize, reversed: bool) -> VectorPath {
        let corners = [
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
        ];
        let order: Vec<usize> = if reversed {
            (0..4).map(|offset| (start + 4 - offset) % 4).collect()
        } else {
            (0..4).map(|offset| (start + offset) % 4).collect()
        };
        let mut path = VectorPath::new().move_to(corners[order[0]]);
        for &index in &order[1..] {
            path = path.line_to(corners[index]);
        }
        path.close()
    }

    #[test]
    fn open_paths_resample_to_equal_deterministic_correspondence() {
        let source = VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .line_to(Vec2::new(4.0, 0.0));
        let target = VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .quadratic_to(Vec2::new(2.0, 3.0), Vec2::new(4.0, 0.0));
        let options = MorphOptions {
            samples_per_contour: 9,
            ..MorphOptions::DEFAULT
        };
        let first = plan_morph(&source, &target, options).expect("compatible paths");
        let second = plan_morph(&source, &target, options).expect("deterministic plan");
        assert_eq!(first, second);
        assert_eq!(first.contours.len(), 1);
        assert_eq!(first.point_count(), 9);
        assert_eq!(first.contours[0].source_points[0], Vec2::new(0.0, 0.0));
        assert_eq!(first.contours[0].source_points[8], Vec2::new(4.0, 0.0));
        assert_eq!(first.contours[0].target_points[0], Vec2::new(0.0, 0.0));
        assert_eq!(first.contours[0].target_points[8], Vec2::new(4.0, 0.0));
    }

    #[test]
    fn interpolation_has_exact_planned_endpoints_and_midpoints() {
        let source = VectorPath::new()
            .move_to(Vec2::new(-2.0, 0.0))
            .line_to(Vec2::new(2.0, 0.0));
        let target = VectorPath::new()
            .move_to(Vec2::new(-2.0, 2.0))
            .line_to(Vec2::new(2.0, 2.0));
        let plan = plan_morph(
            &source,
            &target,
            MorphOptions {
                samples_per_contour: 5,
                ..MorphOptions::DEFAULT
            },
        )
        .expect("compatible paths");
        assert_eq!(
            plan.interpolate(0.0).contours[0].points,
            plan.contours[0].source_points
        );
        assert_eq!(
            plan.interpolate(1.0).contours[0].points,
            plan.contours[0].target_points
        );
        assert!(plan.interpolate(0.5).contours[0]
            .points
            .iter()
            .all(|point| (point.y - 1.0).abs() < 1e-6));
    }

    #[test]
    fn closed_contours_ignore_start_vertex_and_winding_when_matching() {
        let source = square_from(0, false);
        for target in [square_from(2, false), square_from(1, true)] {
            let plan = plan_morph(
                &source,
                &target,
                MorphOptions {
                    samples_per_contour: 16,
                    ..MorphOptions::DEFAULT
                },
            )
            .expect("equivalent closed contour");
            let displacement: f32 = plan.contours[0]
                .source_points
                .iter()
                .zip(&plan.contours[0].target_points)
                .map(|(source, target)| squared_distance(*source, *target))
                .sum();
            assert!(displacement < 1e-5);
        }
    }

    #[test]
    fn incompatible_topology_is_rejected_before_runtime() {
        let open = VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE);
        let closed = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0))
            .line_to(Vec2::ONE)
            .close();
        assert!(matches!(
            plan_morph(&open, &closed, MorphOptions::DEFAULT),
            Err(MorphError::ClosureMismatch { .. })
        ));
        let two_contours = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::ONE)
            .move_to(Vec2::new(2.0, 0.0))
            .line_to(Vec2::new(3.0, 0.0));
        assert_eq!(
            plan_morph(&open, &two_contours, MorphOptions::DEFAULT),
            Err(MorphError::ContourCountMismatch {
                source: 1,
                target: 2,
            })
        );
    }

    #[test]
    fn degenerate_contours_and_invalid_options_are_rejected() {
        let point = VectorPath::new().move_to(Vec2::ZERO);
        let line = VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE);
        assert_eq!(
            plan_morph(&point, &line, MorphOptions::DEFAULT),
            Err(MorphError::DegenerateContour {
                contour: 0,
                side: MorphSide::Source,
            })
        );
        assert_eq!(
            plan_morph(
                &line,
                &line,
                MorphOptions {
                    samples_per_contour: 2,
                    ..MorphOptions::DEFAULT
                }
            ),
            Err(MorphError::InvalidSampleCount(2))
        );
    }

    #[test]
    fn multi_contour_plans_keep_contours_independent() {
        let source = VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .line_to(Vec2::new(2.0, 0.0))
            .move_to(Vec2::new(10.0, 0.0))
            .line_to(Vec2::new(10.0, 3.0));
        let target = VectorPath::new()
            .move_to(Vec2::new(0.0, 2.0))
            .line_to(Vec2::new(2.0, 2.0))
            .move_to(Vec2::new(12.0, 0.0))
            .line_to(Vec2::new(12.0, 3.0));
        let plan = plan_morph(
            &source,
            &target,
            MorphOptions {
                samples_per_contour: 8,
                ..MorphOptions::DEFAULT
            },
        )
        .expect("compatible multi-contour paths");
        assert_eq!(plan.contours.len(), 2);
        assert_eq!(plan.point_count(), 16);
        assert!(plan.contours.iter().all(|contour| !contour.closed));
    }
}
