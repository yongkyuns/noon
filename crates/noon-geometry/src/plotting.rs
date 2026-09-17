//! Deterministic preparation of static function and parametric plots.
//!
//! Parameter order follows ManimCE v0.21's `ParametricFunction.generate_points`:
//! sorted discontinuity boundaries, half-open regular samples, then an exact
//! endpoint for each subpath. Callbacks are evaluated only by `evaluate`; the
//! resulting ordinary retained path contains no callback or playback state.

use std::ops::Range;

use noon_core::{Vec2, VectorPath};

use crate::change_path_anchor_mode;

pub const DEFAULT_PARAMETRIC_STEP: f64 = 0.01;
pub const DEFAULT_PLOT_DISCONTINUITY_DT: f64 = 1.0e-8;
pub const DEFAULT_PLOT_SAMPLE_LIMIT: usize = 1_000_000;
pub const GRAPH_SAMPLES_PER_TICK: f64 = 10.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlotPreparationError {
    InvalidRange,
    InvalidDiscontinuity,
    SampleLimitExceeded,
    AllocationFailed,
    SampleCountMismatch { expected: usize, actual: usize },
    InvalidPoint { sample_index: usize },
    SmoothingFailed,
}

impl std::fmt::Display for PlotPreparationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange => formatter.write_str("invalid finite increasing plot range"),
            Self::InvalidDiscontinuity => {
                formatter.write_str("plot discontinuities and nonnegative dt must be finite")
            }
            Self::SampleLimitExceeded => formatter.write_str("plot sample limit exceeded"),
            Self::AllocationFailed => formatter.write_str("plot preparation allocation failed"),
            Self::SampleCountMismatch { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} plot samples, received {actual}"
                )
            }
            Self::InvalidPoint { sample_index } => {
                write!(
                    formatter,
                    "plot sample {sample_index} is not a finite renderable 2D point"
                )
            }
            Self::SmoothingFailed => formatter.write_str("shared plot path smoothing failed"),
        }
    }
}

impl std::error::Error for PlotPreparationError {}

/// An inert preparation request, not an authored scene or an execution plan.
///
/// Limits are an explicit admission policy. Rejection never truncates samples
/// or silently changes the requested step. All sample counts are checked before
/// evaluating a function or allocating its point/path output.
#[derive(Clone, Debug)]
pub struct PlotSamplingOptions {
    pub range: [f64; 3],
    pub discontinuities: Vec<f64>,
    pub dt: f64,
    pub max_samples: usize,
}

impl PlotSamplingOptions {
    pub fn parametric(range: &[f64]) -> Result<Self, PlotPreparationError> {
        Ok(Self {
            range: normalize_range(range, DEFAULT_PARAMETRIC_STEP)?,
            discontinuities: Vec::new(),
            dt: DEFAULT_PLOT_DISCONTINUITY_DT,
            max_samples: DEFAULT_PLOT_SAMPLE_LIMIT,
        })
    }

    /// Resolve an Axes plot range. A two-element override changes the interval,
    /// not the shared default step of one tenth of the axis tick spacing.
    pub fn axes(axis_range: [f64; 3], range: Option<&[f64]>) -> Result<Self, PlotPreparationError> {
        let axis = normalize_range(&axis_range, 1.0)?;
        let step = axis[2] / GRAPH_SAMPLES_PER_TICK;
        let resolved = match range {
            Some(range) => normalize_range(range, step)?,
            None => normalize_range(&[axis[0], axis[1]], step)?,
        };
        Ok(Self {
            range: resolved,
            discontinuities: Vec::new(),
            dt: DEFAULT_PLOT_DISCONTINUITY_DT,
            max_samples: DEFAULT_PLOT_SAMPLE_LIMIT,
        })
    }

