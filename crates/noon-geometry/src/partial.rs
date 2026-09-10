use noon_core::{PathCommand, SemanticVec3, Vec2, VectorPath};

const MANIM_LENGTH_SAMPLE_POINTS: usize = 10;

#[derive(Clone, Copy, Debug)]
enum CurveKind {
    Line,
    Quadratic { control: Vec2 },
    Cubic { control1: Vec2, control2: Vec2 },
    Close,
}

#[derive(Clone, Copy, Debug)]
struct Curve {
    from: Vec2,
    to: Vec2,
    kind: CurveKind,
    subpath: usize,
    closes_contour: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathProportionError {
    InvalidProportion(f32),
    EmptyPath,
    InvalidMetric,
    InvalidSampleCount,
    InvalidInterval,
    InvalidCurveCount,
    InvalidCurveIndex { index: usize, count: usize },
}

impl std::fmt::Display for PathProportionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidProportion(alpha) => {
                write!(
                    formatter,
                    "path proportion must be finite and between 0 and 1: {alpha}"
                )
            }
            Self::EmptyPath => formatter.write_str("path has no drawable curves"),
            Self::InvalidMetric => {
                formatter.write_str("path metric must have finite coordinates, scales and length")
            }
            Self::InvalidCurveCount => {
                formatter.write_str("requested curve count exceeds addressable capacity")
            }
            Self::InvalidCurveIndex { index, count } => write!(
                formatter,
                "curve index {index} is out of bounds for {count} curves"
            ),
            Self::InvalidInterval => {
                formatter.write_str("partial path requires an ordered interval: a <= b")
            }
            Self::InvalidSampleCount => {
                formatter.write_str("path length sample count must be at least two")
            }
        }
    }
}

impl std::error::Error for PathProportionError {}

fn validate_proportion(alpha: f32) -> Result<(), PathProportionError> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(PathProportionError::InvalidProportion(alpha));
    }
    Ok(())
}

fn validate_proportion_f64(alpha: f64) -> Result<(), PathProportionError> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(PathProportionError::InvalidProportion(alpha as f32));
    }
    Ok(())
}

/// Reusable ManimCE v0.21 path-proportion measure for repeated point queries.
///
/// Construction collects drawable curves and performs Manim's ten-sample Bezier
/// length approximation exactly once. [`Self::point`] then reuses those immutable
/// sampled lengths, avoiding repeated curve collection, allocation, and length
/// sampling during animation playback. Finite paths additionally keep cumulative
/// curve lengths so each point query selects its curve in logarithmic time.
#[derive(Clone, Debug)]
pub struct PathProportionPlan {
    curves: Vec<Curve>,
    lengths: Vec<f64>,
    cumulative_lengths: Vec<f64>,
    total_length: f64,
    scale: (f64, f64),
}

impl PathProportionPlan {
    pub fn curve_count(&self) -> usize {
        self.curves.len()
    }

    /// Cubic controls for one retained curve, promoting lines and quadratics
    /// exactly. Points remain in local content space regardless of query metric.
    pub fn curve_points(&self, index: usize) -> Result<[SemanticVec3; 4], PathProportionError> {
        let curve = self
            .curves
            .get(index)
            .ok_or(PathProportionError::InvalidCurveIndex {
                index,
                count: self.curves.len(),
            })?;
        let from = SemanticVec3::from_vec2(curve.from);
        let to = SemanticVec3::from_vec2(curve.to);
        let (first, second) = match curve.kind {
            CurveKind::Line | CurveKind::Close => {
                (lerp_f64(from, to, 1. / 3.), lerp_f64(from, to, 2. / 3.))
            }
            CurveKind::Quadratic { control } => {
                let control = SemanticVec3::from_vec2(control);
                (
                    lerp_f64(from, control, 2. / 3.),
                    lerp_f64(to, control, 2. / 3.),
                )
            }
            CurveKind::Cubic { control1, control2 } => (
                SemanticVec3::from_vec2(control1),
                SemanticVec3::from_vec2(control2),
            ),
        };
        Ok([from, first, second, to])
    }

