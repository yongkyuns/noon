/// Default spacing inserted when Manim-style vector-field ranges omit a step.
pub const DEFAULT_VECTOR_FIELD_STEP: f64 = 0.5;

/// One 2D point/vector in the coordinate space sampled by a static vector field.
///
/// The planner keeps f64 values until later arrow construction so sampling and
/// function evaluation are not constrained by renderer precision.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VectorFieldPoint {
    pub x: f64,
    pub y: f64,
}

impl VectorFieldPoint {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    fn scaled(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }
}

/// One Manim-style sampling range before the internal `arange` stop expansion.
///
/// Pinned ManimCE v0.21 mutates `[min, max, step]` to
/// `[min, max + step, step]` before calling NumPy `arange`, making the requested
/// maximum effectively inclusive when the step divides the span. When it does
/// not divide the span, the final sample may lie beyond the nominal maximum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorFieldAxisRange {
    pub start: f64,
    pub end: f64,
    pub step: f64,
}

impl VectorFieldAxisRange {
    pub const fn new(start: f64, end: f64, step: f64) -> Self {
        Self { start, end, step }
    }

    pub const fn with_default_step(start: f64, end: f64) -> Self {
        Self::new(start, end, DEFAULT_VECTOR_FIELD_STEP)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorFieldRanges2D {
    pub x: VectorFieldAxisRange,
    pub y: VectorFieldAxisRange,
}

impl VectorFieldRanges2D {
    pub const fn new(x: VectorFieldAxisRange, y: VectorFieldAxisRange) -> Self {
        Self { x, y }
    }
}

/// One prepared static ArrowVectorField sample.
///
/// `raw_vector` is the exact field result at `point`; `raw_norm` is retained for
/// later color-scheme preparation. `display_vector` applies only the configured
/// length mapping and remains rooted at `point`; arrow/tip geometry is a later
/// B2/#77 consumer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorFieldSample {
    pub point: VectorFieldPoint,
    pub raw_vector: VectorFieldPoint,
    pub raw_norm: f64,
    pub display_vector: VectorFieldPoint,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaticVectorFieldPlan {
    pub samples: Vec<VectorFieldSample>,
    pub x_samples: usize,
    pub y_samples: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorFieldAxis {
    X,
    Y,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaticVectorFieldError {
    InvalidRange { axis: VectorFieldAxis },
    SampleCountOverflow,
    NonFiniteFieldOutput { sample_index: usize },
    NonFiniteDisplayedLength { sample_index: usize },
}

impl std::fmt::Display for StaticVectorFieldError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange { axis } => {
                write!(formatter, "vector-field {axis:?} range is invalid")
            }
            Self::SampleCountOverflow => {
                formatter.write_str("vector-field sample count exceeds addressable storage")
            }
            Self::NonFiniteFieldOutput { sample_index } => write!(
                formatter,
                "vector-field sample {sample_index} produced a non-finite vector"
            ),
            Self::NonFiniteDisplayedLength { sample_index } => write!(
                formatter,
                "vector-field sample {sample_index} produced a non-finite displayed length"
            ),
        }
    }
}

impl std::error::Error for StaticVectorFieldError {}

/// Pinned ManimCE v0.21 default ArrowVectorField length mapping.
///
/// This is `0.45 * sigmoid(norm)`, with the zero-vector special case handled by
/// the planner before this function is called.
pub fn default_vector_field_length(norm: f64) -> f64 {
    0.45 / (1.0 + (-norm).exp())
}

/// Prepare a static 2D ArrowVectorField using the pinned default length mapping.
///
/// Field evaluation is preparation-time only. The returned records contain no
/// callback, scene, runtime or renderer authority.
pub fn plan_static_arrow_vector_field<F>(
    field: F,
    ranges: VectorFieldRanges2D,
) -> Result<StaticVectorFieldPlan, StaticVectorFieldError>
where
    F: FnMut(VectorFieldPoint) -> VectorFieldPoint,
{
    plan_static_arrow_vector_field_with_length(field, ranges, default_vector_field_length)
}

/// Prepare a static 2D ArrowVectorField with an explicit displayed-length mapper.
///
/// The mapper is called with the raw vector norm only for non-zero vectors,
/// matching pinned ManimCE v0.21 `ArrowVectorField.get_vector` behavior.
pub fn plan_static_arrow_vector_field_with_length<F, L>(
    mut field: F,
    ranges: VectorFieldRanges2D,
    mut length: L,
) -> Result<StaticVectorFieldPlan, StaticVectorFieldError>
where
    F: FnMut(VectorFieldPoint) -> VectorFieldPoint,
    L: FnMut(f64) -> f64,
{
    let x_values = arange_values(ranges.x, VectorFieldAxis::X)?;
    let y_values = arange_values(ranges.y, VectorFieldAxis::Y)?;
    let sample_count = x_values
        .len()
        .checked_mul(y_values.len())
        .ok_or(StaticVectorFieldError::SampleCountOverflow)?;
    let mut samples = Vec::with_capacity(sample_count);

    // `itertools.product(x_range, y_range, z_range)` in pinned Manim iterates
    // x outermost and y next for the 2D z=0 case. Preserve that stable order so
    // later semantic family identity does not depend on frontend traversal.
    for x in &x_values {
        for y in &y_values {
            let point = VectorFieldPoint::new(*x, *y);
            let raw_vector = field(point);
            let raw_norm = raw_vector.length();
            if !raw_vector.x.is_finite() || !raw_vector.y.is_finite() || !raw_norm.is_finite() {
                return Err(StaticVectorFieldError::NonFiniteFieldOutput {
                    sample_index: samples.len(),
                });
            }

            let display_vector = if raw_norm == 0.0 {
                VectorFieldPoint::ZERO
            } else {
                let display_length = length(raw_norm);
                if !display_length.is_finite() {
                    return Err(StaticVectorFieldError::NonFiniteDisplayedLength {
                        sample_index: samples.len(),
                    });
                }
                raw_vector.scaled(display_length / raw_norm)
            };
            samples.push(VectorFieldSample {
                point,
                raw_vector,
                raw_norm,
                display_vector,
            });
        }
    }

    Ok(StaticVectorFieldPlan {
        samples,
        x_samples: x_values.len(),
        y_samples: y_values.len(),
    })
}

fn arange_values(
    range: VectorFieldAxisRange,
    axis: VectorFieldAxis,
) -> Result<Vec<f64>, StaticVectorFieldError> {
    if !range.start.is_finite()
        || !range.end.is_finite()
        || !range.step.is_finite()
        || range.step == 0.0
    {
        return Err(StaticVectorFieldError::InvalidRange { axis });
    }

    let stop = range.end + range.step;
    if !stop.is_finite() {
        return Err(StaticVectorFieldError::InvalidRange { axis });
    }
    let span = (stop - range.start) / range.step;
    if !span.is_finite() {
        return Err(StaticVectorFieldError::InvalidRange { axis });
    }
    if span <= 0.0 {
        return Ok(Vec::new());
    }

    let count = span.ceil();
    if count > usize::MAX as f64 {
        return Err(StaticVectorFieldError::SampleCountOverflow);
    }
    let count = count as usize;

    // NumPy arange's floating-point implementation does not repeatedly add the
    // requested step directly. Its effective increment is the representable
    // difference `(start + step) - start`; this matters when `start` is large
    // relative to `step`, and it can even be zero. The element count is still
    // derived from the requested step/stop span above. Preserve both details so
    // Manim's range normalization is reproduced without importing NumPy.
    let actual_step = (range.start + range.step) - range.start;
    if !actual_step.is_finite() {
        return Err(StaticVectorFieldError::InvalidRange { axis });
    }

    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        values.push(range.start + actual_step * index as f64);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(x: VectorFieldAxisRange, y: VectorFieldAxisRange) -> VectorFieldRanges2D {
        VectorFieldRanges2D::new(x, y)
    }

    #[test]
    fn default_step_matches_pinned_half_unit_spacing() {
        let plan = plan_static_arrow_vector_field(
            |_| VectorFieldPoint::ZERO,
            ranges(
                VectorFieldAxisRange::with_default_step(0.0, 1.0),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
        )
        .unwrap();

        let xs: Vec<_> = plan.samples.iter().map(|sample| sample.point.x).collect();
        assert_eq!(xs, vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn non_dividing_step_preserves_manim_arange_overshoot() {
        let plan = plan_static_arrow_vector_field(
            |_| VectorFieldPoint::ZERO,
            ranges(
                VectorFieldAxisRange::new(0.0, 1.0, 0.3),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
        )
        .unwrap();

        let xs: Vec<_> = plan.samples.iter().map(|sample| sample.point.x).collect();
        assert_eq!(xs.len(), 5);
        assert!((xs[4] - 1.2).abs() < 1e-12);
        assert!(xs[4] > 1.0);
    }

    #[test]
    fn arange_uses_numpy_effective_step_at_large_offsets() {
        let start = 100_000_000.0;
        let end = start + 1.0e-6;
        let step = 1.0e-8;
        let plan = plan_static_arrow_vector_field(
            |_| VectorFieldPoint::ZERO,
            ranges(
                VectorFieldAxisRange::new(start, end, step),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
        )
        .unwrap();

        let xs: Vec<_> = plan.samples.iter().map(|sample| sample.point.x).collect();
        let actual_step = (start + step) - start;
        assert_eq!(xs.len(), 102);
        assert_eq!(xs[1] - xs[0], actual_step);
        assert_ne!(actual_step, step);
        assert!(xs[xs.len() - 1] > end + step);
    }

    #[test]
    fn negative_step_uses_the_same_stop_expansion() {
        let plan = plan_static_arrow_vector_field(
            |_| VectorFieldPoint::ZERO,
            ranges(
                VectorFieldAxisRange::new(1.0, -1.0, -0.75),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
        )
        .unwrap();

        let xs: Vec<_> = plan.samples.iter().map(|sample| sample.point.x).collect();
        assert_eq!(xs, vec![1.0, 0.25, -0.5, -1.25]);
    }

    #[test]
    fn sample_order_is_x_major_then_y() {
        let plan = plan_static_arrow_vector_field(
            |point| point,
            ranges(
                VectorFieldAxisRange::new(0.0, 1.0, 1.0),
                VectorFieldAxisRange::new(10.0, 11.0, 1.0),
            ),
        )
        .unwrap();

        let points: Vec<_> = plan.samples.iter().map(|sample| sample.point).collect();
        assert_eq!(
            points,
            vec![
                VectorFieldPoint::new(0.0, 10.0),
                VectorFieldPoint::new(0.0, 11.0),
                VectorFieldPoint::new(1.0, 10.0),
                VectorFieldPoint::new(1.0, 11.0),
            ]
        );
    }

    #[test]
    fn default_length_preserves_direction_and_pinned_sigmoid_norm() {
        let plan = plan_static_arrow_vector_field(
            |_| VectorFieldPoint::new(3.0, 4.0),
            ranges(
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
        )
        .unwrap();
        let sample = plan.samples[0];
        let expected_length = default_vector_field_length(5.0);

        assert_eq!(sample.raw_norm, 5.0);
        assert!((sample.display_vector.length() - expected_length).abs() < 1e-12);
        assert!((sample.display_vector.x / sample.display_vector.y - 0.75).abs() < 1e-12);
    }

    #[test]
    fn zero_vector_skips_length_function() {
        let plan = plan_static_arrow_vector_field_with_length(
            |_| VectorFieldPoint::ZERO,
            ranges(
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
            |_| panic!("pinned Manim does not call length_func for a zero vector"),
        )
        .unwrap();

        assert_eq!(plan.samples[0].display_vector, VectorFieldPoint::ZERO);
    }

    #[test]
    fn invalid_range_fails_before_field_evaluation() {
        assert_eq!(
            plan_static_arrow_vector_field(
                |_| panic!("invalid ranges must fail before field evaluation"),
                ranges(
                    VectorFieldAxisRange::new(0.0, 1.0, 0.0),
                    VectorFieldAxisRange::new(0.0, 0.0, 1.0),
                ),
            ),
            Err(StaticVectorFieldError::InvalidRange {
                axis: VectorFieldAxis::X,
            })
        );
    }

    #[test]
    fn non_finite_field_and_length_results_fail_explicitly() {
        let one_point = ranges(
            VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            VectorFieldAxisRange::new(0.0, 0.0, 1.0),
        );
        assert_eq!(
            plan_static_arrow_vector_field(|_| VectorFieldPoint::new(f64::NAN, 1.0), one_point),
            Err(StaticVectorFieldError::NonFiniteFieldOutput { sample_index: 0 })
        );
        assert_eq!(
            plan_static_arrow_vector_field_with_length(
                |_| VectorFieldPoint::new(1.0, 0.0),
                one_point,
                |_| f64::INFINITY,
            ),
            Err(StaticVectorFieldError::NonFiniteDisplayedLength { sample_index: 0 })
        );
    }
}