    pub fn plan(&self) -> Result<PlotSamplingPlan, PlotPreparationError> {
        let [start, end, step] = normalize_range(&self.range, DEFAULT_PARAMETRIC_STEP)?;
        if !self.dt.is_finite()
            || self.dt < 0.0
            || self.discontinuities.iter().any(|value| !value.is_finite())
        {
            return Err(PlotPreparationError::InvalidDiscontinuity);
        }
        let mut boundaries = Vec::new();
        boundaries
            .try_reserve_exact(2)
            .map_err(|_| PlotPreparationError::AllocationFailed)?;
        boundaries.extend([start, end]);
        for &value in &self.discontinuities {
            if value < start || value > end {
                continue;
            }
            let low = value - self.dt;
            let high = value + self.dt;
            if !low.is_finite() || !high.is_finite() {
                return Err(PlotPreparationError::InvalidDiscontinuity);
            }
            // Each additional boundary pair creates another endpoint sample.
            if boundaries.len() / 2 >= self.max_samples {
                return Err(PlotPreparationError::SampleLimitExceeded);
            }
            boundaries
                .try_reserve(2)
                .map_err(|_| PlotPreparationError::AllocationFailed)?;
            boundaries.extend([low, high]);
        }
        boundaries.sort_by(f64::total_cmp);

        // Preserve the oracle's sorted-pair rule even for overlapping windows
        // or a discontinuity at an endpoint. Clipping/merging would change it.
        let mut total = 0usize;
        let mut counts = Vec::new();
        counts
            .try_reserve_exact(boundaries.len() / 2)
            .map_err(|_| PlotPreparationError::AllocationFailed)?;
        for pair in boundaries.as_chunks::<2>().0 {
            let regular = regular_sample_count(pair[0], pair[1], step, self.max_samples)?;
            let count = regular
                .checked_add(1)
                .ok_or(PlotPreparationError::SampleLimitExceeded)?;
            total = total
                .checked_add(count)
                .filter(|total| *total <= self.max_samples)
                .ok_or(PlotPreparationError::SampleLimitExceeded)?;
            counts.push(count);
        }

        let mut parameters = Vec::new();
        parameters
            .try_reserve_exact(total)
            .map_err(|_| PlotPreparationError::AllocationFailed)?;
        let mut subpaths = Vec::new();
        subpaths
            .try_reserve_exact(counts.len())
            .map_err(|_| PlotPreparationError::AllocationFailed)?;
        for (pair, count) in boundaries.as_chunks::<2>().0.iter().zip(counts) {
            let first = parameters.len();
            let regular = count - 1;
            if regular == 1 {
                // A single regular sample is exactly the span start. Computing
                // an unused next increment could overflow for a valid range.
                parameters.push(pair[0]);
            } else if regular > 1 {
                // NumPy's floating arange increment is the representable
                // difference, not repeated addition of the requested step.
                let increment = (pair[0] + step) - pair[0];
                if !increment.is_finite() {
                    return Err(PlotPreparationError::InvalidRange);
                }
                for index in 0..regular {
                    let value = pair[0] + index as f64 * increment;
                    if !value.is_finite() {
                        return Err(PlotPreparationError::InvalidRange);
                    }
                    parameters.push(value);
                }
            }
            parameters.push(pair[1]);
            subpaths.push(first..parameters.len());
        }
        Ok(PlotSamplingPlan {
            range: [start, end, step],
            parameters,
            subpaths,
        })
    }
}

/// Immutable sample order shared by native Rust and host-language evaluators.
#[derive(Clone, Debug, PartialEq)]
pub struct PlotSamplingPlan {
    range: [f64; 3],
    parameters: Vec<f64>,
    subpaths: Vec<Range<usize>>,
}

impl PlotSamplingPlan {
    pub const fn range(&self) -> [f64; 3] {
        self.range
    }

    pub fn parameters(&self) -> &[f64] {
        &self.parameters
    }

    pub fn subpaths(&self) -> &[Range<usize>] {
        &self.subpaths
    }

    /// Invoke a native evaluator exactly once per planned sample, in order.
    /// No reference to the callback is retained in the returned path.
    pub fn evaluate(
        &self,
        mut function: impl FnMut(f64) -> [f64; 2],
        use_smoothing: bool,
    ) -> Result<VectorPath, PlotPreparationError> {
        let mut points = Vec::new();
        points
            .try_reserve_exact(self.parameters.len())
            .map_err(|_| PlotPreparationError::AllocationFailed)?;
        for (index, &parameter) in self.parameters.iter().enumerate() {
            let point = function(parameter);
            checked_point(point, index)?;
            points.push(point);
        }
        self.path_from_samples(&points, use_smoothing)
    }

