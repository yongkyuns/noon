//! Immutable observations of one retained path in an explicit coordinate space.
mod point_counts;
pub use point_counts::effective_mobject_point_count;

use crate::{
    AuthoringError, ExecutionSession, Mobject, MobjectTarget, UnsupportedAuthoringOperation,
};
use noon_core::{
    GeometryRef, GeometryResource, SemanticNodeId, SemanticNodeKind, SemanticObjectContent,
    SemanticStore, SemanticTransform2_5D, StoredGeometry, VectorPath,
};
use noon_geometry::{
    canonical_outline_path, PathProportionError, PathProportionPlan, MANIM_LENGTH_SAMPLE_POINTS,
};
use std::{cell::RefCell, rc::Rc};

/// A reusable snapshot of one path and its coordinate-space transform.
/// Preparation is O(path curves); proportion queries are O(log curves). This
/// observation owns derived query data, never scene or runtime authority.
#[derive(Clone, Debug)]
pub struct PathQuery {
    plan: Option<PathProportionPlan>,
    /// ManimCE's flattened four-point cubic storage, including padding between
    /// an unfinished subpath and the next MoveTo.
    manim_points: Vec<(f64, f64)>,
    transform: SemanticTransform2_5D,
    endpoints: Option<((f64, f64), (f64, f64))>,
    unfinished_anchor: Option<(f64, f64)>,
}

/// A recursive snapshot of owned planar points in Manim family order.
///
/// Unlike [`MobjectTarget::all_points`], this preserves object boundaries so
/// language adapters can invoke their caller-owned reducer once per object and
/// once per family without owning semantic family traversal.
#[derive(Clone, Debug, PartialEq)]
pub struct PointReductionTree {
    /// The object's own points in its current coordinate space.
    pub points: Vec<(f64, f64)>,
    /// Direct family members in semantic membership order.
    pub children: Vec<Self>,
}

impl PointReductionTree {
    /// Flatten every owned point array in semantic pre-order, retaining aliases.
    pub fn flattened_points(&self) -> Vec<(f64, f64)> {
        let mut points = self.points.clone();
        for child in &self.children {
            points.extend(child.flattened_points());
        }
        points
    }
}

#[derive(Clone, Debug)]
struct PointReductionTopology {
    object: Option<SemanticNodeId>,
    children: Vec<Self>,
}

impl PathQuery {
    pub(crate) fn prepare(
        path: &VectorPath,
        transform: SemanticTransform2_5D,
    ) -> Result<Self, AuthoringError> {
        let plan =
            match PathProportionPlan::with_scale(path, (transform.scale.x, transform.scale.y)) {
                Ok(plan) => Some(plan),
                Err(PathProportionError::EmptyPath) => None,
                Err(error) => return Err(AuthoringError::PathQuery(error)),
            };
        let raw_points = path.manim_points();
        let manim_points = match raw_points {
            Some(points) => points.to_vec(),
            None => manim_points(path, plan.as_ref())?,
        };
        let endpoints = raw_points
            .and_then(|points| points.first().zip(points.last()))
            .map(|(first, last)| (*first, *last))
            .or_else(|| {
                path.endpoints()
                    .map(|(a, b)| ((a.x as f64, a.y as f64), (b.x as f64, b.y as f64)))
            });
        Ok(Self {
            manim_points,
            plan,
            transform,
            endpoints,
            unfinished_anchor: match path.commands().last() {
                Some(noon_core::PathCommand::MoveTo { to }) => Some(
                    raw_points
                        .and_then(|points| points.get(points.len() / 4 * 4).copied())
                        .unwrap_or((to.x as f64, to.y as f64)),
                ),
                _ => None,
            },
        })
    }

    /// Number of complete four-point curves in Manim's stored point array.
    pub fn curve_count(&self) -> usize {
        self.manim_points.len() / 4
    }

    /// Manim's cubic point-array length, including unfinished anchors and any
    /// degenerate curve padding used to start a new subpath.
    pub fn point_count(&self) -> usize {
        self.manim_points.len()
    }

    /// Whether the raw point array ends with a pending subpath anchor at the
    /// default four-point cubic stride.
    pub fn has_new_path_started(&self) -> bool {
        self.has_new_path_started_with_stride(4)
    }

    /// Whether the raw point array ends with a pending subpath anchor at the
    /// caller's current Manim stride.
    pub fn has_new_path_started_with_stride(&self, stride: i32) -> bool {
        point_array_has_new_path_started(self.point_count(), stride)
    }

    /// Cubic controls in this snapshot's coordinate space. Lines and quadratics
    /// are promoted exactly; the returned points are derived observations.
    pub fn curve_points(&self, index: usize) -> Result<[(f64, f64); 4], AuthoringError> {
        if index >= self.curve_count() {
            return Err(AuthoringError::PathQuery(
                PathProportionError::InvalidCurveIndex {
                    index,
                    count: self.curve_count(),
                },
            ));
        }
        let start = index * 4;
        Ok(std::array::from_fn(|offset| {
            let (x, y) = self.manim_points[start + offset];
            self.transform_point(x, y)
        }))
    }

    /// Evaluate one curve at each supplied parameter in this snapshot's
    /// coordinate space. Parameters outside `[0, 1]` extrapolate the curve.
    pub fn curve_points_at(
        &self,
        index: usize,
        parameters: &[f64],
    ) -> Result<Vec<(f64, f64, f64)>, AuthoringError> {
        let controls = self.curve_points(index)?;
        Ok(parameters
            .iter()
            .map(|&t| {
                let (x, y) = evaluate_cubic(controls, t);
                (x, y, 0.0)
            })
            .collect())
    }

    /// Evaluate an arbitrary Manim point-array slice as a Bezier curve.
    ///
    /// This preserves VMobject's mutable `n_points_per_cubic_curve` behavior,
    /// where callers may select line, quadratic, or higher-grade point slices.
    pub fn bezier_points_at_range(
        &self,
        start: usize,
        stop: usize,
        parameters: &[f64],
    ) -> Result<Vec<(f64, f64, f64)>, AuthoringError> {
        let controls = self.bezier_range_controls(start, stop)?;
        Ok(parameters
            .iter()
            .map(|&t| {
                let (x, y) = evaluate_bezier_points(&controls, t);
                (x, y, 0.0)
            })
            .collect())
    }

    /// Find rounded ManimCE parameters where an arbitrary Bézier point slice
    /// intersects a planar point.
    pub fn bezier_parameters_for_point_range(
        &self,
        start: usize,
        stop: usize,
        point: (f64, f64, f64),
    ) -> Result<Vec<f64>, AuthoringError> {
        if point.2 != 0.0 {
            return Ok(Vec::new());
        }
        let controls = self.bezier_range_controls(start, stop)?;
        Ok(bezier_parameters_for_point(&controls, (point.0, point.1)))
    }

    /// Sample lengths for an arbitrary Manim point-array Bezier slice.
    pub fn bezier_length_pieces_range(
        &self,
        start: usize,
        stop: usize,
        sample_points: Option<usize>,
    ) -> Result<Vec<f64>, AuthoringError> {
        let controls = self.bezier_range_controls(start, stop)?;
        let sample_points = sample_points.unwrap_or(MANIM_LENGTH_SAMPLE_POINTS);
        if sample_points < 2 {
            return Ok(Vec::new());
        }
        let mut pieces = Vec::with_capacity(sample_points - 1);
        let step = 1.0 / (sample_points - 1) as f64;
        let mut previous = evaluate_bezier_points(&controls, 0.0);
        for sample in 1..sample_points {
            let current = evaluate_bezier_points(
                &controls,
                if sample + 1 == sample_points {
                    1.0
                } else {
                    sample as f64 * step
                },
            );
            pieces.push((current.0 - previous.0).hypot(current.1 - previous.1));
            previous = current;
        }
        Ok(pieces)
    }

