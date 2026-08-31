use crate::NumberRange;

pub const MANIM_SAMPLED_GRAPH_POINTS_PER_TICK: f64 = 10.0;
pub const MANIM_DEFAULT_PARAMETRIC_STEP: f64 = 0.01;
pub const MANIM_DEFAULT_DISCONTINUITY_DT: f64 = 1.0e-8;

/// Caller-visible range override for the initial linear `Axes.plot` subset.
///
/// Manim gives the third `x_range` value two different meanings: on `Axes` it
/// is the tick step, while on `Axes.plot` it becomes the function sample step.
/// This enum keeps that distinction explicit instead of overloading `NumberRange`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PlotRangeRequest {
    /// Reuse axis bounds and derive sample step as `axis_tick_step / 10`.
    #[default]
    AxisDefault,
    /// Override only bounds; sample step still derives from the axis tick step.
    Bounds { min: f64, max: f64 },
    /// Override bounds and function sample step explicitly.
    Explicit { min: f64, max: f64, step: f64 },
}

/// Deterministic parameter range consumed by `ParametricFunction` sampling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleRange {
    start: f64,
    end: f64,
    step: f64,
}

impl SampleRange {
    pub fn new(start: f64, end: f64, step: f64) -> Result<Self, PlotSamplingError> {
        if !start.is_finite() || !end.is_finite() || !step.is_finite() {
            return Err(PlotSamplingError::NonFiniteRange { start, end, step });
        }
        if step == 0.0 {
            return Err(PlotSamplingError::InvalidStep(step));
        }
        Ok(Self { start, end, step })
    }

    /// Manim `ParametricFunction(t_range=(start, end))` adds a `0.01` step.
    pub fn parametric_bounds(start: f64, end: f64) -> Result<Self, PlotSamplingError> {
        Self::new(start, end, MANIM_DEFAULT_PARAMETRIC_STEP)
    }

    /// Resolve the v0.21 `Axes.plot` range rule from one canonical axis range.
    pub fn for_axes_plot(
        axis_range: NumberRange,
        request: PlotRangeRequest,
    ) -> Result<Self, PlotSamplingError> {
        let derived_step = axis_range.step() / MANIM_SAMPLED_GRAPH_POINTS_PER_TICK;
        match request {
            PlotRangeRequest::AxisDefault => {
                Self::new(axis_range.min(), axis_range.max(), derived_step)
            }
            PlotRangeRequest::Bounds { min, max } => Self::new(min, max, derived_step),
            PlotRangeRequest::Explicit { min, max, step } => Self::new(min, max, step),
        }
    }

    pub const fn start(self) -> f64 {
        self.start
    }

    pub const fn end(self) -> f64 {
        self.end
    }

    pub const fn step(self) -> f64 {
        self.step
    }
}

/// One continuous parameter interval. Discontinuities produce multiple spans.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleSpan {
    start: f64,
    end: f64,
    step: f64,
}

impl SampleSpan {
    fn new(start: f64, end: f64, step: f64) -> Self {
        debug_assert!(step != 0.0);
        Self { start, end, step }
    }

    pub const fn start(self) -> f64 {
        self.start
    }

    pub const fn end(self) -> f64 {
        self.end
    }

    pub const fn step(self) -> f64 {
        self.step
    }

    /// Reproduce the linear-scaling parameter sequence used by Manim v0.21:
    /// `np.arange(start, end, step)` followed by the exact `end` value.
    ///
    /// A step whose sign moves away from `end` intentionally produces no regular
    /// samples; Manim still appends the exact endpoint in that case.
    pub fn parameters(self) -> Result<Vec<f64>, PlotSamplingError> {
        let distance = self.end - self.start;
        let traverses_toward_end = distance != 0.0 && distance.signum() == self.step.signum();
        let regular_count = if traverses_toward_end {
            let raw_count = (distance / self.step).ceil();
            if !raw_count.is_finite() || raw_count > usize::MAX as f64 {
                return Err(PlotSamplingError::SampleCountOverflow {
                    start: self.start,
                    end: self.end,
                    step: self.step,
                });
            }
            raw_count.max(0.0) as usize
        } else {
            0
        };

        let capacity =
            regular_count
                .checked_add(1)
                .ok_or(PlotSamplingError::SampleCountOverflow {
                    start: self.start,
                    end: self.end,
                    step: self.step,
                })?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(capacity)
            .map_err(|_| PlotSamplingError::SampleAllocationFailed(capacity))?;

        for index in 0..regular_count {
            let value = self.start + index as f64 * self.step;
            let before_end = if self.step > 0.0 {
                value < self.end
            } else {
                value > self.end
            };
            if !before_end {
                break;
            }
            values.push(value);
        }
        values.push(self.end);
        Ok(values)
    }
}