    /// Consume host-evaluated samples without duplicating sampling or subpath
    /// decisions in the host. The host must return one point per parameter.
    pub fn path_from_samples(
        &self,
        points: &[[f64; 2]],
        use_smoothing: bool,
    ) -> Result<VectorPath, PlotPreparationError> {
        if points.len() != self.parameters.len() {
            return Err(PlotPreparationError::SampleCountMismatch {
                expected: self.parameters.len(),
                actual: points.len(),
            });
        }
        let mut path = VectorPath::new();
        for subpath in &self.subpaths {
            let first = checked_point(points[subpath.start], subpath.start)?;
            path = path.move_to(first);
            for (offset, &point) in points[subpath.start + 1..subpath.end].iter().enumerate() {
                path = path.line_to(checked_point(point, subpath.start + 1 + offset)?);
            }
        }
        if use_smoothing {
            change_path_anchor_mode(&path, true).map_err(|_| PlotPreparationError::SmoothingFailed)
        } else {
            Ok(path)
        }
    }
}

/// Prepare a sampled-data polyline without resampling or implicit smoothing.
/// Ordering (including repeated x coordinates) is authored input and is retained.
pub fn sampled_plot_path(points: &[[f64; 2]]) -> Result<VectorPath, PlotPreparationError> {
    if points.len() > DEFAULT_PLOT_SAMPLE_LIMIT {
        return Err(PlotPreparationError::SampleLimitExceeded);
    }
    let mut path = VectorPath::new();
    for (index, &point) in points.iter().enumerate() {
        let point = checked_point(point, index)?;
        path = if index == 0 {
            path.move_to(point)
        } else {
            path.line_to(point)
        };
    }
    Ok(path)
}

fn normalize_range(range: &[f64], default_step: f64) -> Result<[f64; 3], PlotPreparationError> {
    let (start, end, step) = match *range {
        [start, end] => (start, end, default_step),
        [start, end, step] => (start, end, step),
        _ => return Err(PlotPreparationError::InvalidRange),
    };
    if !start.is_finite()
        || !end.is_finite()
        || !step.is_finite()
        || start >= end
        || step <= 0.0
        || !(end - start).is_finite()
    {
        return Err(PlotPreparationError::InvalidRange);
    }
    Ok([start, end, step])
}

fn regular_sample_count(
    start: f64,
    end: f64,
    step: f64,
    limit: usize,
) -> Result<usize, PlotPreparationError> {
    let count = ((end - start) / step).ceil();
    // A nonempty half-open span always contains its start, even when the
    // positive span/step quotient underflows to zero. Equal endpoints do not.
    let count = if start < end && count == 0.0 {
        1.0
    } else {
        count
    };
    if !count.is_finite() || count < 0.0 || count >= limit as f64 {
        return Err(PlotPreparationError::SampleLimitExceeded);
    }
    Ok(count as usize)
}

fn checked_point(point: [f64; 2], sample_index: usize) -> Result<Vec2, PlotPreparationError> {
    if point
        .iter()
        .any(|value| !value.is_finite() || value.abs() > f64::from(f32::MAX))
    {
        return Err(PlotPreparationError::InvalidPoint { sample_index });
    }
    Ok(Vec2::new(point[0] as f32, point[1] as f32))
}