    /// Prepare the reusable local-space proportion measure for `path`.
    pub fn new(path: &VectorPath) -> Result<Self, PathProportionError> {
        Self::with_scale(path, (1.0, 1.0))
    }

    /// Prepare a measure in the metric induced by a planar scale.
    /// Rotation and translation preserve lengths; nonuniform scale changes
    /// which curve contains a given proportion. Points remain in local space.
    pub fn with_scale(path: &VectorPath, scale: (f64, f64)) -> Result<Self, PathProportionError> {
        if !scale.0.is_finite() || !scale.1.is_finite() {
            return Err(PathProportionError::InvalidMetric);
        }
        let curves = collect_curves(path);
        if curves.is_empty() {
            return Err(PathProportionError::EmptyPath);
        }
        let lengths: Vec<_> = curves
            .iter()
            .map(|&curve| sampled_curve_length(curve, scale, MANIM_LENGTH_SAMPLE_POINTS))
            .collect();
        let mut cumulative_lengths = Vec::with_capacity(lengths.len());
        let mut total_length = 0.0;
        for &length in &lengths {
            total_length += length;
            cumulative_lengths.push(total_length);
        }
        if !total_length.is_finite() {
            return Err(PathProportionError::InvalidMetric);
        }
        Ok(Self {
            curves,
            lengths,
            cumulative_lengths,
            total_length,
            scale,
        })
    }

    /// Return a local point using this plan's metric and renderer precision.
    pub fn point(&self, alpha: f32) -> Result<Vec2, PathProportionError> {
        let point = self.point_f64(f64::from(alpha))?;
        Ok(Vec2::new(point.x as f32, point.y as f32))
    }

