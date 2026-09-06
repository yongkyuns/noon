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
    feature_indices: Vec<usize>,
    closed: bool,
}

pub fn plan_morph(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
) -> Result<MorphPlan, MorphError> {
    plan_morph_impl(source, target, options, true)
}

/// Plan a morph while preserving the authored contour start point and winding.
///
/// ManimCE Transform aligns VMobject cubic arrays by index and does not cyclically
/// rotate a closed contour to minimize geometric distance. This mode exists for
/// the compatibility renderer; native Noon morph planning keeps geometric alignment.
pub fn plan_morph_preserving_order(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
) -> Result<MorphPlan, MorphError> {
    plan_morph_impl(source, target, options, false)
}

fn plan_morph_impl(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
    align_closed_correspondence: bool,
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
        if source.closed && align_closed_correspondence {
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
    let mut feature_indices = Vec::new();
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
                        feature_indices: std::mem::take(&mut feature_indices),
                        closed: false,
                    });
                }
                points.push(to);
                feature_indices.push(0);
                current = to;
                start = to;
                active = true;
            }
            PathCommand::LineTo { to } => {
                require_active(active)?;
                finite(to)?;
                push_distinct(&mut points, to);
                mark_feature(&mut feature_indices, points.len() - 1);
                current = to;
            }
            PathCommand::QuadraticTo { control, to } => {
                require_active(active)?;
                finite(control)?;
                finite(to)?;
                flatten_quadratic(current, control, to, tolerance, 0, &mut points);
                mark_feature(&mut feature_indices, points.len() - 1);
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
                mark_feature(&mut feature_indices, points.len() - 1);
                current = to;
            }
            PathCommand::Close => {
                if !active {
                    return Err(GeometryError::CloseBeforeMove);
                }
                if points.last().copied() == Some(start) {
                    let removed = points.len() - 1;
                    points.pop();
                    feature_indices.retain(|index| *index != removed);
                }
                contours.push(FlattenedContour {
                    points: std::mem::take(&mut points),
                    feature_indices: std::mem::take(&mut feature_indices),
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
            feature_indices,
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

    let feature_distances: Vec<f32> = contour
        .feature_indices
        .iter()
        .copied()
        .filter(|index| *index < contour.points.len())
        .map(|index| cumulative[index])
        .collect();
    let interval_count = if contour.closed {
        feature_distances.len()
    } else {
        feature_distances.len().saturating_sub(1)
    };
    let segment_budget = if contour.closed {
        sample_count
    } else {
        sample_count.saturating_sub(1)
    };

    if interval_count == 0 || segment_budget < interval_count {
        let denominator = if contour.closed {
            sample_count as f32
        } else {
            (sample_count - 1) as f32
        };
        return Ok((0..sample_count)
            .map(|sample| {
                let target_distance = total * sample as f32 / denominator;
                sample_polyline(contour, &cumulative, target_distance)
            })
            .collect());
    }

    let mut interval_lengths = Vec::with_capacity(interval_count);
    for interval in 0..interval_count {
        let start = feature_distances[interval];
        let end = if interval + 1 < feature_distances.len() {
            feature_distances[interval + 1]
        } else {
            total
        };
        interval_lengths.push((end - start).max(0.0));
    }

    let mut allocations = vec![1_usize; interval_count];
    for _ in interval_count..segment_budget {
        let next = (0..interval_count)
            .max_by(|left, right| {
                let left_span = interval_lengths[*left] / allocations[*left] as f32;
                let right_span = interval_lengths[*right] / allocations[*right] as f32;
                left_span
                    .partial_cmp(&right_span)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| right.cmp(left))
            })
            .expect("non-empty interval set");
        allocations[next] += 1;
    }

    let mut samples = Vec::with_capacity(sample_count);
    for interval in 0..interval_count {
        let start = feature_distances[interval];
        let end = if interval + 1 < feature_distances.len() {
            feature_distances[interval + 1]
        } else {
            total
        };
        let count = allocations[interval];
        for step in 0..count {
            let progress = step as f32 / count as f32;
            let target_distance = start + (end - start) * progress;
            samples.push(sample_polyline(contour, &cumulative, target_distance));
        }
    }
    if !contour.closed {
        samples.push(*contour.points.last().expect("validated contour"));
    }
    debug_assert_eq!(samples.len(), sample_count);
    Ok(samples)
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

fn mark_feature(features: &mut Vec<usize>, index: usize) {
    if features.last().copied() != Some(index) {
        features.push(index);
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
    if progress <= 0.0 {
        return from;
    }
    if progress >= 1.0 {
        return to;
    }
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

fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

const FILL_AREA_EPSILON: f32 = 1.0e-5;

#[derive(Clone, Debug, PartialEq)]
pub struct FilledMorphPlan {
    pub contour: MorphContourPlan,
    pub source_center: Vec2,
    pub target_center: Vec2,
    /// Triangle indices over boundary points followed by one center vertex.
    pub indices: Vec<u32>,
}

impl FilledMorphPlan {
    pub fn vertex_count(&self) -> usize {
        self.contour.source_points.len() + 1
    }

    pub fn interpolate_vertices(&self, progress: f32) -> Vec<Vec2> {
        let progress = if progress.is_finite() {
            progress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut vertices = self
            .contour
            .source_points
            .iter()
            .zip(&self.contour.target_points)
            .map(|(source, target)| lerp_vec2(*source, *target, progress))
            .collect::<Vec<_>>();
        vertices.push(lerp_vec2(self.source_center, self.target_center, progress));
        vertices
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FilledMorphError {
    Morph(MorphError),
    RequiresSingleClosedContour,
    DegenerateArea { side: MorphSide },
    SelfIntersecting { side: MorphSide },
    NoStableFanTriangulation,
}

impl std::fmt::Display for FilledMorphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Morph(error) => write!(formatter, "filled morph planning failed: {error}"),
            Self::RequiresSingleClosedContour => formatter.write_str(
                "filled path Transform currently requires exactly one closed contour",
            ),
            Self::DegenerateArea { side } => {
                write!(formatter, "filled morph {side:?} contour has degenerate area")
            }
            Self::SelfIntersecting { side } => {
                write!(formatter, "filled morph {side:?} contour self-intersects")
            }
            Self::NoStableFanTriangulation => formatter.write_str(
                "filled morph has no stable center-fan triangulation over the full interpolation interval",
            ),
        }
    }
}

impl std::error::Error for FilledMorphError {}

impl From<MorphError> for FilledMorphError {
    fn from(value: MorphError) -> Self {
        Self::Morph(value)
    }
}

/// Plans the deliberately bounded first filled-path Transform topology.
///
/// The supported class is one simple closed contour whose source and target
/// are both star-shaped around their area centroids, with every center-fan
/// triangle retaining its winding for the complete linear morph. A triangle
/// may collapse at an isolated interior instant, but both endpoints must have
/// positive area and the interpolation may not invert. This gives one fixed
/// triangle topology without per-frame triangulation.
pub fn plan_filled_morph(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
) -> Result<FilledMorphPlan, FilledMorphError> {
    plan_filled_morph_impl(source, target, options, true)
}

/// Filled-path counterpart of [`plan_morph_preserving_order`].
pub fn plan_filled_morph_preserving_order(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
) -> Result<FilledMorphPlan, FilledMorphError> {
    plan_filled_morph_impl(source, target, options, false)
}

fn plan_filled_morph_impl(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
    align_closed_correspondence: bool,
) -> Result<FilledMorphPlan, FilledMorphError> {
    let plan = plan_morph_impl(source, target, options, align_closed_correspondence)?;
    if plan.contours.len() != 1 || !plan.contours[0].closed {
        return Err(FilledMorphError::RequiresSingleClosedContour);
    }
    let mut contour = plan
        .contours
        .into_iter()
        .next()
        .expect("one contour validated");

    canonicalize_ccw(&mut contour.source_points, MorphSide::Source)?;
    canonicalize_ccw(&mut contour.target_points, MorphSide::Target)?;
    if align_closed_correspondence {
        contour.target_points =
            align_closed_contour_preserving_winding(&contour.source_points, &contour.target_points);
    }

    if !polygon_is_simple(&contour.source_points) {
        return Err(FilledMorphError::SelfIntersecting {
            side: MorphSide::Source,
        });
    }
    if !polygon_is_simple(&contour.target_points) {
        return Err(FilledMorphError::SelfIntersecting {
            side: MorphSide::Target,
        });
    }

    let source_center =
        polygon_centroid(&contour.source_points).ok_or(FilledMorphError::DegenerateArea {
            side: MorphSide::Source,
        })?;
    let target_center =
        polygon_centroid(&contour.target_points).ok_or(FilledMorphError::DegenerateArea {
            side: MorphSide::Target,
        })?;

    let count = contour.source_points.len();
    if count < 3 {
        return Err(FilledMorphError::NoStableFanTriangulation);
    }
    for index in 0..count {
        let next = (index + 1) % count;
        let orientation = triangle_orientation_over_interval(
            source_center,
            target_center,
            contour.source_points[index],
            contour.target_points[index],
            contour.source_points[next],
            contour.target_points[next],
        );
        if !orientation.is_valid_non_inverting() {
            return Err(FilledMorphError::NoStableFanTriangulation);
        }
    }

    let center = u32::try_from(count).map_err(|_| FilledMorphError::NoStableFanTriangulation)?;
    let mut indices = Vec::with_capacity(count * 3);
    for index in 0..count {
        let next = (index + 1) % count;
        indices.extend([
            center,
            u32::try_from(index).map_err(|_| FilledMorphError::NoStableFanTriangulation)?,
            u32::try_from(next).map_err(|_| FilledMorphError::NoStableFanTriangulation)?,
        ]);
    }

    Ok(FilledMorphPlan {
        contour,
        source_center,
        target_center,
        indices,
    })
}

fn canonicalize_ccw(points: &mut [Vec2], side: MorphSide) -> Result<(), FilledMorphError> {
    let area = signed_polygon_area(points);
    if !area.is_finite() || area.abs() <= FILL_AREA_EPSILON {
        return Err(FilledMorphError::DegenerateArea { side });
    }
    if area < 0.0 {
        points.reverse();
    }
    Ok(())
}

fn signed_polygon_area(points: &[Vec2]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }
    0.5 * (0..points.len())
        .map(|index| {
            let next = (index + 1) % points.len();
            points[index].x * points[next].y - points[next].x * points[index].y
        })
        .sum::<f32>()
}

fn polygon_centroid(points: &[Vec2]) -> Option<Vec2> {
    let twice_area = 2.0 * signed_polygon_area(points);
    if !twice_area.is_finite() || twice_area.abs() <= 2.0 * FILL_AREA_EPSILON {
        return None;
    }
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    for index in 0..points.len() {
        let next = (index + 1) % points.len();
        let cross = points[index].x * points[next].y - points[next].x * points[index].y;
        x += (points[index].x + points[next].x) * cross;
        y += (points[index].y + points[next].y) * cross;
    }
    let denominator = 3.0 * twice_area;
    let centroid = Vec2::new(x / denominator, y / denominator);
    (centroid.x.is_finite() && centroid.y.is_finite()).then_some(centroid)
}

fn align_closed_contour_preserving_winding(source: &[Vec2], target: &[Vec2]) -> Vec<Vec2> {
    debug_assert_eq!(source.len(), target.len());
    let count = source.len();
    let mut best_cost = f64::INFINITY;
    let mut best_shift = 0;
    for shift in 0..count {
        let cost = (0..count)
            .map(|index| squared_distance(source[index], target[(index + shift) % count]) as f64)
            .sum::<f64>();
        if cost < best_cost {
            best_cost = cost;
            best_shift = shift;
        }
    }
    (0..count)
        .map(|index| target[(index + best_shift) % count])
        .collect()
}

fn polygon_is_simple(points: &[Vec2]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let count = points.len();
    for first in 0..count {
        let first_next = (first + 1) % count;
        if distance(points[first], points[first_next]) <= DEGENERATE_LENGTH_EPSILON {
            return false;
        }
        for second in first + 1..count {
            let second_next = (second + 1) % count;
            if first == second
                || first_next == second
                || second_next == first
                || (first == 0 && second_next == 0)
            {
                continue;
            }
            if segments_intersect(
                points[first],
                points[first_next],
                points[second],
                points[second_next],
            ) {
                return false;
            }
        }
    }
    true
}

fn segments_intersect(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let ab_c = orientation(a, b, c);
    let ab_d = orientation(a, b, d);
    let cd_a = orientation(c, d, a);
    let cd_b = orientation(c, d, b);
    let epsilon = FILL_AREA_EPSILON;

    if ((ab_c > epsilon && ab_d < -epsilon) || (ab_c < -epsilon && ab_d > epsilon))
        && ((cd_a > epsilon && cd_b < -epsilon) || (cd_a < -epsilon && cd_b > epsilon))
    {
        return true;
    }
    (ab_c.abs() <= epsilon && point_on_segment(c, a, b))
        || (ab_d.abs() <= epsilon && point_on_segment(d, a, b))
        || (cd_a.abs() <= epsilon && point_on_segment(a, c, d))
        || (cd_b.abs() <= epsilon && point_on_segment(b, c, d))
}

fn orientation(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    cross(
        Vec2::new(b.x - a.x, b.y - a.y),
        Vec2::new(c.x - a.x, c.y - a.y),
    )
}

fn point_on_segment(point: Vec2, start: Vec2, end: Vec2) -> bool {
    let epsilon = FILL_AREA_EPSILON;
    point.x >= start.x.min(end.x) - epsilon
        && point.x <= start.x.max(end.x) + epsilon
        && point.y >= start.y.min(end.y) - epsilon
        && point.y <= start.y.max(end.y) + epsilon
}

#[derive(Clone, Copy, Debug)]
struct TriangleOrientationInterval {
    start: f64,
    end: f64,
    minimum: f64,
    numerical_tolerance: f64,
}

impl TriangleOrientationInterval {
    fn is_valid_non_inverting(self) -> bool {
        self.start.is_finite()
            && self.end.is_finite()
            && self.minimum.is_finite()
            && self.start > FILL_AREA_EPSILON as f64
            && self.end > FILL_AREA_EPSILON as f64
            && self.minimum >= -self.numerical_tolerance
    }
}

fn triangle_orientation_over_interval(
    source_center: Vec2,
    target_center: Vec2,
    source_a: Vec2,
    target_a: Vec2,
    source_b: Vec2,
    target_b: Vec2,
) -> f64 {
    let a0 = Vec2::new(source_a.x - source_center.x, source_a.y - source_center.y);
    let b0 = Vec2::new(source_b.x - source_center.x, source_b.y - source_center.y);
    let center_delta = Vec2::new(
        target_center.x - source_center.x,
        target_center.y - source_center.y,
    );
    let da = Vec2::new(
        target_a.x - source_a.x - center_delta.x,
        target_a.y - source_a.y - center_delta.y,
    );
    let db = Vec2::new(
        target_b.x - source_b.x - center_delta.x,
        target_b.y - source_b.y - center_delta.y,
    );
    let c0 = cross(a0, b0) as f64;
    let c1 = (cross(da, b0) + cross(a0, db)) as f64;
    let c2 = cross(da, db) as f64;
    let evaluate = |time: f64| c0 + c1 * time + c2 * time * time;
    let start = evaluate(0.0);
    let end = evaluate(1.0);
    let mut minimum = start.min(end);
    if c2 > 0.0 {
        let critical = -c1 / (2.0 * c2);
        if (0.0..1.0).contains(&critical) {
            minimum = minimum.min(evaluate(critical));
        }
    }
    // The coefficients originate in f32 geometry, then the quadratic minimum
    // is evaluated in f64. Permit only the rounding-sized negative residue of
    // an otherwise tangential collapse; a material negative minimum would
    // reverse a fixed fan triangle and still fails validation.
    let numerical_tolerance = f64::EPSILON * 64.0 * (c0.abs() + c1.abs() + c2.abs()).max(1.0);
    TriangleOrientationInterval {
        start,
        end,
        minimum,
        numerical_tolerance,
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
    fn preserving_order_filled_half_turn_keeps_a_fixed_fan_through_collapse() {
        let source = square_from(0, false);
        // This is the same square rotated by π while retaining point
        // correspondence. Every fan triangle collapses at t=0.5, then resumes
        // with the original winding; no retessellation is required.
        let target = square_from(2, false);

        let plan = plan_filled_morph_preserving_order(&source, &target, MorphOptions::DEFAULT)
            .expect("a tangential interior collapse keeps its fixed fan");
        let midpoint = plan.interpolate_vertices(0.5);

        assert_eq!(plan.vertex_count(), 5);
        assert!(midpoint.iter().all(|point| point.x.abs() < 1.0e-6));
        assert!(midpoint.iter().all(|point| point.y.abs() < 1.0e-6));
    }

    #[test]
    fn filled_fan_still_rejects_an_interior_winding_reversal() {
        let orientation = triangle_orientation_over_interval(
            Vec2::ZERO,
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            Vec2::new(-1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(0.0, -2.0),
        );

        assert!(orientation.start > 0.0);
        assert!(orientation.end > 0.0);
        assert!(orientation.minimum < 0.0);
        assert!(!orientation.is_valid_non_inverting());
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
    fn authored_star_vertices_survive_morph_resampling_exactly() {
        let source = VectorPath::new()
            .move_to(Vec2::new(0.0, 1.65))
            .cubic_to(
                Vec2::new(0.95, 1.65),
                Vec2::new(1.65, 0.95),
                Vec2::new(1.65, 0.0),
            )
            .cubic_to(
                Vec2::new(1.65, -0.95),
                Vec2::new(0.95, -1.65),
                Vec2::new(0.0, -1.65),
            )
            .cubic_to(
                Vec2::new(-0.95, -1.65),
                Vec2::new(-1.65, -0.95),
                Vec2::new(-1.65, 0.0),
            )
            .cubic_to(
                Vec2::new(-1.65, 0.95),
                Vec2::new(-0.95, 1.65),
                Vec2::new(0.0, 1.65),
            )
            .close();
        let target_vertices = [
            Vec2::new(0.0, 2.0),
            Vec2::new(0.47, 0.65),
            Vec2::new(1.9, 0.62),
            Vec2::new(0.76, -0.25),
            Vec2::new(1.18, -1.62),
            Vec2::new(0.0, -0.82),
            Vec2::new(-1.18, -1.62),
            Vec2::new(-0.76, -0.25),
            Vec2::new(-1.9, 0.62),
            Vec2::new(-0.47, 0.65),
        ];
        let mut target = VectorPath::new().move_to(target_vertices[0]);
        for vertex in &target_vertices[1..] {
            target = target.line_to(*vertex);
        }
        target = target.close();

        let plan = plan_morph(&source, &target, MorphOptions::DEFAULT).expect("valid morph");
        let samples = &plan.contours[0].target_points;
        assert_eq!(samples.len(), MorphOptions::DEFAULT.samples_per_contour);
        for vertex in target_vertices {
            assert!(samples
                .iter()
                .any(|sample| squared_distance(*sample, vertex) < 1.0e-10));
        }
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