/// Renderer-independent sampling plan for one static parametric/function graph.
#[derive(Clone, Debug, PartialEq)]
pub struct ParametricSamplePlan {
    range: SampleRange,
    discontinuity_dt: f64,
    spans: Vec<SampleSpan>,
}

impl ParametricSamplePlan {
    /// Build the branch Manim uses when `discontinuities` is supplied, including
    /// an explicitly empty iterable. Manim sorts the expanded boundary array in
    /// this branch before pairing it into continuous spans.
    pub fn new(
        range: SampleRange,
        discontinuities: &[f64],
        discontinuity_dt: f64,
    ) -> Result<Self, PlotSamplingError> {
        if !discontinuity_dt.is_finite() || discontinuity_dt < 0.0 {
            return Err(PlotSamplingError::InvalidDiscontinuityDt(discontinuity_dt));
        }
        if discontinuities.iter().any(|value| !value.is_finite()) {
            return Err(PlotSamplingError::NonFiniteDiscontinuity);
        }

        let mut boundaries = vec![range.start(), range.end()];
        boundaries.extend(
            discontinuities
                .iter()
                .copied()
                .filter(|value| range.start() <= *value && *value <= range.end())
                .flat_map(|value| [value - discontinuity_dt, value + discontinuity_dt]),
        );
        boundaries.sort_by(f64::total_cmp);

        let (boundary_pairs, remainder) = boundaries.as_chunks::<2>();
        debug_assert!(remainder.is_empty());
        let spans = boundary_pairs
            .iter()
            .map(|pair| SampleSpan::new(pair[0], pair[1], range.step()))
            .collect();

        Ok(Self {
            range,
            discontinuity_dt,
            spans,
        })
    }

    /// Build the distinct Manim branch where `discontinuities=None`; unlike
    /// [`Self::new`], this preserves the caller's original range orientation.
    pub fn without_discontinuities(range: SampleRange) -> Self {
        Self {
            range,
            discontinuity_dt: MANIM_DEFAULT_DISCONTINUITY_DT,
            spans: vec![SampleSpan::new(range.start(), range.end(), range.step())],
        }
    }

    pub const fn range(&self) -> SampleRange {
        self.range
    }

    pub const fn discontinuity_dt(&self) -> f64 {
        self.discontinuity_dt
    }

    pub fn spans(&self) -> &[SampleSpan] {
        &self.spans
    }

    pub fn parameter_subpaths(&self) -> Result<Vec<Vec<f64>>, PlotSamplingError> {
        self.spans
            .iter()
            .copied()
            .map(SampleSpan::parameters)
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlotSamplingError {
    NonFiniteRange { start: f64, end: f64, step: f64 },
    InvalidStep(f64),
    InvalidDiscontinuityDt(f64),
    NonFiniteDiscontinuity,
    SampleCountOverflow { start: f64, end: f64, step: f64 },
    SampleAllocationFailed(usize),
}

impl std::fmt::Display for PlotSamplingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NonFiniteRange { start, end, step } => write!(
                formatter,
                "plot sample range must be finite: [{start}, {end}, {step}]"
            ),
            Self::InvalidStep(step) => {
                write!(formatter, "plot sample step must be non-zero: {step}")
            }
            Self::InvalidDiscontinuityDt(dt) => write!(
                formatter,
                "plot discontinuity dt must be finite and non-negative: {dt}"
            ),
            Self::NonFiniteDiscontinuity => {
                formatter.write_str("plot discontinuities must be finite")
            }
            Self::SampleCountOverflow { start, end, step } => write!(
                formatter,
                "plot sample count exceeds addressable memory: [{start}, {end}, {step}]"
            ),
            Self::SampleAllocationFailed(count) => {
                write!(
                    formatter,
                    "plot sample allocation failed for {count} values"
                )
            }
        }
    }
}