    fn bezier_range_controls(
        &self,
        start: usize,
        stop: usize,
    ) -> Result<Vec<(f64, f64)>, AuthoringError> {
        if start >= stop || stop > self.manim_points.len() {
            return Err(AuthoringError::PathQuery(
                PathProportionError::InvalidCurveIndex {
                    index: stop,
                    count: self.manim_points.len(),
                },
            ));
        }
        Ok(self.manim_points[start..stop]
            .iter()
            .map(|&(x, y)| self.transform_point(x, y))
            .collect())
    }

    /// Straight-line sample lengths for one curve, matching ManimCE's
    /// `get_nth_curve_length_pieces` default of ten Bezier samples.
    pub fn curve_length_pieces(
        &self,
        index: usize,
        sample_points: Option<usize>,
    ) -> Result<Vec<f64>, AuthoringError> {
        let controls = self.curve_points(index)?;
        let sample_points = sample_points.unwrap_or(MANIM_LENGTH_SAMPLE_POINTS);
        if sample_points < 2 {
            return Ok(Vec::new());
        }
        let mut pieces = Vec::with_capacity(sample_points - 1);
        let mut previous = controls[0];
        let step = 1.0 / (sample_points - 1) as f64;
        for sample in 1..sample_points {
            let t = if sample + 1 == sample_points {
                1.0
            } else {
                sample as f64 * step
            };
            let current = evaluate_cubic(controls, t);
            pieces.push((current.0 - previous.0).hypot(current.1 - previous.1));
            previous = current;
        }
        Ok(pieces)
    }

    /// Approximate length of one curve using the same samples as its pieces.
    pub fn curve_length(
        &self,
        index: usize,
        sample_points: Option<usize>,
    ) -> Result<f64, AuthoringError> {
        Ok(self
            .curve_length_pieces(index, sample_points)?
            .into_iter()
            .sum())
    }

    fn curve_column(&self, column: usize) -> Vec<(f64, f64)> {
        let mut points = (0..self.curve_count())
            .map(|index| self.curve_points(index).expect("known curve index")[column])
            .collect::<Vec<_>>();
        if column == 0 {
            if let Some(point) = self.unfinished_anchor {
                points.push(self.transform_point(point.0, point.1));
            }
        }
        points
    }

    pub fn start_anchors(&self) -> Vec<(f64, f64)> {
        self.curve_column(0)
    }

    /// Return Manim's shoelace direction for the ordered start anchors.
    /// This intentionally mirrors `numpy.trapezoid(y, x)` without adding a
    /// closing edge: Manim applies that operation to the anchor array as-is.
    pub fn direction(&self) -> &'static str {
        shoelace_direction(&self.start_anchors())
    }

    /// Return direction for the raw Manim point array at its mutable stride.
    /// Positive and negative strides follow Python's `points[::stride]` order.
    pub fn direction_with_stride(&self, stride: i32) -> Option<&'static str> {
        if stride == 0 {
            return None;
        }
        let step = stride.unsigned_abs() as usize;
        let transform = |point: &(f64, f64)| self.transform_point(point.0, point.1);
        let anchors = if stride > 0 {
            self.manim_points
                .iter()
                .step_by(step)
                .map(transform)
                .collect::<Vec<_>>()
        } else {
            self.manim_points
                .iter()
                .rev()
                .step_by(step)
                .map(transform)
                .collect::<Vec<_>>()
        };
        Some(shoelace_direction(&anchors))
    }

    /// Manim Polygram vertex runs end whenever a curve returns to their first
    /// anchor. This also splits repeated starting vertices, as upstream does.
    pub fn vertex_groups(&self) -> Vec<Vec<(f64, f64)>> {
        let mut groups = Vec::new();
        let mut group = Vec::new();
        for index in 0..self.curve_count() {
            let points = self.curve_points(index).expect("known curve index");
            group.push(points[0]);
            if points_coincide(points[3], group[0]) {
                groups.push(std::mem::take(&mut group));
            }
        }
        groups
    }
    pub fn first_handles(&self) -> Vec<(f64, f64)> {
        self.curve_column(1)
    }
    pub fn second_handles(&self) -> Vec<(f64, f64)> {
        self.curve_column(2)
    }
    pub fn end_anchors(&self) -> Vec<(f64, f64)> {
        self.curve_column(3)
    }

    /// Start anchors, first handles, second handles and end anchors. An
    /// unfinished subpath contributes only to the start-anchor column.
    pub fn anchors_and_handles(&self) -> [Vec<(f64, f64)>; 4] {
        std::array::from_fn(|column| self.curve_column(column))
    }

    /// Ordered start/end anchors, followed by any unfinished starting anchor.
    pub fn anchors(&self) -> Vec<(f64, f64)> {
        let mut anchors = Vec::with_capacity(
            self.curve_count() * 2 + usize::from(self.unfinished_anchor.is_some()),
        );
        for index in 0..self.curve_count() {
            let points = self.curve_points(index).expect("known curve index");
            anchors.extend([points[0], points[3]]);
        }
        if let Some(point) = self.unfinished_anchor {
            anchors.push(self.transform_point(point.0, point.1));
        }
        anchors
    }

    /// ManimCE's flattened cubic point array, including any padded subpaths.
    pub fn points(&self) -> Result<Vec<(f64, f64)>, AuthoringError> {
        Ok(self
            .manim_points
            .iter()
            .map(|&(x, y)| self.transform_point(x, y))
            .collect())
    }

    /// Whether the first and last stored Manim points are close using the
    /// object's configured point-equality tolerance.
    pub fn is_closed(&self) -> Result<bool, AuthoringError> {
        self.is_closed_with_tolerance(1e-6)
    }

    pub fn is_closed_with_tolerance(&self, tolerance: f64) -> Result<bool, AuthoringError> {
        let first = self
            .manim_points
            .first()
            .copied()
            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?;
        let last = self
            .manim_points
            .last()
            .copied()
            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?;
        let first = self.transform_point(first.0, first.1);
        let last = self.transform_point(last.0, last.1);
        Ok(manim_points_allclose(
            &[first.0, first.1, 0.0],
            &[last.0, last.1, 0.0],
            tolerance,
        ))
    }

    /// Cubic point runs separated by noncoincident anchors, matching Manim's
    /// observable subpaths. Explicit retained contour breaks remain unchanged.
    /// A dangling anchor joins the final run only when it coincides with its end.
    pub fn subpaths(&self) -> Vec<Vec<(f64, f64)>> {
        let mut result = Vec::new();
        let mut run = Vec::new();
        for index in 0..self.curve_count() {
            let points = self.curve_points(index).expect("known curve index");
            if run
                .last()
                .is_some_and(|last| !points_coincide(*last, points[0]))
            {
                result.push(std::mem::take(&mut run));
            }
            run.extend(points);
        }
        if let Some(point) = self.unfinished_anchor {
            let point = self.transform_point(point.0, point.1);
            if run.last().is_some_and(|last| points_coincide(*last, point)) {
                run.push(point);
            }
        }
        if !run.is_empty() {
            result.push(run);
        }
        result
    }

    /// Point in the coordinate space captured when this query was prepared.
    pub fn point_from_proportion(&self, alpha: f64) -> Result<(f64, f64), AuthoringError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(AuthoringError::PathQuery(
                PathProportionError::InvalidProportion(alpha as f32),
            ));
        }
        if alpha == 1.0 {
            return self.end();
        }
        let point = self
            .plan
            .as_ref()
            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?
            .point_f64(alpha)
            .map_err(AuthoringError::PathQuery)?;
        Ok(self.transform_point(point.x, point.y))
    }

    /// Find the arc-length proportion of a point on this planar path, matching
    /// ManimCE's rounded Bézier-root intersection and latest-parameter choice.
    pub fn proportion_from_point(
        &self,
        point: (f64, f64, f64),
    ) -> Result<Option<f64>, AuthoringError> {
        let lengths = (0..self.curve_count())
            .map(|index| self.curve_length(index, None))
            .collect::<Result<Vec<_>, _>>()?;
        proportion_for_curve_lengths(&lengths, |index| {
            let controls = self.curve_points(index)?;
            Ok(if point.2 == 0.0 {
                cubic_parameters_for_point(controls, (point.0, point.1))
            } else {
                Vec::new()
            })
        })
    }

    /// Inverse query over caller-selected Manim point slices, in supplied order.
    ///
    /// Arbitrary Bezier degrees use the same sampled-length normalization and
    /// first-matching-curve/latest-parameter rules as the default cubic query.
    pub fn proportion_from_point_in_ranges(
        &self,
        ranges: &[(usize, usize)],
        point: (f64, f64, f64),
    ) -> Result<Option<f64>, AuthoringError> {
        let lengths = ranges
            .iter()
            .map(|&(start, stop)| {
                self.bezier_length_pieces_range(start, stop, None)
                    .map(|pieces| pieces.into_iter().sum())
            })
            .collect::<Result<Vec<_>, _>>()?;
        proportion_for_curve_lengths(&lengths, |index| {
            let (start, stop) = ranges[index];
            self.bezier_parameters_for_point_range(start, stop, point)
        })
    }

    fn transform_point(&self, x: f64, y: f64) -> (f64, f64) {
        let x = x * self.transform.scale.x;
        let y = y * self.transform.scale.y;
        let (sin, cos) = self.transform.rotation_z.sin_cos();
        (
            x * cos - y * sin + self.transform.translation.x,
            x * sin + y * cos + self.transform.translation.y,
        )
    }

    pub fn start(&self) -> Result<(f64, f64), AuthoringError> {
        self.endpoint(false)
    }
    pub fn end(&self) -> Result<(f64, f64), AuthoringError> {
        self.endpoint(true)
    }

    fn endpoint(&self, end: bool) -> Result<(f64, f64), AuthoringError> {
        let (first, last) = self
            .endpoints
            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?;
        let point = if end { last } else { first };
        Ok(self.transform_point(point.0, point.1))
    }

    /// Ten samples per curve by default, matching Manim's path query measure.
    pub fn arc_length(
        &self,
        sample_points_per_curve: Option<usize>,
    ) -> Result<f64, AuthoringError> {
        self.plan.as_ref().map_or(Ok(0.0), |plan| {
            plan.arc_length(sample_points_per_curve)
                .map_err(AuthoringError::PathQuery)
        })
    }
}

