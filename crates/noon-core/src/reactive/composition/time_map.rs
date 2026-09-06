use serde::{Deserialize, Serialize};

use crate::RateFunction;
use crate::TrackTiming;

/// One root-to-leaf remapping step inside a nested animation composition.
///
/// The parent animation's normalized alpha is first warped by `rate_func`, then
/// remapped into this child's normalized interval. `start` and `duration` are
/// expressed as fractions of the parent composition's virtual run time.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompositionTimeMapStep {
    pub start: f64,
    pub duration: f64,
    pub rate_func: RateFunction,
}

impl CompositionTimeMapStep {
    pub const fn new(start: f64, duration: f64, rate_func: RateFunction) -> Self {
        Self {
            start,
            duration,
            rate_func,
        }
    }

    fn evaluate(self, alpha: f32) -> CompositionTimeSample {
        let warped = f64::from(self.rate_func.evaluate(alpha));
        if warped < self.start {
            return CompositionTimeSample::before();
        }
        if self.duration <= 0.0 {
            return CompositionTimeSample {
                alpha: 1.0,
                begun: true,
                finished: true,
            };
        }
        CompositionTimeSample {
            alpha: ((warped - self.start) / self.duration).clamp(0.0, 1.0) as f32,
            begun: true,
            finished: warped > self.start + self.duration,
        }
    }
}

/// Deterministic nested composition timing carried by a leaf track.
///
/// Ordinary tracks use the identity map (no steps). Composition lowering adds
/// one step per group boundary, preserving nonlinear and reversing group rate
/// functions without executing frontend code during playback.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CompositionTimeMap {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<CompositionTimeMapStep>,
}

impl CompositionTimeMap {
    pub const fn identity() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn from_steps(steps: Vec<CompositionTimeMapStep>) -> Self {
        Self { steps }
    }