impl std::error::Error for PlotSamplingError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1.0e-12,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn axes_plot_uses_ten_samples_per_axis_tick_by_default() {
        let axis = NumberRange::new(-2.0, 8.0, 2.0).unwrap();
        let range = SampleRange::for_axes_plot(axis, PlotRangeRequest::AxisDefault).unwrap();
        assert_eq!(range.start(), -2.0);
        assert_eq!(range.end(), 8.0);
        assert_close(range.step(), 0.2);
    }

    #[test]
    fn bounds_override_keeps_axis_derived_sample_step() {
        let axis = NumberRange::new(-4.0, 4.0, 0.5).unwrap();
        let range =
            SampleRange::for_axes_plot(axis, PlotRangeRequest::Bounds { min: 1.0, max: 2.0 })
                .unwrap();
        assert_eq!(range.start(), 1.0);
        assert_eq!(range.end(), 2.0);
        assert_close(range.step(), 0.05);
    }

    #[test]
    fn explicit_plot_sample_step_is_not_divided_again() {
        let axis = NumberRange::new(-4.0, 4.0, 1.0).unwrap();
        let range = SampleRange::for_axes_plot(
            axis,
            PlotRangeRequest::Explicit {
                min: 0.0,
                max: 1.0,
                step: 0.125,
            },
        )
        .unwrap();
        assert_close(range.step(), 0.125);
    }

    #[test]
    fn parametric_bounds_use_manim_default_point_zero_one_step() {
        let range = SampleRange::parametric_bounds(0.0, 1.0).unwrap();
        assert_close(range.step(), 0.01);
    }

    #[test]
    fn span_is_half_open_then_appends_exact_endpoint() {
        let span = SampleSpan::new(0.0, 1.0, 0.3);
        let values = span.parameters().unwrap();
        assert_eq!(values.len(), 5);
        for (actual, expected) in values.into_iter().zip([0.0, 0.3, 0.6, 0.9, 1.0]) {
            assert_close(actual, expected);
        }
    }

    #[test]
    fn signed_step_matches_arange_direction_and_endpoint_append() {
        let descending = SampleSpan::new(1.0, -0.1, -0.4).parameters().unwrap();
        assert_eq!(descending.len(), 4);
        for (actual, expected) in descending.into_iter().zip([1.0, 0.6, 0.2, -0.1]) {
            assert_close(actual, expected);
        }

        let away_from_end = SampleSpan::new(1.0, -1.0, 0.25).parameters().unwrap();
        assert_eq!(away_from_end, vec![-1.0]);
    }

    #[test]
    fn equal_bounds_append_one_exact_endpoint() {
        let values = SampleSpan::new(2.0, 2.0, 0.1).parameters().unwrap();
        assert_eq!(values, vec![2.0]);
    }

    #[test]
    fn discontinuities_split_sorted_paired_subpaths() {
        let range = SampleRange::new(-3.0, 3.0, 1.0).unwrap();
        let plan = ParametricSamplePlan::new(range, &[-2.0, 2.0], 0.1).unwrap();
        assert_eq!(
            plan.spans(),
            &[
                SampleSpan::new(-3.0, -2.1, 1.0),
                SampleSpan::new(-1.9, 1.9, 1.0),
                SampleSpan::new(2.1, 3.0, 1.0),
            ]
        );
        let paths = plan.parameter_subpaths().unwrap();
        assert_close(*paths[0].last().unwrap(), -2.1);
        assert_close(paths[1][0], -1.9);
        assert_close(*paths[1].last().unwrap(), 1.9);
        assert_close(paths[2][0], 2.1);
    }

    #[test]
    fn explicit_empty_discontinuities_still_sort_boundaries() {
        let range = SampleRange::new(2.0, -1.0, -0.5).unwrap();
        let explicit_empty = ParametricSamplePlan::new(range, &[], 0.1).unwrap();
        assert_eq!(explicit_empty.spans(), &[SampleSpan::new(-1.0, 2.0, -0.5)]);
        assert_eq!(
            explicit_empty.parameter_subpaths().unwrap(),
            vec![vec![2.0]]
        );

        let absent = ParametricSamplePlan::without_discontinuities(range);
        assert_eq!(absent.spans(), &[SampleSpan::new(2.0, -1.0, -0.5)]);
        assert_eq!(
            absent.parameter_subpaths().unwrap(),
            vec![vec![2.0, 1.5, 1.0, 0.5, 0.0, -0.5, -1.0]]
        );
    }

    #[test]
    fn discontinuities_outside_requested_range_are_ignored() {
        let range = SampleRange::new(0.0, 1.0, 0.25).unwrap();
        let plan = ParametricSamplePlan::new(range, &[-1.0, 2.0], 0.1).unwrap();
        assert_eq!(plan.spans(), &[SampleSpan::new(0.0, 1.0, 0.25)]);
    }

    #[test]
    fn invalid_sampling_inputs_fail_before_function_evaluation() {
        assert!(matches!(
            SampleRange::new(0.0, 1.0, 0.0),
            Err(PlotSamplingError::InvalidStep(0.0))
        ));
        let range = SampleRange::new(0.0, 1.0, 0.1).unwrap();
        assert!(matches!(
            ParametricSamplePlan::new(range, &[f64::NAN], 0.1),
            Err(PlotSamplingError::NonFiniteDiscontinuity)
        ));
    }
}