fn proportion_for_curve_lengths(
    lengths: &[f64],
    mut parameters: impl FnMut(usize) -> Result<Vec<f64>, AuthoringError>,
) -> Result<Option<f64>, AuthoringError> {
    // Use the same captured-coordinate measure for both terms. A cached local
    // metric can round differently after affine edits, producing values above
    // one that the corresponding forward query correctly rejects.
    let total_length: f64 = lengths.iter().sum();
    let mut preceding_length = 0.0;
    for (index, &length) in lengths.iter().enumerate() {
        if let Some(parameter) = parameters(index)?.into_iter().reduce(f64::max) {
            return Ok(Some((preceding_length + length * parameter) / total_length));
        }
        preceding_length += length;
    }
    Ok(None)
}

fn shoelace_direction(anchors: &[(f64, f64)]) -> &'static str {
    let integral = anchors
        .windows(2)
        .map(|pair| (pair[1].0 - pair[0].0) * (pair[0].1 + pair[1].1) * 0.5)
        .sum::<f64>();
    if integral > 0.0 {
        "CW"
    } else {
        "CCW"
    }
}

pub(crate) fn point_array_has_new_path_started(point_count: usize, stride: i32) -> bool {
    if stride == 0 {
        return false;
    }
    let divisor = i64::from(stride);
    let mut remainder = point_count as i64 % divisor;
    if remainder != 0 && remainder.signum() != divisor.signum() {
        remainder += divisor;
    }
    remainder == 1
}

pub(crate) fn points_coincide(left: (f64, f64), right: (f64, f64)) -> bool {
    (left.0 - right.0).abs() <= 1e-6 + 1e-5 * right.0.abs()
        && (left.1 - right.1).abs() <= 1e-6 + 1e-5 * right.1.abs()
}

fn evaluate_cubic(controls: [(f64, f64); 4], t: f64) -> (f64, f64) {
    let point = noon_geometry::cubic_bezier_point_f64(
        controls.map(|(x, y)| noon_core::SemanticVec3::new(x, y, 0.0)),
        t,
    );
    (point.x, point.y)
}

fn evaluate_bezier_points(controls: &[(f64, f64)], t: f64) -> (f64, f64) {
    match controls {
        [point] => *point,
        [start, end] => (
            start.0 + t * (end.0 - start.0),
            start.1 + t * (end.1 - start.1),
        ),
        [start, control, end] => {
            let t2 = t * t;
            let mt = 1.0 - t;
            let mt2 = mt * mt;
            (
                mt2 * start.0 + 2.0 * t * mt * control.0 + t2 * end.0,
                mt2 * start.1 + 2.0 * t * mt * control.1 + t2 * end.1,
            )
        }
        [p0, p1, p2, p3] => evaluate_cubic([*p0, *p1, *p2, *p3], t),
        _ => {
            let mut points = controls.to_vec();
            for remaining in (1..points.len()).rev() {
                for index in 0..remaining {
                    points[index].0 += t * (points[index + 1].0 - points[index].0);
                    points[index].1 += t * (points[index + 1].1 - points[index].1);
                }
            }
            points[0]
        }
    }
}

/// Return Manim's six-decimal real parameters where a cubic crosses a 2D point.
fn cubic_parameters_for_point(controls: [(f64, f64); 4], point: (f64, f64)) -> Vec<f64> {
    let x_roots = cubic_coordinate_roots(controls.map(|control| control.0), point.0);
    let y_roots = cubic_coordinate_roots(controls.map(|control| control.1), point.1);
    match (x_roots, y_roots) {
        (None, None) => Vec::new(),
        (Some(roots), None) | (None, Some(roots)) => roots,
        (Some(mut x), Some(y)) => {
            x.retain(|root| {
                y.binary_search_by(|candidate| candidate.total_cmp(root))
                    .is_ok()
            });
            x
        }
    }
}

fn bezier_parameters_for_point(controls: &[(f64, f64)], point: (f64, f64)) -> Vec<f64> {
    if controls.len() == 4 {
        return cubic_parameters_for_point(
            [controls[0], controls[1], controls[2], controls[3]],
            point,
        );
    }
    let x_roots = bezier_coordinate_roots(
        &controls.iter().map(|control| control.0).collect::<Vec<_>>(),
        point.0,
    );
    let y_roots = bezier_coordinate_roots(
        &controls.iter().map(|control| control.1).collect::<Vec<_>>(),
        point.1,
    );
    match (x_roots, y_roots) {
        (None, None) => Vec::new(),
        (Some(roots), None) | (None, Some(roots)) => roots,
        (Some(mut x), Some(y)) => {
            x.retain(|root| {
                y.binary_search_by(|candidate| candidate.total_cmp(root))
                    .is_ok()
            });
            x
        }
    }
}

/// Return rounded real roots for one coordinate. `None` means that coordinate
/// is constant and already equal to the requested point coordinate.
fn bezier_coordinate_roots(controls: &[f64], point: f64) -> Option<Vec<f64>> {
    if controls.is_empty() {
        return Some(Vec::new());
    }
    let degree = controls.len() - 1;
    let mut coefficients = vec![0.0; controls.len()];
    for (index, &control) in controls.iter().enumerate() {
        for expansion in 0..=degree - index {
            let power = index + expansion;
            let sign = if expansion % 2 == 0 { 1.0 } else { -1.0 };
            coefficients[power] +=
                sign * binomial(degree, index) * binomial(degree - index, expansion) * control;
        }
    }
    coefficients[0] -= point;
    if coefficients.iter().all(|coefficient| *coefficient == 0.0) {
        return None;
    }

    let mut roots = real_polynomial_roots_in_unit_interval(&coefficients)
        .into_iter()
        .map(|root| (root * 1_000_000.0).round_ties_even() / 1_000_000.0)
        .filter(|root| (0.0..=1.0).contains(root))
        .collect::<Vec<_>>();
    roots.sort_by(f64::total_cmp);
    roots.dedup();
    Some(roots)
}