    pub fn is_identity(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn push(&mut self, step: CompositionTimeMapStep) {
        self.steps.push(step);
    }

    pub fn validate(&self) -> Result<(), CompositionTimeMapError> {
        for (index, step) in self.steps.iter().copied().enumerate() {
            if !step.start.is_finite() || step.start < 0.0 || step.start > 1.0 {
                return Err(CompositionTimeMapError::InvalidStart {
                    index,
                    value: step.start,
                });
            }
            if !step.duration.is_finite() || step.duration < 0.0 {
                return Err(CompositionTimeMapError::InvalidDuration {
                    index,
                    value: step.duration,
                });
            }
            if step.start + step.duration > 1.0 + 1e-9 {
                return Err(CompositionTimeMapError::IntervalOutsideParent { index });
            }
        }
        Ok(())
    }

    pub fn evaluate(&self, alpha: f32) -> CompositionTimeSample {
        let mut sample = CompositionTimeSample {
            alpha: alpha.clamp(0.0, 1.0),
            begun: true,
            finished: alpha >= 1.0,
        };
        for step in &self.steps {
            if !sample.begun {
                return sample;
            }
            sample = step.evaluate(sample.alpha);
        }
        sample
    }

    /// Resolve the root alpha at which a discrete leaf first begins.
    ///
    /// Discrete events cannot sample a time map every frame: doing so would make
    /// an event repeat when a parent rate reverses. Compilation therefore turns a
    /// supported map into one ordinary scheduler timestamp. The supported rate
    /// functions are continuous and monotone, so walking the nested intervals
    /// from leaf to root has one deterministic lower boundary.
    pub fn monotone_event_alpha(&self) -> Result<f64, CompositionTimeMapError> {
        self.monotone_root_alpha(0.0)
    }

    /// Resolve the root alpha corresponding to one leaf-local alpha for a monotone map.
    pub fn monotone_root_alpha(&self, leaf_alpha: f64) -> Result<f64, CompositionTimeMapError> {
        self.validate()?;
        let mut required_alpha = leaf_alpha.clamp(0.0, 1.0);
        for (index, step) in self.steps.iter().copied().enumerate().rev() {
            let required_warp = step.start + step.duration * required_alpha;
            required_alpha = inverse_monotone_rate(step.rate_func, required_warp).ok_or(
                CompositionTimeMapError::UnsupportedDiscreteRate {
                    index,
                    rate_func: step.rate_func,
                },
            )?;
        }
        Ok(required_alpha)
    }
}

pub fn continuous_time_map_interval(
    timing: TrackTiming,
    time_map: &CompositionTimeMap,
) -> Result<(f64, f64), CompositionTimeMapError> {
    time_map.validate()?;
    let (start_alpha, end_alpha) = match (
        time_map.monotone_root_alpha(0.0),
        time_map.monotone_root_alpha(1.0),
    ) {
        (Ok(start), Ok(end)) => (start, end),
        (Err(CompositionTimeMapError::UnsupportedDiscreteRate { .. }), _)
        | (_, Err(CompositionTimeMapError::UnsupportedDiscreteRate { .. })) => (0.0, 1.0),
        (Err(error), _) | (_, Err(error)) => return Err(error),
    };
    Ok((
        timing.start_time + timing.duration * start_alpha,
        timing.start_time + timing.duration * end_alpha,
    ))
}

fn inverse_monotone_rate(rate_func: RateFunction, value: f64) -> Option<f64> {
    let value = value.clamp(0.0, 1.0);
    match rate_func {
        RateFunction::Linear => Some(value),
        RateFunction::Smooth
        | RateFunction::RushInto
        | RateFunction::RushFrom
        | RateFunction::EaseInOutCubic => {
            if value <= 0.0 {
                return Some(0.0);
            }
            if value >= 1.0 {
                return Some(1.0);
            }
            let mut lower = 0.0_f64;
            let mut upper = 1.0_f64;
            // Evaluation is defined in f32. A fixed search derives the earliest
            // representable root boundary whose shared evaluator reaches value.
            for _ in 0..64 {
                let middle = lower + (upper - lower) * 0.5;
                if f64::from(rate_func.evaluate(middle as f32)) < value {
                    lower = middle;
                } else {
                    upper = middle;
                }
            }
            Some(upper)
        }
        RateFunction::ThereAndBack | RateFunction::StepStart | RateFunction::StepEnd => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompositionTimeSample {
    pub alpha: f32,
    pub begun: bool,
    pub finished: bool,
}

impl CompositionTimeSample {
    const fn before() -> Self {
        Self {
            alpha: 0.0,
            begun: false,
            finished: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompositionTimeMapError {
    InvalidStart {
        index: usize,
        value: f64,
    },
    InvalidDuration {
        index: usize,
        value: f64,
    },
    IntervalOutsideParent {
        index: usize,
    },
    UnsupportedDiscreteRate {
        index: usize,
        rate_func: RateFunction,
    },
}

impl std::fmt::Display for CompositionTimeMapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStart { index, value } => write!(
                formatter,
                "composition time-map step {index} start must be finite and within [0, 1], got {value}"
            ),
            Self::InvalidDuration { index, value } => write!(
                formatter,
                "composition time-map step {index} duration must be finite and non-negative, got {value}"
            ),
            Self::IntervalOutsideParent { index } => write!(
                formatter,
                "composition time-map step {index} extends outside its parent interval"
            ),
            Self::UnsupportedDiscreteRate {
                index,
                rate_func,
            } => write!(
                formatter,
                "composition time-map step {index} uses {rate_func:?}, which has no single deterministic discrete-event boundary"
            ),
        }
    }
}

impl std::error::Error for CompositionTimeMapError {}

/// Evaluate one continuous leaf against its root interval and nested composition map.
///
/// `None` means the mapped leaf has not begun. Exact root endpoints settle to the
/// authored target even for reversing parent rates, matching runtime finish/seek semantics.
pub fn mapped_continuous_progress(
    timing: TrackTiming,
    time_map: &CompositionTimeMap,
    time: f64,
) -> Option<f32> {
    if time < timing.start_time {
        return None;
    }
    if timing.is_instant() {
        return Some(1.0);
    }
    if time >= timing.start_time + timing.duration {
        return Some(1.0);
    }
    let raw = ((time - timing.start_time) / timing.duration).clamp(0.0, 1.0) as f32;
    if time_map.is_identity() {
        return Some(timing.easing.evaluate(raw));
    }
    let sample = time_map.evaluate(raw);
    sample.begun.then(|| timing.easing.evaluate(sample.alpha))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_interval_maps_parent_alpha() {
        let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.25,
            0.5,
            RateFunction::Linear,
        )]);
        assert!(!map.evaluate(0.2).begun);
        assert_eq!(map.evaluate(0.25).alpha, 0.0);
        assert_eq!(map.evaluate(0.5).alpha, 0.5);
        assert_eq!(map.evaluate(0.75).alpha, 1.0);
        assert!(map.evaluate(0.9).finished);
    }

