use noon_core::{GeometryRef, PathCommand, Vec2, VectorPath, TAU};

use crate::{point_from_proportion, PathProportionError};

const MANIM_CIRCLE_COMPONENTS: usize = 9;
const MANIM_LENGTH_SAMPLE_POINTS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeometryProportionError {
    Path(PathProportionError),
    InvalidHighPrecisionProportion(f64),
    EmptyPath,
    UnsupportedGeometry,
}

impl std::fmt::Display for GeometryProportionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(error) => error.fmt(formatter),
            Self::InvalidHighPrecisionProportion(alpha) => write!(
                formatter,
                "path proportion must be finite and between 0 and 1: {alpha}"
            ),
            Self::EmptyPath => formatter.write_str("path has no drawable curves"),
            Self::UnsupportedGeometry => formatter.write_str(
                "point_from_proportion requires retained circle, line, or vector-path geometry",
            ),
        }
    }
}

impl std::error::Error for GeometryProportionError {}

impl From<PathProportionError> for GeometryProportionError {
    fn from(value: PathProportionError) -> Self {
        Self::Path(value)
    }
}

/// High-precision local-space point used by derivative-like authoring queries.
///
/// Retained geometry is still stored in the renderer-facing f32 representation,
/// but proportion arithmetic and Bezier evaluation can remain f64 until the final
/// authored result is lowered. This matters for Manim operations such as
/// `TangentLine`, whose default finite difference is only `1e-6` in path progress.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GeometryPoint64 {
    pub x: f64,
    pub y: f64,
}

impl GeometryPoint64 {
    const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn from_vec2(point: Vec2) -> Self {
        Self::new(f64::from(point.x), f64::from(point.y))
    }

    fn distance(self, other: Self) -> f64 {
        (other.x - self.x).hypot(other.y - self.y)
    }
}

#[derive(Clone, Copy, Debug)]
enum CurveKind64 {
    Line,
    Quadratic {
        control: GeometryPoint64,
    },
    Cubic {
        control1: GeometryPoint64,
        control2: GeometryPoint64,
    },
    Close,
}

#[derive(Clone, Copy, Debug)]
struct Curve64 {
    from: GeometryPoint64,
    to: GeometryPoint64,
    kind: CurveKind64,
}

/// Return the local-space point at `alpha` for path-like retained geometry.
///
/// Vector paths reuse [`point_from_proportion`]. Lines remain exact analytic
/// segments. Circles reproduce ManimCE v0.21's nine-component quadratic-Bezier
/// `Arc(TAU)` representation rather than substituting an ideal trigonometric
/// circle, so downstream tangent/path queries observe the same authored curve.
pub fn point_from_geometry_proportion(
    geometry: &GeometryRef,
    alpha: f32,
) -> Result<Vec2, GeometryProportionError> {
    validate_proportion(alpha)?;
    match geometry {
        GeometryRef::Circle { radius } => Ok(circle_point_from_proportion(*radius, alpha)),
        GeometryRef::Line { start, end } => Ok(*start + (*end - *start) * alpha),
        GeometryRef::VectorPath(path) => Ok(point_from_proportion(path, alpha)?),
        GeometryRef::Rectangle { .. } | GeometryRef::External(_) => {
            Err(GeometryProportionError::UnsupportedGeometry)
        }
    }
}

/// Evaluate two path proportions with f64 arithmetic through the same shared measure.
///
/// This is the precision-preserving boundary for finite-difference authoring
/// operations. It intentionally evaluates both samples from one prepared path
/// measure so callers such as `TangentLine` do not rebuild path lengths twice and
/// do not downcast `alpha ± d_alpha` before subtracting nearly coincident points.
/// The retained geometry itself remains unchanged and no renderer state is added.
pub fn point_pair_from_geometry_proportion_f64(
    geometry: &GeometryRef,
    first_alpha: f64,
    second_alpha: f64,
) -> Result<(GeometryPoint64, GeometryPoint64), GeometryProportionError> {
    validate_high_precision_proportion(first_alpha)?;
    validate_high_precision_proportion(second_alpha)?;

    match geometry {
        GeometryRef::Circle { radius } => Ok((
            circle_point_from_proportion_f64(f64::from(*radius), first_alpha),
            circle_point_from_proportion_f64(f64::from(*radius), second_alpha),
        )),
        GeometryRef::Line { start, end } => {
            let start = GeometryPoint64::from_vec2(*start);
            let end = GeometryPoint64::from_vec2(*end);
            Ok((
                lerp64(start, end, first_alpha),
                lerp64(start, end, second_alpha),
            ))
        }
        GeometryRef::VectorPath(path) => {
            let plan = GeometryProportionPlan64::new(path)?;
            Ok((plan.point(first_alpha), plan.point(second_alpha)))
        }
        GeometryRef::Rectangle { .. } | GeometryRef::External(_) => {
            Err(GeometryProportionError::UnsupportedGeometry)
        }
    }
}