fn binomial(n: usize, k: usize) -> f64 {
    let k = k.min(n.saturating_sub(k));
    (1..=k).fold(1.0, |value, index| {
        value * (n + 1 - index) as f64 / index as f64
    })
}

/// Isolate all real roots in `[0, 1]` by recursively partitioning a polynomial
/// at its derivative roots. Low-degree paths use the existing exact solver.
fn real_polynomial_roots_in_unit_interval(coefficients: &[f64]) -> Vec<f64> {
    if coefficients
        .iter()
        .any(|coefficient| !coefficient.is_finite())
    {
        return Vec::new();
    }
    let degree = coefficients
        .iter()
        .rposition(|coefficient| *coefficient != 0.0)
        .unwrap_or(0);
    if degree == 0 {
        return Vec::new();
    }
    if degree <= 3 {
        let mut cubic = [0.0; 4];
        cubic[..=degree].copy_from_slice(&coefficients[..=degree]);
        return real_polynomial_roots(cubic)
            .into_iter()
            .filter(|root| root.is_finite() && (0.0..=1.0).contains(root))
            .collect();
    }

    let derivative = coefficients
        .iter()
        .enumerate()
        .skip(1)
        .map(|(power, coefficient)| *coefficient * power as f64)
        .collect::<Vec<_>>();
    let mut boundaries = vec![0.0];
    boundaries.extend(real_polynomial_roots_in_unit_interval(&derivative));
    boundaries.push(1.0);
    boundaries.sort_by(f64::total_cmp);
    boundaries.dedup();

    let scale = coefficients
        .iter()
        .map(|coefficient| coefficient.abs())
        .sum::<f64>()
        .max(1.0);
    let zero_tolerance = scale * 1.0e-13;
    let evaluate = |parameter: f64| {
        coefficients
            .iter()
            .rev()
            .fold(0.0, |value, coefficient| value * parameter + coefficient)
    };
    let mut roots = Vec::new();
    for &boundary in &boundaries {
        if evaluate(boundary).abs() <= zero_tolerance {
            roots.push(boundary);
        }
    }
    for interval in boundaries.windows(2) {
        let mut low = interval[0];
        let mut high = interval[1];
        let mut low_value = evaluate(low);
        let high_value = evaluate(high);
        if low_value == 0.0 || high_value == 0.0 || low_value.signum() == high_value.signum() {
            continue;
        }
        for _ in 0..80 {
            let middle = (low + high) * 0.5;
            let middle_value = evaluate(middle);
            if middle_value == 0.0 {
                low = middle;
                high = middle;
                break;
            }
            if low_value.signum() == middle_value.signum() {
                low = middle;
                low_value = middle_value;
            } else {
                high = middle;
            }
        }
        roots.push((low + high) * 0.5);
    }
    roots
}

/// `None` means this coordinate is constant and equal to the requested value.
fn cubic_coordinate_roots(controls: [f64; 4], point: f64) -> Option<Vec<f64>> {
    let coefficients = [
        controls[0] - point,
        3.0 * (controls[1] - controls[0]),
        3.0 * (controls[0] - 2.0 * controls[1] + controls[2]),
        -controls[0] + 3.0 * controls[1] - 3.0 * controls[2] + controls[3],
    ];
    if coefficients.iter().all(|coefficient| *coefficient == 0.0) {
        return None;
    }

    let mut roots = real_polynomial_roots(coefficients)
        .into_iter()
        .map(|root| (root * 1_000_000.0).round_ties_even() / 1_000_000.0)
        .filter(|root| (0.0..=1.0).contains(root))
        .collect::<Vec<_>>();
    roots.sort_by(f64::total_cmp);
    roots.dedup();
    Some(roots)
}

/// Solve a cubic or lower-degree polynomial with coefficients in ascending order.
fn real_polynomial_roots(coefficients: [f64; 4]) -> Vec<f64> {
    if coefficients
        .iter()
        .any(|coefficient| !coefficient.is_finite())
    {
        return Vec::new();
    }
    let mut degree = 3;
    while degree > 0 && coefficients[degree] == 0.0 {
        degree -= 1;
    }
    match degree {
        0 => Vec::new(),
        1 => vec![-coefficients[0] / coefficients[1]],
        2 => quadratic_real_roots(coefficients[2], coefficients[1], coefficients[0]),
        _ => cubic_real_roots(
            coefficients[3],
            coefficients[2],
            coefficients[1],
            coefficients[0],
        ),
    }
}

fn quadratic_real_roots(a: f64, b: f64, c: f64) -> Vec<f64> {
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 || !discriminant.is_finite() {
        return Vec::new();
    }
    if discriminant == 0.0 {
        return vec![-b / (2.0 * a)];
    }
    let square_root = discriminant.sqrt();
    let q = -0.5 * (b + b.signum() * square_root);
    if q == 0.0 {
        vec![
            (-b - square_root) / (2.0 * a),
            (-b + square_root) / (2.0 * a),
        ]
    } else {
        vec![q / a, c / q]
    }
}

fn cubic_real_roots(a: f64, b: f64, c: f64, d: f64) -> Vec<f64> {
    let a2 = b / a;
    let a1 = c / a;
    let a0 = d / a;
    let p = a1 - a2 * a2 / 3.0;
    let q = 2.0 * a2 * a2 * a2 / 27.0 - a2 * a1 / 3.0 + a0;
    let half_q = q * 0.5;
    let third_p = p / 3.0;
    let discriminant = half_q * half_q + third_p * third_p * third_p;

    if discriminant > 0.0 {
        let square_root = discriminant.sqrt();
        vec![(-half_q + square_root).cbrt() + (-half_q - square_root).cbrt() - a2 / 3.0]
    } else if discriminant < 0.0 {
        let radius = 2.0 * (-third_p).sqrt();
        let angle = (3.0 * q / (2.0 * p) * (-3.0 / p).sqrt()).acos();
        (0..3)
            .map(|index| {
                radius * ((angle + 2.0 * std::f64::consts::PI * index as f64) / 3.0).cos()
                    - a2 / 3.0
            })
            .collect()
    } else {
        let repeated = (-half_q).cbrt();
        vec![2.0 * repeated - a2 / 3.0, -repeated - a2 / 3.0]
    }
}

fn manim_points(
    path: &VectorPath,
    plan: Option<&PathProportionPlan>,
) -> Result<Vec<(f64, f64)>, AuthoringError> {
    let mut points = Vec::new();
    let mut current = None;
    let mut subpath_start = None;
    let mut curve_index = 0;

    for command in path.commands() {
        match *command {
            noon_core::PathCommand::MoveTo { to } => {
                let remainder = points.len() % 4;
                if remainder != 0 {
                    let start_index = ((points.len() - 1) / 4) * 4;
                    let start_anchor = points[start_index];
                    for _ in 0..(4 - remainder) {
                        points.push(start_anchor);
                    }
                }
                points.push((f64::from(to.x), f64::from(to.y)));
                current = Some(to);
                subpath_start = Some(to);
            }
            noon_core::PathCommand::LineTo { to }
            | noon_core::PathCommand::QuadraticTo { to, .. }
            | noon_core::PathCommand::CubicTo { to, .. } => {
                if current.is_some() {
                    let controls = plan
                        .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?
                        .curve_points(curve_index)
                        .map_err(AuthoringError::PathQuery)?
                        .map(|point| (point.x, point.y));
                    append_manim_curve(&mut points, controls);
                    curve_index += 1;
                }
                current = Some(to);
            }
            noon_core::PathCommand::Close => {
                if let (Some(from), Some(start)) = (current, subpath_start) {
                    if (from - start).length() > f32::EPSILON {
                        let controls = plan
                            .ok_or(AuthoringError::PathQuery(PathProportionError::EmptyPath))?
                            .curve_points(curve_index)
                            .map_err(AuthoringError::PathQuery)?
                            .map(|point| (point.x, point.y));
                        append_manim_curve(&mut points, controls);
                        curve_index += 1;
                    }
                    current = Some(start);
                }
            }
        }
    }
    Ok(points)
}