    /// Return a local point without quantizing proportion or interpolation.
    pub fn point_f64(&self, alpha: f64) -> Result<SemanticVec3, PathProportionError> {
        validate_proportion_f64(alpha)?;
        if alpha == 1.0 {
            return Ok(SemanticVec3::from_vec2(self.curves.last().unwrap().to));
        }
        let target = alpha * self.total_length;
        let index = self
            .cumulative_lengths
            .partition_point(|&end| end < target)
            .min(self.curves.len() - 1);
        let previous = index
            .checked_sub(1)
            .map_or(0.0, |i| self.cumulative_lengths[i]);
        let residue = if self.lengths[index] > 0.0 {
            ((target - previous) / self.lengths[index]).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Ok(curve_point_f64(self.curves[index], residue))
    }

    /// Approximate arc length in this plan's metric. The default ten-sample
    /// measure is retained; an explicit sample count only visits this path.
    pub fn arc_length(&self, sample_points: Option<usize>) -> Result<f64, PathProportionError> {
        let samples = sample_points.unwrap_or(MANIM_LENGTH_SAMPLE_POINTS);
        if samples < 2 {
            return Err(PathProportionError::InvalidSampleCount);
        }
        if samples == MANIM_LENGTH_SAMPLE_POINTS {
            return Ok(self.total_length);
        }
        let length: f64 = self
            .curves
            .iter()
            .map(|&curve| sampled_curve_length(curve, self.scale, samples))
            .sum();
        if !length.is_finite() {
            return Err(PathProportionError::InvalidMetric);
        }
        Ok(length)
    }
}

/// Return the point at `alpha` using ManimCE v0.21's `VMobject.point_from_proportion` measure.
///
/// Manim approximates every Bezier curve length from ten uniformly spaced parameter
/// samples, uses those approximate lengths to choose a curve, then evaluates the chosen
/// curve at the residual length fraction. The residual is intentionally a Bezier
/// parameter rather than an arc-length inversion. Keeping this separate from
/// [`pointwise_partial_path`] matters because Create/partial paths use uniform curve-count
/// progress instead of this length-weighted measure.
///
/// Repeated callers should prepare a [`PathProportionPlan`] once and reuse it.
pub fn point_from_proportion(path: &VectorPath, alpha: f32) -> Result<Vec2, PathProportionError> {
    validate_proportion(alpha)?;
    PathProportionPlan::new(path)?.point(alpha)
}

/// High-precision authoring variant of [`point_from_proportion`].
///
/// This preserves f64 proportions and Bezier interpolation while reusing the
/// exact same prepared Manim sampled-length measure. It is intended for
/// precision-sensitive authoring operations such as finite-difference tangents;
/// retained renderer geometry can be lowered to f32 after those semantic
/// decisions are complete.
pub fn point_from_proportion_f64(
    path: &VectorPath,
    alpha: f64,
) -> Result<SemanticVec3, PathProportionError> {
    validate_proportion_f64(alpha)?;
    PathProportionPlan::new(path)?.point_f64(alpha)
}

/// Return the portion of a vector path whose global Bezier parameter lies in `[a, b]`.
///
/// ManimCE's `VMobject.pointwise_become_partial` divides `[0, 1]` uniformly by
/// Bezier curve count and then uses the local Bezier parameter inside the two
/// boundary curves. It does *not* use arc length. This function mirrors that
/// contract while preserving explicit Noon subpath breaks.
pub fn pointwise_partial_path(path: &VectorPath, a: f32, b: f32) -> VectorPath {
    partial_path(path, a, b, false)
}

/// Persistent partial geometry retains degenerate boundary curves, because later
/// curve-count selection observes them. Render-only reveal may omit those curves.
pub fn authored_partial_path(path: &VectorPath, a: f32, b: f32) -> VectorPath {
    partial_path(path, a, b, true)
}

fn partial_path(path: &VectorPath, a: f32, b: f32, retain_boundary_curves: bool) -> VectorPath {
    let a = a.clamp(0.0, 1.0);
    let b = b.clamp(a, 1.0);
    if a <= 0.0 && b >= 1.0 {
        return path.clone();
    }

    let curves = collect_curves(path);
    if curves.is_empty() {
        return VectorPath::new();
    }

    let (lower_index, lower_t) = integer_interpolate(curves.len(), a);
    let (upper_index, upper_t) = integer_interpolate(curves.len(), b);

    if b <= a && !retain_boundary_curves {
        let point = curve_point(curves[lower_index], lower_t);
        return VectorPath::new().move_to(point);
    }

    let mut result = VectorPath::new();
    let mut active_subpath = None;
    for (index, &curve) in curves
        .iter()
        .enumerate()
        .take(upper_index + 1)
        .skip(lower_index)
    {
        let t0 = if index == lower_index { lower_t } else { 0.0 };
        let t1 = if index == upper_index { upper_t } else { 1.0 };
        if t1 <= t0 && !retain_boundary_curves {
            continue;
        }

        let partial = partial_curve(curve, t0, t1);
        if active_subpath != Some(curve.subpath) {
            result = result.move_to(partial.from);
            active_subpath = Some(curve.subpath);
        }

        result = append_curve(result, partial);
    }
    result
}

/// Insert exactly `additional` curves by evenly subdividing existing curves.
/// Distribution matches Manim's integer repeat-index mapping, independent of
/// geometric length. Explicit breaks, closed joins and a trailing anchor survive.
pub fn subdivide_path(
    path: &VectorPath,
    additional: usize,
) -> Result<VectorPath, PathProportionError> {
    if additional == 0 {
        return Ok(path.clone());
    }
    let curves = collect_curves(path);
    if curves.is_empty() {
        if let [PathCommand::MoveTo { to }] = path.commands() {
            let mut result = VectorPath::new().move_to(*to);
            for _ in 0..additional {
                result = result.line_to(*to);
            }
            return Ok(result.move_to(*to));
        }
        return Err(PathProportionError::EmptyPath);
    }
    let total = curves
        .len()
        .checked_add(additional)
        .ok_or(PathProportionError::InvalidCurveCount)?;
    let mut result = VectorPath::new();
    let mut active_subpath = None;
    for (index, &curve) in curves.iter().enumerate() {
        if active_subpath != Some(curve.subpath) {
            result = result.move_to(curve.from);
            active_subpath = Some(curve.subpath);
        }
        // u128 prevents multiplication overflow on both native and WASM.
        let boundary =
            |i: usize| (i as u128 * total as u128).div_ceil(curves.len() as u128) as usize;
        let count = boundary(index + 1) - boundary(index);
        for part in 0..count {
            let selected = partial_curve(
                curve,
                part as f32 / count as f32,
                (part + 1) as f32 / count as f32,
            );
            result = append_curve(result, selected);
        }
        if curve.closes_contour {
            result = result.close();
        }
    }
    if let Some(PathCommand::MoveTo { to }) = path.commands().last() {
        result = result.move_to(*to);
    }
    Ok(result)
}

fn append_curve(path: VectorPath, curve: Curve) -> VectorPath {
    match curve.kind {
        CurveKind::Line | CurveKind::Close => path.line_to(curve.to),
        CurveKind::Quadratic { control } => path.quadratic_to(control, curve.to),
        CurveKind::Cubic { control1, control2 } => path.cubic_to(control1, control2, curve.to),
    }
}

fn integer_interpolate(curve_count: usize, alpha: f32) -> (usize, f32) {
    debug_assert!(curve_count > 0);
    if alpha >= 1.0 {
        return (curve_count - 1, 1.0);
    }
    let scaled = alpha.max(0.0) * curve_count as f32;
    let index = (scaled.floor() as usize).min(curve_count - 1);
    (index, scaled - index as f32)
}

fn collect_curves(path: &VectorPath) -> Vec<Curve> {
    let mut curves = Vec::new();
    let mut current = None;
    let mut subpath_start = None;
    let mut subpath = 0usize;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                current = Some(to);
                subpath_start = Some(to);
                if !curves.is_empty() {
                    subpath += 1;
                }
            }
            PathCommand::LineTo { to } => {
                if let Some(from) = current {
                    curves.push(Curve {
                        from,
                        to,
                        kind: CurveKind::Line,
                        subpath,
                        closes_contour: false,
                    });
                }
                current = Some(to);
            }
            PathCommand::QuadraticTo { control, to } => {
                if let Some(from) = current {
                    curves.push(Curve {
                        from,
                        to,
                        kind: CurveKind::Quadratic { control },
                        subpath,
                        closes_contour: false,
                    });
                }
                current = Some(to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                if let Some(from) = current {
                    curves.push(Curve {
                        from,
                        to,
                        kind: CurveKind::Cubic { control1, control2 },
                        subpath,
                        closes_contour: false,
                    });
                }
                current = Some(to);
            }
            PathCommand::Close => {
                if let (Some(from), Some(to)) = (current, subpath_start) {
                    if (from - to).length() > f32::EPSILON {
                        curves.push(Curve {
                            from,
                            to,
                            kind: CurveKind::Close,
                            subpath,
                            closes_contour: false,
                        });
                    }
                    if let Some(last) = curves.last_mut().filter(|curve| curve.subpath == subpath) {
                        last.closes_contour = true;
                    }
                    current = Some(to);
                }
            }
        }
    }
    curves
}