    #[test]
    fn zero_width_interval_is_an_exact_instant_boundary() {
        let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.5,
            0.0,
            RateFunction::Linear,
        )]);
        assert!(map.validate().is_ok());
        assert!(!map.evaluate(0.5 - f32::EPSILON).begun);
        assert_eq!(
            map.evaluate(0.5),
            CompositionTimeSample {
                alpha: 1.0,
                begun: true,
                finished: true,
            }
        );
        assert_eq!(map.evaluate(0.75).alpha, 1.0);
        assert_eq!(map.monotone_event_alpha(), Ok(0.5));
    }

    #[test]
    fn zero_width_root_end_event_resolves_to_one() {
        let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            1.0,
            0.0,
            RateFunction::Smooth,
        )]);
        // Smooth uses f32 and may round to one just before the endpoint. The
        // discrete compiler boundary remains exactly the root endpoint.
        assert!(!map.evaluate(0.9).begun);
        assert!(map.evaluate(1.0).begun);
        assert_eq!(map.monotone_event_alpha(), Ok(1.0));
    }

    #[test]
    fn negative_and_non_finite_widths_remain_invalid() {
        for duration in [-f64::EPSILON, f64::NAN, f64::INFINITY] {
            let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                0.5,
                duration,
                RateFunction::Linear,
            )]);
            assert!(matches!(
                map.validate(),
                Err(CompositionTimeMapError::InvalidDuration { .. })
            ));
        }
    }

    #[test]
    fn nonlinear_parent_rate_is_applied_before_interval_remap() {
        let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.0,
            1.0,
            RateFunction::Smooth,
        )]);
        assert!((map.evaluate(0.25).alpha - RateFunction::Smooth.evaluate(0.25)).abs() < 1e-6);
    }

    #[test]
    fn shared_mapped_progress_honors_child_delay_and_root_endpoint() {
        let timing = TrackTiming::new(2.0, 4.0, RateFunction::Linear);
        let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.5,
            0.5,
            RateFunction::Smooth,
        )]);
        assert_eq!(mapped_continuous_progress(timing, &map, 2.5), None);
        assert_eq!(mapped_continuous_progress(timing, &map, 6.0), Some(1.0));
        let progress = mapped_continuous_progress(timing, &map, 5.0).unwrap();
        assert!(progress > 0.0 && progress < 1.0);
    }

    #[test]
    fn reversing_parent_rate_reopens_earlier_child() {
        let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.0,
            0.5,
            RateFunction::ThereAndBack,
        )]);
        assert_eq!(map.evaluate(0.25).alpha, 1.0);
        assert_eq!(map.evaluate(0.5).alpha, 1.0);
        assert_eq!(map.evaluate(0.75).alpha, 1.0);
        assert!(map.evaluate(0.9).alpha < 1.0);
        assert_eq!(map.evaluate(1.0).alpha, 0.0);
    }

    #[test]
    fn nested_steps_compose_root_to_leaf() {
        let map = CompositionTimeMap::from_steps(vec![
            CompositionTimeMapStep::new(0.0, 1.0, RateFunction::Linear),
            CompositionTimeMapStep::new(0.5, 0.5, RateFunction::Linear),
        ]);
        assert!(!map.evaluate(0.25).begun);
        assert_eq!(map.evaluate(0.75).alpha, 0.5);
    }

    #[test]
    fn nested_monotone_event_boundary_is_resolved_inside_out() {
        let map = CompositionTimeMap::from_steps(vec![
            CompositionTimeMapStep::new(0.2, 0.6, RateFunction::Smooth),
            CompositionTimeMapStep::new(0.5, 0.5, RateFunction::Linear),
        ]);
        let boundary = map.monotone_event_alpha().unwrap();
        assert!(!map.evaluate((boundary - 1e-6) as f32).begun);
        assert!(map.evaluate(boundary as f32).begun);
    }

    #[test]
    fn reversing_and_discontinuous_rates_have_no_single_event_boundary() {
        for rate_func in [
            RateFunction::ThereAndBack,
            RateFunction::StepStart,
            RateFunction::StepEnd,
        ] {
            let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                0.25, 0.5, rate_func,
            )]);
            assert!(matches!(
                map.monotone_event_alpha(),
                Err(CompositionTimeMapError::UnsupportedDiscreteRate { .. })
            ));
        }
    }
}