fn append_manim_curve(points: &mut Vec<(f64, f64)>, controls: [(f64, f64); 4]) {
    if points.len() % 4 == 1 {
        points.extend(controls.into_iter().skip(1));
    } else {
        points.extend(controls);
    }
}

pub(crate) fn content_path(
    store: &SemanticStore,
    content: SemanticObjectContent,
) -> Result<std::borrow::Cow<'_, VectorPath>, AuthoringError> {
    let SemanticObjectContent::Geometry(content) = content else {
        return Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::PathQueryContent,
        ));
    };
    let primitive = match content {
        StoredGeometry::Circle { radius } => GeometryRef::circle(radius),
        StoredGeometry::Rectangle { size } => GeometryRef::Rectangle { size },
        StoredGeometry::Line { start, end } => GeometryRef::Line { start, end },
        StoredGeometry::Resource(handle) => {
            match store
                .geometry_resources()
                .get(handle)
                .ok_or(AuthoringError::MissingGeometryResource(handle))?
            {
                GeometryResource::VectorPath(path) => {
                    return Ok(std::borrow::Cow::Borrowed(path));
                }
                GeometryResource::Mesh(_) => {
                    return Err(AuthoringError::Unsupported(
                        UnsupportedAuthoringOperation::PathQueryContent,
                    ));
                }
            }
        }
    };
    Ok(std::borrow::Cow::Owned(
        canonical_outline_path(&primitive).expect("analytic primitive outline"),
    ))
}

pub(crate) fn prepare_content(
    store: &SemanticStore,
    content: SemanticObjectContent,
    transform: SemanticTransform2_5D,
) -> Result<PathQuery, AuthoringError> {
    PathQuery::prepare(content_path(store, content)?.as_ref(), transform)
}

#[derive(Clone, Copy)]
enum FamilyPointObservation {
    RecursivePoints,
    VectorBoundary,
}

impl FamilyPointObservation {
    fn nodes(
        self,
        store: &SemanticStore,
        root: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, AuthoringError> {
        match self {
            Self::RecursivePoints => family_point_occurrences(store, root),
            Self::VectorBoundary => Ok(store
                .ordered_last_occurrence_authoring_nodes(root)
                .map_err(AuthoringError::from)?
                .into_iter()
                .filter(|id| {
                    matches!(
                        store.node(*id).map(|node| node.kind()),
                        Some(SemanticNodeKind::AuthoringObject)
                    )
                })
                .collect()),
        }
    }

    fn points(self, query: &PathQuery) -> Result<Vec<(f64, f64)>, AuthoringError> {
        match self {
            Self::RecursivePoints => query.points(),
            Self::VectorBoundary => Ok(query.anchors()),
        }
    }
}

fn family_point_occurrences(
    store: &SemanticStore,
    root: SemanticNodeId,
) -> Result<Vec<SemanticNodeId>, AuthoringError> {
    let mut result = Vec::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        let node = store
            .node(id)
            .ok_or_else(|| AuthoringError::from(noon_core::SemanticStoreError::UnknownNode(id)))?;
        match node.kind() {
            SemanticNodeKind::AuthoringObject => result.push(id),
            SemanticNodeKind::Family(_) => {}
            SemanticNodeKind::Signal(_)
            | SemanticNodeKind::Animation(_)
            | SemanticNodeKind::Effect(_) => continue,
        }
        // Keep every child occurrence in pre-order. An explicit worklist avoids
        // growing the Rust call stack while preserving repeated alias runs.
        let children: Vec<_> = node.members_iter().collect();
        pending.extend(children.into_iter().rev());
    }
    Ok(result)
}

fn point_reduction_topology(
    store: &SemanticStore,
    root: SemanticNodeId,
) -> Result<PointReductionTopology, AuthoringError> {
    let node = store
        .node(root)
        .ok_or_else(|| AuthoringError::from(noon_core::SemanticStoreError::UnknownNode(root)))?;
    let object = match node.kind() {
        SemanticNodeKind::AuthoringObject => Some(root),
        SemanticNodeKind::Family(_) => None,
        SemanticNodeKind::Signal(_)
        | SemanticNodeKind::Animation(_)
        | SemanticNodeKind::Effect(_) => {
            return Ok(PointReductionTopology {
                object: None,
                children: Vec::new(),
            })
        }
    };
    Ok(PointReductionTopology {
        object,
        children: node
            .members_iter()
            .map(|member| point_reduction_topology(store, member))
            .collect::<Result<_, _>>()?,
    })
}

fn point_reduction_tree_from_topology(
    topology: &PointReductionTopology,
    point_query: &mut impl FnMut(SemanticNodeId) -> Result<Vec<(f64, f64)>, AuthoringError>,
) -> Result<PointReductionTree, AuthoringError> {
    Ok(PointReductionTree {
        points: topology
            .object
            .map_or_else(|| Ok(Vec::new()), &mut *point_query)?,
        children: topology
            .children
            .iter()
            .map(|child| point_reduction_tree_from_topology(child, point_query))
            .collect::<Result<_, _>>()?,
    })
}

fn authored_points_for_occurrences(
    store: &SemanticStore,
    occurrences: &[SemanticNodeId],
    observation: FamilyPointObservation,
) -> Result<Vec<(f64, f64)>, AuthoringError> {
    let mut result = Vec::new();
    for id in occurrences {
        let state = store
            .semantic_object_state_checked(*id)
            .map_err(AuthoringError::from)?;
        let transform = state
            .transform
            .as_planar()
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        result.extend(observation.points(&prepare_content(store, state.content, transform)?)?);
    }
    Ok(result)
}

impl MobjectTarget<'_> {
    /// Return an immutable recursive point snapshot in semantic family order.
    /// Repeated aliases remain repeated children, matching Manim's recursive
    /// reducer call order while keeping traversal in shared Rust semantics.
    pub fn point_reduction_tree(&self) -> Result<PointReductionTree, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        let topology = point_reduction_topology(&store, self.node_id())?;
        point_reduction_tree_from_topology(&topology, &mut |object| {
            let state = store
                .semantic_object_state_checked(object)
                .map_err(AuthoringError::from)?;
            let transform = state
                .transform
                .as_planar()
                .ok_or(AuthoringError::NonFiniteObjectState)?;
            prepare_content(&store, state.content, transform)?.points()
        })
    }

    /// Return authored points for this object/family in Manim's recursive
    /// pre-order, preserving repeated family aliases as repeated point runs.
    pub fn all_points(&self) -> Result<Vec<(f64, f64)>, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        let observation = FamilyPointObservation::RecursivePoints;
        let occurrences = observation.nodes(&store, self.node_id())?;
        authored_points_for_occurrences(&store, &occurrences, observation)
    }

    /// Return vector-family anchors in Manim's unique family order, including
    /// an ordinary root's own content and all vector descendants.
    pub fn vector_boundary_points(&self) -> Result<Vec<(f64, f64)>, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        let observation = FamilyPointObservation::VectorBoundary;
        let occurrences = observation.nodes(&store, self.node_id())?;
        authored_points_for_occurrences(&store, &occurrences, observation)
    }

    /// Return vector-path boundary anchors for one object, or the ordinary
    /// recursive point array for a non-vector family target.
    pub fn path_boundary_points(&self) -> Result<Vec<(f64, f64)>, AuthoringError> {
        match self {
            Self::Object(_) => self.vector_boundary_points(),
            Self::Family(_) => self.all_points(),
        }
    }

    /// Return Manim's extrema over vector-path anchors for this object.
    pub fn path_boundary_extremum_along_dim(
        &self,
        dim: usize,
        key: f64,
    ) -> Result<f64, AuthoringError> {
        extremum_for_xy_points(&self.path_boundary_points()?, dim, key)
    }

    /// Return the vector-path boundary anchor with the greatest projection.
    pub fn path_boundary_point(
        &self,
        direction_x: f64,
        direction_y: f64,
    ) -> Result<(f64, f64), AuthoringError> {
        boundary_point_for_xy_points(&self.path_boundary_points()?, direction_x, direction_y)
    }

    /// Return a Manim-style min, midpoint, or max coordinate across this
    /// object's or family's boundary points.
    pub fn extremum_along_dim(&self, dim: usize, key: f64) -> Result<f64, AuthoringError> {
        extremum_for_xy_points(&self.all_points()?, dim, key)
    }

    /// Return the arithmetic mean of this object's or family's points.
    pub fn center_of_mass(&self) -> Result<(f64, f64), AuthoringError> {
        center_of_mass_for_xy_points(&self.all_points()?)
    }

    /// Return the boundary point with the greatest projection along a direction.
    pub fn boundary_point(
        &self,
        direction_x: f64,
        direction_y: f64,
    ) -> Result<(f64, f64), AuthoringError> {
        boundary_point_for_xy_points(&self.all_points()?, direction_x, direction_y)
    }

    /// Return the point extent along one coordinate dimension.
    pub fn length_over_dim(&self, dim: usize) -> Result<f64, AuthoringError> {
        length_over_xy_points(&self.all_points()?, dim)
    }
}