fn validate_proportion(alpha: f32) -> Result<(), GeometryProportionError> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(PathProportionError::InvalidProportion(alpha).into());
    }
    Ok(())
}

fn validate_high_precision_proportion(alpha: f64) -> Result<(), GeometryProportionError> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(GeometryProportionError::InvalidHighPrecisionProportion(
            alpha,
        ));
    }
    Ok(())
}

fn circle_point_from_proportion(radius: f32, alpha: f32) -> Vec2 {
    if alpha == 1.0 {
        return Vec2::new(radius, 0.0);
    }

    let component_count = MANIM_CIRCLE_COMPONENTS as f32;
    let scaled = alpha * component_count;
    let component = (scaled.floor() as usize).min(MANIM_CIRCLE_COMPONENTS - 1);
    let t = scaled - component as f32;
    let theta = TAU / component_count;
    let start_angle = component as f32 * theta;
    let end_angle = (component + 1) as f32 * theta;
    let middle_angle = (start_angle + end_angle) * 0.5;
    let control_radius = radius / (theta * 0.5).cos();

    let start = Vec2::new(start_angle.cos() * radius, start_angle.sin() * radius);
    let control = Vec2::new(
        middle_angle.cos() * control_radius,
        middle_angle.sin() * control_radius,
    );
    let end = Vec2::new(end_angle.cos() * radius, end_angle.sin() * radius);

    let first = start + (control - start) * t;
    let second = control + (end - control) * t;
    first + (second - first) * t
}

fn circle_point_from_proportion_f64(radius: f64, alpha: f64) -> GeometryPoint64 {
    if alpha == 1.0 {
        return GeometryPoint64::new(radius, 0.0);
    }

    let component_count = MANIM_CIRCLE_COMPONENTS as f64;
    let scaled = alpha * component_count;
    let component = (scaled.floor() as usize).min(MANIM_CIRCLE_COMPONENTS - 1);
    let t = scaled - component as f64;
    let theta = std::f64::consts::TAU / component_count;
    let start_angle = component as f64 * theta;
    let end_angle = (component + 1) as f64 * theta;
    let middle_angle = (start_angle + end_angle) * 0.5;
    let control_radius = radius / (theta * 0.5).cos();

    let start = GeometryPoint64::new(start_angle.cos() * radius, start_angle.sin() * radius);
    let control = GeometryPoint64::new(
        middle_angle.cos() * control_radius,
        middle_angle.sin() * control_radius,
    );
    let end = GeometryPoint64::new(end_angle.cos() * radius, end_angle.sin() * radius);

    let first = lerp64(start, control, t);
    let second = lerp64(control, end, t);
    lerp64(first, second, t)
}

#[derive(Clone, Debug)]
struct GeometryProportionPlan64 {
    curves: Vec<Curve64>,
    lengths: Vec<f64>,
    cumulative_lengths: Vec<f64>,
    total_length: f64,
}

impl GeometryProportionPlan64 {
    fn new(path: &VectorPath) -> Result<Self, GeometryProportionError> {
        let curves = collect_curves64(path);
        if curves.is_empty() {
            return Err(GeometryProportionError::EmptyPath);
        }

        let lengths = curves
            .iter()
            .copied()
            .map(sampled_curve_length64)
            .collect::<Vec<_>>();
        let mut cumulative_lengths = Vec::with_capacity(lengths.len());
        let mut total_length = 0.0_f64;
        for &length in &lengths {
            total_length += length;
            cumulative_lengths.push(total_length);
        }

        Ok(Self {
            curves,
            lengths,
            cumulative_lengths,
            total_length,
        })
    }

