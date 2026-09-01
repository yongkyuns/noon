use serde::{Deserialize, Serialize};

use crate::{resolve_uniform_composition_schedule, CompositionError, RateFunction};

/// Renderer- and frontend-independent timing for one uniform family animation.
///
/// Manim applies lag before the rate function for each family member. Keeping the
/// normalized child interval here preserves that ordering without making a renderer,
/// Python adapter, or Text resource own animation timing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct UniformFamilyAnimationPlan {
    member_count: u32,
    lag_ratio: f64,
    member_duration: f64,
    member_start_step: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FamilyAnimationProgressError {
    Composition(CompositionError),
    InvalidOverallProgress(f64),
    InvalidMemberIndex { index: u32, member_count: u32 },
}

impl std::fmt::Display for FamilyAnimationProgressError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Composition(error) => error.fmt(formatter),
            Self::InvalidOverallProgress(value) => {
                write!(
                    formatter,
                    "family animation progress must be finite, got {value}"
                )
            }
            Self::InvalidMemberIndex {
                index,
                member_count,
            } => write!(
                formatter,
                "family animation member index {index} is outside member count {member_count}"
            ),
        }
    }
}

impl std::error::Error for FamilyAnimationProgressError {}

impl From<CompositionError> for FamilyAnimationProgressError {
    fn from(value: CompositionError) -> Self {
        Self::Composition(value)
    }
}

impl UniformFamilyAnimationPlan {
    /// Build the same equal-child timing geometry used by the shared composition
    /// scheduler, normalized to an overall duration of one.
    pub fn new(member_count: u32, lag_ratio: f64) -> Result<Self, FamilyAnimationProgressError> {
        let schedule = resolve_uniform_composition_schedule(
            usize::try_from(member_count).expect("u32 member count must fit usize"),
            lag_ratio,
            1.0,
        )?;
        let first = schedule
            .intervals
            .first()
            .expect("non-empty family schedule must contain its first interval");
        let member_start_step = schedule
            .intervals
            .get(1)
            .map_or(0.0, |second| second.start_time - first.start_time);
        Ok(Self {
            member_count,
            lag_ratio,
            member_duration: first.duration,
            member_start_step,
        })
    }

    pub const fn member_count(self) -> u32 {
        self.member_count
    }

    pub const fn lag_ratio(self) -> f64 {
        self.lag_ratio
    }

    pub const fn member_duration(self) -> f64 {
        self.member_duration
    }

    pub const fn member_start_step(self) -> f64 {
        self.member_start_step
    }

    /// Return the member-local progress before any rate function is applied.
    ///
    /// Values intentionally remain outside `[0, 1]`; Manim's rate functions own
    /// endpoint handling. This is equivalent to v0.21 `Animation.get_sub_alpha`'s
    /// `value - lower` term, expressed through the shared resolved schedule.
    pub fn raw_member_progress(
        self,
        overall_progress: f64,
        index: u32,
    ) -> Result<f64, FamilyAnimationProgressError> {
        if !overall_progress.is_finite() {
            return Err(FamilyAnimationProgressError::InvalidOverallProgress(
                overall_progress,
            ));
        }
        if index >= self.member_count {
            return Err(FamilyAnimationProgressError::InvalidMemberIndex {
                index,
                member_count: self.member_count,
            });
        }
        let start = f64::from(index) * self.member_start_step;
        Ok((overall_progress - start) / self.member_duration)
    }

    /// Evaluate one family's visible sub-alpha using Manim's ordering:
    /// family lag -> optional rate reversal -> rate function.
    pub fn member_progress(
        self,
        overall_progress: f64,
        index: u32,
        rate_function: RateFunction,
        reverse_rate_function: bool,
    ) -> Result<f32, FamilyAnimationProgressError> {
        let raw = self.raw_member_progress(overall_progress, index)?;
        let input = if reverse_rate_function {
            1.0 - raw
        } else {
            raw
        };
        Ok(rate_function.evaluate(input as f32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn create_lag_one_matches_manim_family_sub_alpha() {
        let plan = UniformFamilyAnimationPlan::new(5, 1.0).unwrap();
        assert!((plan.member_duration() - 0.2).abs() < 1e-12);
        assert!((plan.member_start_step() - 0.2).abs() < 1e-12);

        let values = (0..5)
            .map(|index| {
                plan.member_progress(0.5, index, RateFunction::Linear, false)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        for (actual, expected) in values.into_iter().zip([1.0, 1.0, 0.5, 0.0, 0.0]) {
            close(actual, expected);
        }
    }

    #[test]
    fn uncreate_reverses_rate_without_reversing_family_order() {
        let plan = UniformFamilyAnimationPlan::new(5, 1.0).unwrap();
        let early = (0..5)
            .map(|index| {
                plan.member_progress(0.2, index, RateFunction::Linear, true)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        for (actual, expected) in early.into_iter().zip([0.0, 1.0, 1.0, 1.0, 1.0]) {
            close(actual, expected);
        }
    }

    #[test]
    fn easing_is_applied_after_member_lag_mapping() {
        let plan = UniformFamilyAnimationPlan::new(4, 0.2).unwrap();
        let raw = plan.raw_member_progress(0.35, 2).unwrap();
        let expected = RateFunction::Smooth.evaluate(raw as f32);
        close(
            plan.member_progress(0.35, 2, RateFunction::Smooth, false)
                .unwrap(),
            expected,
        );

        let globally_eased = f64::from(RateFunction::Smooth.evaluate(0.35));
        let wrong = plan.raw_member_progress(globally_eased, 2).unwrap();
        assert!((f64::from(expected) - wrong.clamp(0.0, 1.0)).abs() > 1e-3);
    }

    #[test]
    fn raw_progress_preserves_pre_and_post_interval_values() {
        let plan = UniformFamilyAnimationPlan::new(3, 1.0).unwrap();
        assert!(plan.raw_member_progress(0.0, 2).unwrap() < 0.0);
        assert!(plan.raw_member_progress(1.0, 0).unwrap() > 1.0);
        close(
            plan.member_progress(0.0, 2, RateFunction::Linear, false)
                .unwrap(),
            0.0,
        );
        close(
            plan.member_progress(1.0, 0, RateFunction::Linear, false)
                .unwrap(),
            1.0,
        );
    }

    #[test]
    fn invalid_inputs_fail_before_animation_state_is_used() {
        assert_eq!(
            UniformFamilyAnimationPlan::new(0, 0.0),
            Err(FamilyAnimationProgressError::Composition(
                CompositionError::Empty
            ))
        );
        assert_eq!(
            UniformFamilyAnimationPlan::new(1, -0.1),
            Err(FamilyAnimationProgressError::Composition(
                CompositionError::InvalidLagRatio(-0.1)
            ))
        );
        let plan = UniformFamilyAnimationPlan::new(2, 0.0).unwrap();
        assert!(matches!(
            plan.raw_member_progress(f64::NAN, 0),
            Err(FamilyAnimationProgressError::InvalidOverallProgress(value)) if value.is_nan()
        ));
        assert_eq!(
            plan.raw_member_progress(0.5, 2),
            Err(FamilyAnimationProgressError::InvalidMemberIndex {
                index: 2,
                member_count: 2,
            })
        );
    }
}