#[cfg(test)]
mod boundary_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::PathCommand;

    #[test]
    fn defaults_resolve_in_the_shared_planner() {
        let plan = PlotSamplingOptions::parametric(&[0.0, 1.0])
            .unwrap()
            .plan()
            .unwrap();
        assert_eq!(plan.range(), [0.0, 1.0, 0.01]);
        assert_eq!(plan.parameters().len(), 101);
        assert_eq!(plan.parameters().last(), Some(&1.0));
        let options = PlotSamplingOptions::axes([0.0, 4.0, 2.0], Some(&[1.0, 3.0])).unwrap();
        assert_eq!(options.range, [1.0, 3.0, 0.2]);
        let explicit = PlotSamplingOptions::axes([0.0, 4.0, 2.0], Some(&[1.0, 3.0, 0.25])).unwrap();
        assert_eq!(explicit.range, [1.0, 3.0, 0.25]);
    }

    #[test]
    fn half_open_samples_have_one_exact_endpoint() {
        let plan = PlotSamplingOptions::parametric(&[0.0, 1.0, 0.3])
            .unwrap()
            .plan()
            .unwrap();
        assert_eq!(plan.parameters().len(), 5);
        assert_eq!(plan.parameters()[0], 0.0);
        assert_eq!(plan.parameters()[4], 1.0);
        assert!((plan.parameters()[3] - 0.9).abs() < 1.0e-14);
        assert_eq!(plan.subpaths().len(), 1);
        assert_eq!(plan.subpaths()[0], 0..5);
    }

    #[test]
    fn declared_discontinuities_split_paths_before_smoothing() {
        let mut options = PlotSamplingOptions::parametric(&[-1.0, 1.0, 0.25]).unwrap();
        options.discontinuities = vec![0.0];
        options.dt = 0.125;
        let plan = options.plan().unwrap();
        assert_eq!(plan.subpaths().len(), 2);
        assert_eq!(plan.parameters()[plan.subpaths()[0].end - 1], -0.125);
        assert_eq!(plan.parameters()[plan.subpaths()[1].start], 0.125);
        let path = plan.evaluate(|t| [t, 1.0 / t], true).unwrap();
        let moves = path
            .commands()
            .iter()
            .filter(|command| matches!(command, PathCommand::MoveTo { .. }))
            .count();
        assert_eq!(moves, 2);
        assert!(path.is_finite());
    }

    #[test]
    fn boundary_discontinuity_preserves_sorted_pair_rule() {
        let mut options = PlotSamplingOptions::parametric(&[0.0, 1.0, 0.25]).unwrap();
        options.discontinuities = vec![0.0, -5.0, 5.0];
        options.dt = 0.125;
        let plan = options.plan().unwrap();
        assert_eq!(plan.parameters()[0], -0.125);
        assert_eq!(plan.parameters()[plan.subpaths()[0].end - 1], 0.0);
        assert_eq!(plan.parameters()[plan.subpaths()[1].start], 0.125);
        assert_eq!(plan.parameters().last(), Some(&1.0));
    }

    #[test]
    fn sampling_is_repeatable_and_evaluator_is_preparation_only() {
        let options = PlotSamplingOptions::parametric(&[0.0, 1.0, 0.25]).unwrap();
        let first = options.plan().unwrap();
        assert_eq!(first, options.plan().unwrap());
        let mut visited = Vec::new();
        let path = first
            .evaluate(
                |t| {
                    visited.push(t);
                    [t, t * t]
                },
                false,
            )
            .unwrap();
        assert_eq!(visited, first.parameters());
        assert_eq!(path.commands().len(), first.parameters().len());
    }

    #[test]
    fn nonfinite_or_unrenderable_samples_fail_explicitly() {
        let plan = PlotSamplingOptions::parametric(&[0.0, 1.0, 0.5])
            .unwrap()
            .plan()
            .unwrap();
        assert_eq!(
            plan.evaluate(|_| [f64::NAN, 0.0], false),
            Err(PlotPreparationError::InvalidPoint { sample_index: 0 })
        );
        assert_eq!(
            sampled_plot_path(&[[0.0, f64::MAX]]),
            Err(PlotPreparationError::InvalidPoint { sample_index: 0 })
        );
        assert!(matches!(
            plan.path_from_samples(&[[0.0, 0.0]], false),
            Err(PlotPreparationError::SampleCountMismatch { .. })
        ));
    }

    #[test]
    fn invalid_requests_and_excessive_samples_are_rejected() {
        for range in [
            [1.0, 0.0, 0.1],
            [0.0, 0.0, 0.1],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, -0.1],
            [0.0, f64::INFINITY, 0.1],
        ] {
            assert!(PlotSamplingOptions::parametric(&range).is_err());
        }
        let mut options = PlotSamplingOptions::parametric(&[0.0, 1.0, 0.25]).unwrap();
        options.max_samples = 4;
        assert_eq!(
            options.plan(),
            Err(PlotPreparationError::SampleLimitExceeded)
        );
        options.max_samples = 5;
        assert_eq!(options.plan().unwrap().parameters().len(), 5);
        options.dt = f64::NAN;
        assert_eq!(
            options.plan(),
            Err(PlotPreparationError::InvalidDiscontinuity)
        );
    }

    #[test]
    fn sampled_data_retains_input_order_and_repeated_x_values() {
        let path = sampled_plot_path(&[[1.0, 2.0], [1.0, 3.0], [0.0, 4.0]]).unwrap();
        assert_eq!(path.commands().len(), 3);
        assert!(matches!(
            path.commands()[2],
            PathCommand::LineTo { to } if to == Vec2::new(0.0, 4.0)
        ));
        assert!(sampled_plot_path(&[]).unwrap().commands().is_empty());
    }
}