    fn point(&self, alpha: f64) -> GeometryPoint64 {
        if alpha == 1.0 {
            return self
                .curves
                .last()
                .expect("high-precision path proportion plans are never empty")
                .to;
        }

        let target_length = alpha * self.total_length;
        if target_length.is_finite() {
            let curve_index = self
                .cumulative_lengths
                .partition_point(|&end_length| end_length < target_length);
            if curve_index < self.curves.len() {
                let current_length = curve_index
                    .checked_sub(1)
                    .map_or(0.0, |index| self.cumulative_lengths[index]);
                let length = self.lengths[curve_index];
                let residue = if length > 0.0 {
                    ((target_length - current_length) / length).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                return curve_point64(self.curves[curve_index], residue);
            }
        }

        self.curves
            .last()
            .expect("high-precision path proportion plans are never empty")
            .to
    }
}

fn collect_curves64(path: &VectorPath) -> Vec<Curve64> {
    let mut curves = Vec::new();
    let mut current = None;
    let mut subpath_start = None;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                let to = GeometryPoint64::from_vec2(to);
                current = Some(to);
                subpath_start = Some(to);
            }
            PathCommand::LineTo { to } => {
                let to = GeometryPoint64::from_vec2(to);
                if let Some(from) = current {
                    curves.push(Curve64 {
                        from,
                        to,
                        kind: CurveKind64::Line,
                    });
                }
                current = Some(to);
            }
            PathCommand::QuadraticTo { control, to } => {
                let to = GeometryPoint64::from_vec2(to);
                if let Some(from) = current {
                    curves.push(Curve64 {
                        from,
                        to,
                        kind: CurveKind64::Quadratic {
                            control: GeometryPoint64::from_vec2(control),
                        },
                    });
                }
                current = Some(to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let to = GeometryPoint64::from_vec2(to);
                if let Some(from) = current {
                    curves.push(Curve64 {
                        from,
                        to,
                        kind: CurveKind64::Cubic {
                            control1: GeometryPoint64::from_vec2(control1),
                            control2: GeometryPoint64::from_vec2(control2),
                        },
                    });
                }
                current = Some(to);
            }
            PathCommand::Close => {
                if let (Some(from), Some(to)) = (current, subpath_start) {
                    if from.distance(to) > f64::from(f32::EPSILON) {
                        curves.push(Curve64 {
                            from,
                            to,
                            kind: CurveKind64::Close,
                        });
                    }
                    current = Some(to);
                }
            }
        }
    }

    curves
}

fn sampled_curve_length64(curve: Curve64) -> f64 {
    let denominator = (MANIM_LENGTH_SAMPLE_POINTS - 1) as f64;
    let mut previous = curve_point64(curve, 0.0);
    let mut length = 0.0_f64;
    for sample in 1..MANIM_LENGTH_SAMPLE_POINTS {
        let point = curve_point64(curve, sample as f64 / denominator);
        length += previous.distance(point);
        previous = point;
    }
    length
}

fn curve_point64(curve: Curve64, t: f64) -> GeometryPoint64 {
    match curve.kind {
        CurveKind64::Line | CurveKind64::Close => lerp64(curve.from, curve.to, t),
        CurveKind64::Quadratic { control } => {
            let first = lerp64(curve.from, control, t);
            let second = lerp64(control, curve.to, t);
            lerp64(first, second, t)
        }
        CurveKind64::Cubic { control1, control2 } => {
            let p01 = lerp64(curve.from, control1, t);
            let p12 = lerp64(control1, control2, t);
            let p23 = lerp64(control2, curve.to, t);
            let p012 = lerp64(p01, p12, t);
            let p123 = lerp64(p12, p23, t);
            lerp64(p012, p123, t)
        }
    }
}

fn lerp64(start: GeometryPoint64, end: GeometryPoint64, t: f64) -> GeometryPoint64 {
    GeometryPoint64::new(
        start.x + (end.x - start.x) * t,
        start.y + (end.y - start.y) * t,
    )
}

#[cfg(test)]
mod tests {
    use noon_core::VectorPath;

    use super::*;

    fn assert_point(actual: Vec2, expected: Vec2) {
        assert!(
            (actual.x - expected.x).abs() <= 1.0e-5 && (actual.y - expected.y).abs() <= 1.0e-5,
            "{actual:?} != {expected:?}"
        );
    }