fn sampled_curve_length(curve: Curve, scale: (f64, f64), samples: usize) -> f64 {
    let denominator = (samples - 1) as f64;
    let mut previous = curve_point_f64(curve, 0.0);
    let mut length = 0.0;
    for sample in 1..samples {
        let point = curve_point_f64(curve, sample as f64 / denominator);
        length += ((point.x - previous.x) * scale.0).hypot((point.y - previous.y) * scale.1);
        previous = point;
    }
    length
}

fn curve_point(curve: Curve, t: f32) -> Vec2 {
    match curve.kind {
        CurveKind::Line | CurveKind::Close => lerp(curve.from, curve.to, t),
        CurveKind::Quadratic { control } => {
            let p01 = lerp(curve.from, control, t);
            let p12 = lerp(control, curve.to, t);
            lerp(p01, p12, t)
        }
        CurveKind::Cubic { control1, control2 } => {
            let p01 = lerp(curve.from, control1, t);
            let p12 = lerp(control1, control2, t);
            let p23 = lerp(control2, curve.to, t);
            let p012 = lerp(p01, p12, t);
            let p123 = lerp(p12, p23, t);
            lerp(p012, p123, t)
        }
    }
}

fn curve_point_f64(curve: Curve, t: f64) -> SemanticVec3 {
    let from = SemanticVec3::from_vec2(curve.from);
    let to = SemanticVec3::from_vec2(curve.to);
    match curve.kind {
        CurveKind::Line | CurveKind::Close => lerp_f64(from, to, t),
        CurveKind::Quadratic { control } => {
            let control = SemanticVec3::from_vec2(control);
            let p01 = lerp_f64(from, control, t);
            let p12 = lerp_f64(control, to, t);
            lerp_f64(p01, p12, t)
        }
        CurveKind::Cubic { control1, control2 } => {
            let control1 = SemanticVec3::from_vec2(control1);
            let control2 = SemanticVec3::from_vec2(control2);
            let p01 = lerp_f64(from, control1, t);
            let p12 = lerp_f64(control1, control2, t);
            let p23 = lerp_f64(control2, to, t);
            let p012 = lerp_f64(p01, p12, t);
            let p123 = lerp_f64(p12, p23, t);
            lerp_f64(p012, p123, t)
        }
    }
}