/// Observe a recursive point snapshot from one execution session, preserving
/// family order and aliases while reading each object's current effective path.
pub fn effective_point_reduction_tree(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
) -> Result<PointReductionTree, AuthoringError> {
    if !Rc::ptr_eq(store, target.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    target.validate()?;
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let topology = {
        let semantics = store.borrow();
        point_reduction_topology(&semantics, target.node_id())?
    };
    point_reduction_tree_from_topology(&topology, &mut |id| {
        let object = Mobject::from_node(Rc::clone(store), id)?;
        effective_path_query(store, execution, &object)?.points()
    })
}

/// Observe current effective points for all object occurrences below a target.
/// Membership is snapshotted once; each immutable path query reads the same
/// execution session and preserves Manim's recursive alias multiplicity.
pub fn effective_all_points(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
) -> Result<Vec<(f64, f64)>, AuthoringError> {
    effective_point_observation(
        store,
        execution,
        target,
        FamilyPointObservation::RecursivePoints,
    )
}

/// Observe vector-family anchors from one coherent publication in Manim's
/// unique family order. Each affected node is queried once; no host graph is used.
pub fn effective_vector_boundary_points(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
) -> Result<Vec<(f64, f64)>, AuthoringError> {
    effective_point_observation(
        store,
        execution,
        target,
        FamilyPointObservation::VectorBoundary,
    )
}

fn effective_point_observation(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
    observation: FamilyPointObservation,
) -> Result<Vec<(f64, f64)>, AuthoringError> {
    if !Rc::ptr_eq(store, target.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    target.validate()?;
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let occurrences = observation.nodes(&store.borrow(), target.node_id())?;
    let mut result = Vec::new();
    for id in occurrences {
        let object = Mobject::from_node(Rc::clone(store), id)?;
        result.extend(observation.points(&effective_path_query(store, execution, &object)?)?);
    }
    Ok(result)
}

/// Observe effective vector-path boundary anchors for an object. Family targets
/// keep Mobject's base behavior and return their recursive point arrays.
pub fn effective_path_boundary_points(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
) -> Result<Vec<(f64, f64)>, AuthoringError> {
    match target {
        MobjectTarget::Object(_) => effective_vector_boundary_points(store, execution, target),
        MobjectTarget::Family(_) => effective_all_points(store, execution, target),
    }
}

/// Observe one dimension's vector-path anchor extremum in the current execution.
pub fn effective_path_boundary_extremum_along_dim(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
    dim: usize,
    key: f64,
) -> Result<f64, AuthoringError> {
    extremum_for_xy_points(
        &effective_path_boundary_points(store, execution, target)?,
        dim,
        key,
    )
}

/// Observe the effective vector-path boundary anchor farthest along a direction.
pub fn effective_path_boundary_point(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
    direction_x: f64,
    direction_y: f64,
) -> Result<(f64, f64), AuthoringError> {
    boundary_point_for_xy_points(
        &effective_path_boundary_points(store, execution, target)?,
        direction_x,
        direction_y,
    )
}

/// Observe one dimension's Manim-style extremum from the current execution.
pub fn effective_extremum_along_dim(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
    dim: usize,
    key: f64,
) -> Result<f64, AuthoringError> {
    extremum_for_xy_points(&effective_all_points(store, execution, target)?, dim, key)
}

/// Observe the average point position from the current execution publication.
pub fn effective_center_of_mass(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
) -> Result<(f64, f64), AuthoringError> {
    center_of_mass_for_xy_points(&effective_all_points(store, execution, target)?)
}

/// Observe the current boundary point with the greatest directional projection.
pub fn effective_boundary_point(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
    direction_x: f64,
    direction_y: f64,
) -> Result<(f64, f64), AuthoringError> {
    boundary_point_for_xy_points(
        &effective_all_points(store, execution, target)?,
        direction_x,
        direction_y,
    )
}

/// Observe the current point extent along one coordinate dimension.
pub fn effective_length_over_dim(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
    dim: usize,
) -> Result<f64, AuthoringError> {
    length_over_xy_points(&effective_all_points(store, execution, target)?, dim)
}

/// Reduce caller-supplied flattened point rows in Rust, using Manim's key rule:
/// negative selects the minimum, zero the midpoint, and positive the maximum.
pub fn reduce_point_extremum(
    coordinates: &[f64],
    point_dimensions: usize,
    dim: usize,
    key: f64,
) -> Result<f64, AuthoringError> {
    if point_dimensions == 0 || dim >= point_dimensions {
        return Err(AuthoringError::InvalidPointDimension(dim as u32));
    }
    if !coordinates.len().is_multiple_of(point_dimensions) {
        return Err(AuthoringError::InvalidPointCoordinates(coordinates.len()));
    }
    let mut values = coordinates
        .chunks_exact(point_dimensions)
        .map(|row| row[dim]);
    let Some(first) = values.next() else {
        return Err(AuthoringError::PathQuery(PathProportionError::EmptyPath));
    };
    let mut minimum = first;
    let mut maximum = first;
    for value in values {
        if value.is_nan() {
            return Ok(f64::NAN);
        }
        minimum = minimum.min(value);
        maximum = maximum.max(value);
    }
    if first.is_nan() {
        return Ok(f64::NAN);
    }
    Ok(if key < 0.0 {
        minimum
    } else if key == 0.0 {
        (minimum + maximum) / 2.0
    } else {
        maximum
    })
}

/// Select the point with the greatest directional projection from flattened
/// caller-supplied point rows, using the shared Rust boundary reducer.
pub fn reduce_point_boundary(
    coordinates: &[f64],
    point_dimensions: usize,
    direction_x: f64,
    direction_y: f64,
) -> Result<(f64, f64), AuthoringError> {
    if point_dimensions < 2 {
        return Err(AuthoringError::InvalidPointDimension(
            point_dimensions as u32,
        ));
    }
    if !coordinates.len().is_multiple_of(point_dimensions) {
        return Err(AuthoringError::InvalidPointCoordinates(coordinates.len()));
    }
    let points = coordinates
        .chunks_exact(point_dimensions)
        .map(|row| (row[0], row[1]))
        .collect::<Vec<_>>();
    boundary_point_for_xy_points(&points, direction_x, direction_y)
}

/// Compare equal-length numeric arrays using NumPy's default `allclose`
/// tolerances used by ManimCE's VMobject point-equality helper.
pub fn manim_points_allclose(left: &[f64], right: &[f64], absolute_tolerance: f64) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(&a, &b)| manim_numpy_isclose(a, b, absolute_tolerance))
}