    fn assert_point64(actual: GeometryPoint64, expected: GeometryPoint64, tolerance: f64) {
        assert!(
            (actual.x - expected.x).abs() <= tolerance
                && (actual.y - expected.y).abs() <= tolerance,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn circle_matches_manim_quadratic_arc_proportion_samples() {
        let circle = GeometryRef::circle(2.0);
        assert_point(
            point_from_geometry_proportion(&circle, 0.0).unwrap(),
            Vec2::new(2.0, 0.0),
        );
        assert_point(
            point_from_geometry_proportion(&circle, 0.25).unwrap(),
            Vec2::new(-0.0057401983, 2.0021698),
        );
        assert_point(
            point_from_geometry_proportion(&circle, 0.4).unwrap(),
            Vec2::new(-1.6191778, 1.1800613),
        );
        assert_point(
            point_from_geometry_proportion(&circle, 1.0).unwrap(),
            Vec2::new(2.0, 0.0),
        );
    }

    #[test]
    fn line_and_vector_path_keep_existing_shared_measures() {
        let line = GeometryRef::line(Vec2::new(-2.0, 1.0), Vec2::new(2.0, 1.0));
        assert_point(
            point_from_geometry_proportion(&line, 0.75).unwrap(),
            Vec2::new(1.0, 1.0),
        );

        let path = GeometryRef::VectorPath(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .quadratic_to(Vec2::new(1.0, 2.0), Vec2::new(2.0, 0.0)),
        );
        assert_point(
            point_from_geometry_proportion(&path, 0.5).unwrap(),
            Vec2::new(1.0, 1.0),
        );
    }

    #[test]
    fn high_precision_pair_preserves_manim_default_tangent_delta() {
        let (first, second) = point_pair_from_geometry_proportion_f64(
            &GeometryRef::circle(2.0),
            0.4 - 1.0e-6,
            0.4 + 1.0e-6,
        )
        .unwrap();

        assert_point64(
            first,
            GeometryPoint64::new(-1.6191706294001942, 1.1800713157856422),
            1.0e-12,
        );
        assert_point64(
            second,
            GeometryPoint64::new(-1.6191850851338567, 1.1800512993440777),
            1.0e-12,
        );

        let dx = second.x - first.x;
        let dy = second.y - first.y;
        let sample_length = dx.hypot(dy);
        let center = GeometryPoint64::new((first.x + second.x) * 0.5, (first.y + second.y) * 0.5);
        let half_x = dx / sample_length * 2.0;
        let half_y = dy / sample_length * 2.0;
        assert_point64(
            GeometryPoint64::new(center.x - half_x, center.y - half_y),
            GeometryPoint64::new(-0.448227905248914, 2.80144226522681),
            1.0e-10,
        );
        assert_point64(
            GeometryPoint64::new(center.x + half_x, center.y + half_y),
            GeometryPoint64::new(-2.7901278092851367, -0.4413196500970902),
            1.0e-10,
        );
    }

    #[test]
    fn high_precision_vector_path_pair_reuses_manim_sampled_measure() {
        let path = GeometryRef::VectorPath(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .quadratic_to(Vec2::new(1.0, 2.0), Vec2::new(2.0, 0.0)),
        );
        let (first, second) =
            point_pair_from_geometry_proportion_f64(&path, 0.5 - 1.0e-6, 0.5 + 1.0e-6).unwrap();

        assert!(first.x < 1.0 && second.x > 1.0);
        assert!(first.distance(second) > 1.0e-6);
        assert_point64(
            point_pair_from_geometry_proportion_f64(&path, 0.5, 0.5)
                .unwrap()
                .0,
            GeometryPoint64::new(1.0, 1.0),
            1.0e-12,
        );
    }

    #[test]
    fn invalid_proportion_and_non_path_geometry_are_rejected() {
        let circle = GeometryRef::circle(1.0);
        assert!(matches!(
            point_from_geometry_proportion(&circle, f32::NAN),
            Err(GeometryProportionError::Path(
                PathProportionError::InvalidProportion(_)
            ))
        ));
        assert!(matches!(
            point_pair_from_geometry_proportion_f64(&circle, f64::NAN, 0.5),
            Err(GeometryProportionError::InvalidHighPrecisionProportion(value)) if value.is_nan()
        ));
        assert_eq!(
            point_from_geometry_proportion(&GeometryRef::rectangle(1.0, 1.0), 0.5),
            Err(GeometryProportionError::UnsupportedGeometry)
        );
        assert_eq!(
            point_pair_from_geometry_proportion_f64(&GeometryRef::rectangle(1.0, 1.0), 0.25, 0.75,),
            Err(GeometryProportionError::UnsupportedGeometry)
        );
    }
}
