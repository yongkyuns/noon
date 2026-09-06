use serde::{Deserialize, Serialize};

use crate::RateFunction;

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
            if !step.duration.is_finite() || step.duration <= 0.0 {
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
        self.validate()?;
        let mut required_alpha = 0.0;
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
                "composition time-map step {index} duration must be finite and positive, got {value}"
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
    fn nonlinear_parent_rate_is_applied_before_interval_remap() {
        let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.0,
            1.0,
            RateFunction::Smooth,
        )]);
        assert!((map.evaluate(0.25).alpha - RateFunction::Smooth.evaluate(0.25)).abs() < 1e-6);
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