/// Return the complete point ranges used by ManimCE's cubic tuple helpers.
///
/// The helpers discard any trailing points that do not complete a group. A
/// non-positive stride produces no groups for negative values; Python raises
/// its native modulo-by-zero error before calling this function for zero.
pub fn manim_cubic_bezier_point_ranges(point_count: usize, points_per_curve: f64) -> Vec<usize> {
    if !points_per_curve.is_finite()
        || points_per_curve <= 0.0
        || points_per_curve.fract() != 0.0
        || points_per_curve > point_count as f64
    {
        return Vec::new();
    }
    let stride = points_per_curve as usize;
    let complete_point_count = point_count / stride * stride;
    (0..complete_point_count)
        .step_by(stride)
        .flat_map(|start| [start, start + stride])
        .collect()
}

/// Return ManimCE's point-index ranges for contiguous groups of cubic curves.
///
/// A new range starts at each four-point boundary whose preceding endpoint is
/// not NumPy-close to the next curve's start point. `compared_dimensions`
/// mirrors `points[:, :n_dims]`; it is clamped to the row width by the caller.
/// A trailing partial cubic remains in the final range, as in ManimCE.
pub fn manim_subpath_split_indices(
    points: &[f64],
    point_count: usize,
    point_dimensions: usize,
    compared_dimensions: usize,
    absolute_tolerance: f64,
) -> Result<Vec<usize>, AuthoringError> {
    manim_subpath_ranges(
        points,
        point_count,
        point_dimensions,
        compared_dimensions,
        4.0,
        absolute_tolerance,
        false,
    )
}

/// Return connected cubic subpath ranges at a caller-selected point stride.
///
/// When `drop_short_ranges` is true, ranges shorter than one cubic stride are
/// omitted, matching ManimCE's `_gen_subpaths_from_points` behavior.
pub fn manim_subpath_ranges(
    points: &[f64],
    point_count: usize,
    point_dimensions: usize,
    compared_dimensions: usize,
    points_per_curve: f64,
    absolute_tolerance: f64,
    drop_short_ranges: bool,
) -> Result<Vec<usize>, AuthoringError> {
    if point_dimensions > 0 && point_count.checked_mul(point_dimensions) != Some(points.len()) {
        return Err(AuthoringError::InvalidPointCoordinates(points.len()));
    }
    if point_dimensions == 0 && !points.is_empty() {
        return Err(AuthoringError::InvalidPointCoordinates(points.len()));
    }
    if points_per_curve.is_nan()
        || (points_per_curve.is_finite() && points_per_curve.fract() != 0.0)
        || points_per_curve == 0.0
    {
        return Ok(Vec::new());
    }
    if points_per_curve > 0.0 && !drop_short_ranges && (point_count as f64) < points_per_curve {
        return Ok(Vec::new());
    }

    let compared_dimensions = compared_dimensions.min(point_dimensions);
    let mut starts = vec![0];
    if compared_dimensions > 0 && points_per_curve > 0.0 && points_per_curve <= point_count as f64 {
        let stride = points_per_curve as usize;
        for boundary in (stride..point_count).step_by(stride) {
            let end_offset = (boundary - 1) * point_dimensions;
            let start_offset = boundary * point_dimensions;
            let previous_end = &points[end_offset..end_offset + compared_dimensions];
            let next_start = &points[start_offset..start_offset + compared_dimensions];
            if !manim_subpath_points_close(previous_end, next_start, absolute_tolerance) {
                starts.push(boundary);
            }
        }
    }
    starts.push(point_count);
    Ok(starts
        .windows(2)
        .filter(|range| !drop_short_ranges || (range[1] - range[0]) as f64 >= points_per_curve)
        .flat_map(|range| [range[0], range[1]])
        .collect())
}

fn manim_subpath_points_close(left: &[f64], right: &[f64], absolute_tolerance: f64) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(&a, &b)| {
            if a == b {
                return true;
            }
            let difference = (a - b).abs();
            difference <= absolute_tolerance + 1.0e-5 * b.abs() && difference.is_finite()
        })
}

/// Compare the two planar coordinates using ManimCE's explicit 2D helper.
/// Unlike NumPy's vectorized `allclose`, this implementation also rejects an
/// overflowing coordinate difference before applying the tolerance.
pub fn manim_points_2d_close(left: [f64; 2], right: [f64; 2], absolute_tolerance: f64) -> bool {
    left.into_iter().zip(right).all(|(a, b)| {
        if a == b {
            return true;
        }
        let difference = (a - b).abs();
        difference <= absolute_tolerance + 1.0e-5 * b.abs() && difference.is_finite()
    })
}

fn manim_numpy_isclose(a: f64, b: f64, absolute_tolerance: f64) -> bool {
    if a == b {
        return true;
    }
    b.is_finite() && (a - b).abs() <= absolute_tolerance + 1.0e-5 * b.abs()
}

fn extremum_for_xy_points(
    points: &[(f64, f64)],
    dim: usize,
    key: f64,
) -> Result<f64, AuthoringError> {
    if dim == 2 {
        return if points.is_empty() {
            Err(AuthoringError::PathQuery(PathProportionError::EmptyPath))
        } else {
            Ok(0.0)
        };
    }
    if dim > 2 {
        return Err(AuthoringError::InvalidPointDimension(dim as u32));
    }
    let coordinates = points.iter().flat_map(|&(x, y)| [x, y]).collect::<Vec<_>>();
    reduce_point_extremum(&coordinates, 2, dim, key)
}

fn center_of_mass_for_xy_points(points: &[(f64, f64)]) -> Result<(f64, f64), AuthoringError> {
    if points.is_empty() {
        return Ok((f64::NAN, f64::NAN));
    }
    let (sum_x, sum_y) = points
        .iter()
        .fold((0.0, 0.0), |(sum_x, sum_y), &(x, y)| (sum_x + x, sum_y + y));
    let count = points.len() as f64;
    Ok((sum_x / count, sum_y / count))
}

fn boundary_point_for_xy_points(
    points: &[(f64, f64)],
    direction_x: f64,
    direction_y: f64,
) -> Result<(f64, f64), AuthoringError> {
    let Some(&first) = points.first() else {
        return Err(AuthoringError::PathQuery(PathProportionError::EmptyPath));
    };
    let mut boundary = first;
    let mut projection = first.0 * direction_x + first.1 * direction_y;
    for &point in &points[1..] {
        let next_projection = point.0 * direction_x + point.1 * direction_y;
        if next_projection.is_nan() {
            if !projection.is_nan() {
                boundary = point;
                projection = next_projection;
            }
        } else if !projection.is_nan() && next_projection > projection {
            boundary = point;
            projection = next_projection;
        }
    }
    Ok(boundary)
}

fn length_over_xy_points(points: &[(f64, f64)], dim: usize) -> Result<f64, AuthoringError> {
    if points.is_empty() || dim == 2 {
        return Ok(0.0);
    }
    if dim > 2 {
        return Err(AuthoringError::InvalidPointDimension(dim as u32));
    }
    let minimum = extremum_for_xy_points(points, dim, -1.0)?;
    let maximum = extremum_for_xy_points(points, dim, 1.0)?;
    Ok(maximum - minimum)
}

fn effective_geometry_path(
    observed: &crate::execution_session::EffectiveSemanticObject<'_>,
) -> Result<VectorPath, AuthoringError> {
    let unsupported =
        || AuthoringError::Unsupported(UnsupportedAuthoringOperation::EffectivePathRenderOverride);
    let geometry = observed
        .render_geometry
        .or_else(|| observed.object.content.geometry())
        .ok_or(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::PathQueryContent,
        ))?;
    let mut path = noon_geometry::canonical_outline_path(geometry).ok_or(
        AuthoringError::Unsupported(UnsupportedAuthoringOperation::PathQueryContent),
    )?;
    if let Some(target) = path.morph_target() {
        path = noon_geometry::interpolate_revealed_path_preserving_order(
            &path,
            target,
            observed.morph,
            observed.reveal,
        )
        .map_err(AuthoringError::MorphQuery)?;
    } else if observed.morph != 0.0 {
        return Err(unsupported());
    } else if observed.reveal != 1.0 {
        path = noon_geometry::authored_partial_path(&path, 0.0, observed.reveal);
    }
    Ok(path)
}