fn partial_curve(curve: Curve, a: f32, b: f32) -> Curve {
    match curve.kind {
        CurveKind::Line | CurveKind::Close => Curve {
            from: lerp(curve.from, curve.to, a),
            to: lerp(curve.from, curve.to, b),
            kind: curve.kind,
            subpath: curve.subpath,
            closes_contour: curve.closes_contour,
        },
        CurveKind::Quadratic { control } => {
            let points = partial_quadratic([curve.from, control, curve.to], a, b);
            Curve {
                from: points[0],
                to: points[2],
                kind: CurveKind::Quadratic { control: points[1] },
                subpath: curve.subpath,
                closes_contour: curve.closes_contour,
            }
        }
        CurveKind::Cubic { control1, control2 } => {
            let points = partial_cubic([curve.from, control1, control2, curve.to], a, b);
            Curve {
                from: points[0],
                to: points[3],
                kind: CurveKind::Cubic {
                    control1: points[1],
                    control2: points[2],
                },
                subpath: curve.subpath,
                closes_contour: curve.closes_contour,
            }
        }
    }
}

fn partial_quadratic(points: [Vec2; 3], a: f32, b: f32) -> [Vec2; 3] {
    let (_, right) = split_quadratic(points, a);
    if a >= 1.0 {
        return [points[2]; 3];
    }
    let local_b = ((b - a) / (1.0 - a)).clamp(0.0, 1.0);
    split_quadratic(right, local_b).0
}

fn split_quadratic(points: [Vec2; 3], t: f32) -> ([Vec2; 3], [Vec2; 3]) {
    let p01 = lerp(points[0], points[1], t);
    let p12 = lerp(points[1], points[2], t);
    let p012 = lerp(p01, p12, t);
    ([points[0], p01, p012], [p012, p12, points[2]])
}

fn partial_cubic(points: [Vec2; 4], a: f32, b: f32) -> [Vec2; 4] {
    let (_, right) = split_cubic(points, a);
    if a >= 1.0 {
        return [points[3]; 4];
    }
    let local_b = ((b - a) / (1.0 - a)).clamp(0.0, 1.0);
    split_cubic(right, local_b).0
}

fn split_cubic(points: [Vec2; 4], t: f32) -> ([Vec2; 4], [Vec2; 4]) {
    let p01 = lerp(points[0], points[1], t);
    let p12 = lerp(points[1], points[2], t);
    let p23 = lerp(points[2], points[3], t);
    let p012 = lerp(p01, p12, t);
    let p123 = lerp(p12, p23, t);
    let p0123 = lerp(p012, p123, t);
    ([points[0], p01, p012, p0123], [p0123, p123, p23, points[3]])
}

fn lerp(a: Vec2, b: Vec2, t: f32) -> Vec2 {
    a + (b - a) * t
}

fn lerp_f64(a: SemanticVec3, b: SemanticVec3, t: f64) -> SemanticVec3 {
    SemanticVec3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec2(actual: Vec2, expected: Vec2) {
        assert!(
            (actual.x - expected.x).abs() < 1e-5,
            "x: {actual:?} != {expected:?}"
        );
        assert!(
            (actual.y - expected.y).abs() < 1e-5,
            "y: {actual:?} != {expected:?}"
        );
    }

    fn assert_semantic(actual: SemanticVec3, expected: SemanticVec3, tolerance: f64) {
        assert!(
            (actual.x - expected.x).abs() <= tolerance
                && (actual.y - expected.y).abs() <= tolerance
                && (actual.z - expected.z).abs() <= tolerance,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn proportion_uses_curve_length_weighting_not_curve_count() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(3.0, 0.0))
            .line_to(Vec2::new(3.0, 1.0));
        assert_vec2(
            point_from_proportion(&path, 0.5).unwrap(),
            Vec2::new(2.0, 0.0),
        );
    }

    #[test]
    fn proportion_uses_residual_as_local_bezier_parameter() {
        let path = VectorPath::new().move_to(Vec2::ZERO).cubic_to(
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        );
        assert_vec2(
            point_from_proportion(&path, 0.25).unwrap(),
            Vec2::new(0.15625, 0.5625),
        );
    }

    #[test]
    fn prepared_proportion_plan_reuses_the_sampled_measure() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(3.0, 0.0))
            .line_to(Vec2::new(3.0, 1.0));
        let plan = PathProportionPlan::new(&path).unwrap();

        assert_eq!(plan.curves.len(), 2);
        assert_eq!(plan.lengths, vec![3.0, 1.0]);
        assert_eq!(plan.cumulative_lengths, vec![3.0, 4.0]);
        assert_eq!(plan.total_length, 4.0);
        assert_vec2(plan.point(0.25).unwrap(), Vec2::new(1.0, 0.0));
        assert_vec2(plan.point(0.5).unwrap(), Vec2::new(2.0, 0.0));
        assert_vec2(plan.point(0.875).unwrap(), Vec2::new(3.0, 0.5));
    }

    #[test]
    fn precise_plan_reuses_measure_without_quantizing_alpha() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .quadratic_to(Vec2::new(1.0, 2.0), Vec2::new(2.0, 0.0));
        let plan = PathProportionPlan::new(&path).unwrap();
        let lower_alpha = 0.4 - 1.0e-9;
        let upper_alpha = 0.4 + 1.0e-9;

        assert_eq!(lower_alpha as f32, upper_alpha as f32);
        let lower = plan.point_f64(lower_alpha).unwrap();
        let upper = plan.point_f64(upper_alpha).unwrap();
        assert!((upper.x - lower.x).hypot(upper.y - lower.y) > 1.0e-9);

        let ordinary = plan.point(0.4).unwrap();
        let precise = plan.point_f64(0.4).unwrap();
        assert_semantic(precise, SemanticVec3::from_vec2(ordinary), 2.0e-7);
    }

    #[test]
    fn prepared_proportion_plan_preserves_first_curve_at_exact_length_boundary() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(3.0, 0.0))
            .line_to(Vec2::new(3.0, 1.0));
        let plan = PathProportionPlan::new(&path).unwrap();

        assert_vec2(plan.point(0.75).unwrap(), Vec2::new(3.0, 0.0));
        assert_eq!(
            plan.cumulative_lengths
                .partition_point(|&end_length| end_length < 3.0),
            0
        );
    }

    #[test]
    fn prepared_proportion_plan_owns_its_curve_measure() {
        let mut path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(2.0, 0.0));
        let plan = PathProportionPlan::new(&path).unwrap();

        path = path.line_to(Vec2::new(2.0, 8.0));

        assert_vec2(plan.point(0.5).unwrap(), Vec2::new(1.0, 0.0));
        assert_vec2(
            point_from_proportion(&path, 0.5).unwrap(),
            Vec2::new(2.0, 3.0),
        );
    }

    #[test]
    fn proportion_preserves_exact_endpoints_and_rejects_invalid_inputs() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 2.0))
            .line_to(Vec2::new(4.0, -3.0));
        assert_vec2(
            point_from_proportion(&path, 0.0).unwrap(),
            Vec2::new(-1.0, 2.0),
        );
        assert_vec2(
            point_from_proportion(&path, 1.0).unwrap(),
            Vec2::new(4.0, -3.0),
        );
        assert_eq!(
            point_from_proportion(&path, -0.01),
            Err(PathProportionError::InvalidProportion(-0.01))
        );
        assert!(matches!(
            point_from_proportion(&path, f32::NAN),
            Err(PathProportionError::InvalidProportion(alpha)) if alpha.is_nan()
        ));
        assert_eq!(
            point_from_proportion(&VectorPath::new(), 0.5),
            Err(PathProportionError::EmptyPath)
        );
        assert_eq!(
            point_from_proportion(&VectorPath::new(), -0.01),
            Err(PathProportionError::InvalidProportion(-0.01))
        );
        assert_eq!(
            PathProportionPlan::new(&VectorPath::new()).map(|_| ()),
            Err(PathProportionError::EmptyPath)
        );
        let plan = PathProportionPlan::new(&path).unwrap();
        assert_eq!(
            plan.point(-0.01),
            Err(PathProportionError::InvalidProportion(-0.01))
        );
        assert!(matches!(
            plan.point(f32::NAN),
            Err(PathProportionError::InvalidProportion(alpha)) if alpha.is_nan()
        ));
        assert!(matches!(
            plan.point_f64(f64::NAN),
            Err(PathProportionError::InvalidProportion(alpha)) if alpha.is_nan()
        ));
    }

    #[test]
    fn line_half_is_first_half() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0));
        let partial = pointwise_partial_path(&path, 0.0, 0.5);
        assert_eq!(partial.commands().len(), 2);
        match partial.commands()[0] {
            PathCommand::MoveTo { to } => assert_vec2(to, Vec2::new(-1.0, 0.0)),
            other => panic!("unexpected command: {other:?}"),
        }
        match partial.commands()[1] {
            PathCommand::LineTo { to } => assert_vec2(to, Vec2::ZERO),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn curve_count_not_arc_length_selects_boundary_curve() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(100.0, 0.0))
            .line_to(Vec2::new(100.0, 1.0));
        let partial = pointwise_partial_path(&path, 0.0, 0.5);
        match partial.commands().last().unwrap() {
            PathCommand::LineTo { to } => assert_vec2(*to, Vec2::new(100.0, 0.0)),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn zero_width_partial_collapses_to_one_point() {
        let path = VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE);
        let partial = pointwise_partial_path(&path, 0.25, 0.25);
        assert_eq!(partial.commands().len(), 1);
    }

    #[test]
    fn full_partial_preserves_close_and_exact_commands() {
        let path = VectorPath::new()
            .move_to(Vec2::new(1.0, 1.0))
            .line_to(Vec2::new(-1.0, 1.0))
            .line_to(Vec2::new(-1.0, -1.0))
            .close();
        assert_eq!(pointwise_partial_path(&path, 0.0, 1.0), path);
    }

    #[test]
    fn cubic_partial_end_matches_original_curve() {
        let path = VectorPath::new().move_to(Vec2::ZERO).cubic_to(
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::ONE,
        );
        let partial = pointwise_partial_path(&path, 0.0, 0.5);
        match partial.commands().last().unwrap() {
            PathCommand::CubicTo { to, .. } => assert_vec2(*to, Vec2::new(0.5, 0.875)),
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