/// Observe one object's exact current path from an existing coherent execution.
///
/// This is explicit low-level integration over the existing Semantic Scene and Runtime
/// authorities. It owns no scene/session state and performs no publication.
pub fn effective_path_query(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    object: &Mobject,
) -> Result<PathQuery, AuthoringError> {
    if !Rc::ptr_eq(store, object.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    object.validate()?;
    let store = store.borrow();
    let observed = execution
        .effective_semantic_object(&store, object.node_id())
        .map_err(AuthoringError::from)?;
    let path = effective_geometry_path(&observed)?;
    let state = store
        .semantic_object_state_checked(object.node_id())
        .map_err(AuthoringError::from)?;
    let transform = crate::semantic_mobject::semantic_transform_with_effective_affine(
        state
            .transform
            .as_planar()
            .ok_or(AuthoringError::NonFiniteObjectState)?,
        observed
            .render_transform
            .unwrap_or(observed.object.transform),
    );
    PathQuery::prepare(&path, transform)
}

impl Mobject {
    /// Prepare a query over immutable local/content-space geometry.
    pub fn local_path_query(&self) -> Result<PathQuery, AuthoringError> {
        let store = self.integration_store().borrow();
        let state = store
            .semantic_object_state_checked(self.node_id())
            .map_err(AuthoringError::from)?;
        prepare_content(&store, state.content, SemanticTransform2_5D::default())
    }

    /// Prepare a query over authored world-space geometry. During execution,
    /// use [`crate::Scene::effective_path_query`] for the current Runtime publication.
    pub fn path_query(&self) -> Result<PathQuery, AuthoringError> {
        let store = self.integration_store().borrow();
        let state = store
            .semantic_object_state_checked(self.node_id())
            .map_err(AuthoringError::from)?;
        prepare_content(
            &store,
            state.content,
            state
                .transform
                .as_planar()
                .ok_or(AuthoringError::NonFiniteObjectState)?,
        )
    }
}

#[cfg(test)]
mod point_equality_tests {
    use super::*;

    #[test]
    fn point_equality_matches_manim_absolute_relative_and_nonfinite_rules() {
        assert!(manim_points_allclose(&[0.0, 2.0], &[5.0e-7, 2.0], 1.0e-6));
        assert!(!manim_points_allclose(&[0.0], &[2.0e-6], 1.0e-6));
        assert!(manim_points_allclose(&[100_000.0], &[100_001.0], 1.0e-6));
        assert!(manim_points_allclose(
            &[f64::INFINITY],
            &[f64::INFINITY],
            1.0e-6
        ));
        assert!(!manim_points_allclose(&[f64::NAN], &[f64::NAN], 1.0e-6));
        assert!(manim_points_allclose(
            &[f64::INFINITY],
            &[1.0],
            f64::INFINITY
        ));

        assert!(manim_points_2d_close([0.0, 1.0], [5.0e-7, 1.0], 1.0e-6));
        assert!(!manim_points_2d_close(
            [f64::NAN, 1.0],
            [f64::NAN, 1.0],
            1.0e-6
        ));
        assert!(manim_points_2d_close(
            [f64::INFINITY, 1.0],
            [f64::INFINITY, 1.0],
            1.0e-6,
        ));
        assert!(!manim_points_2d_close(
            [f64::MAX, 1.0],
            [-f64::MAX, 1.0],
            f64::INFINITY,
        ));
    }

    #[test]
    fn subpath_ranges_match_manim_curve_boundaries_and_dimensions() {
        let points = [
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0, 3.0000005, 0.0, 0.0, 4.0,
            0.0, 0.0, 5.0, 0.0, 0.0, 6.0, 0.0, 0.0, 6.0, 0.0, 0.1, 7.0, 0.0, 0.1, 8.0, 0.0, 0.1,
            9.0, 0.0, 0.1, 9.0, 0.0, 0.1, 10.0, 0.0, 0.1, 11.0, 0.0, 0.1, 12.0, 0.0, 0.1,
        ];
        assert_eq!(
            manim_subpath_split_indices(&points, 16, 3, 3, 1.0e-6).unwrap(),
            [0, 8, 8, 16]
        );
        assert_eq!(
            manim_subpath_split_indices(&points, 16, 3, 2, 1.0e-6).unwrap(),
            [0, 16]
        );
    }

    #[test]
    fn cubic_bezier_tuple_ranges_group_complete_strides_and_drop_remainders() {
        assert_eq!(manim_cubic_bezier_point_ranges(10, 4.0), [0, 4, 4, 8]);
        assert_eq!(manim_cubic_bezier_point_ranges(10, 3.0), [0, 3, 3, 6, 6, 9]);
        assert!(manim_cubic_bezier_point_ranges(3, 4.0).is_empty());
        assert!(manim_cubic_bezier_point_ranges(10, -4.0).is_empty());
        assert!(manim_cubic_bezier_point_ranges(10, 0.0).is_empty());
    }

    #[test]
    fn subpath_ranges_preserve_nonfinite_and_trailing_point_behavior() {
        let points = [
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            2.0,
            0.0,
            0.0,
            3.0,
            0.0,
            f64::NAN,
            3.0,
            0.0,
            f64::NAN,
            4.0,
            0.0,
            0.0,
            5.0,
            0.0,
            0.0,
            6.0,
            0.0,
            0.0,
            6.0,
            0.0,
            0.0,
            7.0,
            0.0,
            0.0,
            8.0,
            0.0,
            0.0,
            9.0,
            0.0,
            0.0,
            20.0,
            0.0,
            0.0,
        ];
        assert_eq!(
            manim_subpath_split_indices(&points, 13, 3, 3, 1.0e-6).unwrap(),
            [0, 4, 4, 12, 12, 13]
        );
        assert!(manim_subpath_split_indices(&[], 3, 0, 3, 1.0e-6)
            .unwrap()
            .is_empty());
        assert_eq!(
            manim_subpath_split_indices(&[], 4, 0, 3, 1.0e-6).unwrap(),
            [0, 4]
        );
        assert!(manim_subpath_split_indices(&[1.0], 2, 3, 2, 1.0e-6).is_err());
        assert_eq!(
            manim_subpath_split_indices(
                &[-1.0, 0.0, 0.0, -1.7e308, 1.7e308, 0.0, 0.0, 0.0],
                8,
                1,
                1,
                f64::INFINITY,
            )
            .unwrap(),
            [0, 4, 4, 8]
        );
    }

    #[test]
    fn generic_subpath_ranges_honor_stride_dimensions_and_short_run_filtering() {
        let points = [
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0, 3.0, 0.0, 0.0, 4.0, 0.0,
            0.0, 5.0, 0.0, 0.0, 6.0, 0.0, 0.0, 6.0, 0.0, 1.0, 7.0, 0.0, 1.0, 8.0, 0.0, 1.0, 9.0,
            0.0, 1.0, 10.0, 0.0, 2.0,
        ];
        assert_eq!(
            manim_subpath_ranges(&points, 13, 3, 3, 4.0, 1.0e-6, true).unwrap(),
            [0, 8, 8, 12]
        );
        assert_eq!(
            manim_subpath_ranges(&points, 13, 3, 2, 4.0, 1.0e-6, true).unwrap(),
            [0, 12]
        );
        assert_eq!(
            manim_subpath_ranges(&points, 13, 3, 3, 3.0, 1.0e-6, false).unwrap(),
            [0, 3, 3, 6, 6, 9, 9, 12, 12, 13]
        );
        assert_eq!(
            manim_subpath_ranges(&points, 13, 3, 3, f64::NEG_INFINITY, 1.0e-6, true).unwrap(),
            [0, 13]
        );
        assert!(manim_subpath_ranges(&points, 13, 3, 3, 3.5, 1.0e-6, true)
            .unwrap()
            .is_empty());
    }
}
