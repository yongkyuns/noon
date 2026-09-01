use crate::{
    FamilyAnimationProgressError, TextFamilyAnimationError, TextFamilyAnimationState,
    UniformFamilyAnimationPlan,
};

/// Allocation-free evaluator for one retained Text family animation frame.
///
/// The animation plan is resolved once for the immutable rendered-member count, then
/// each glyph/submobject lookup is O(1). This avoids rebuilding uniform composition
/// timing for every member while preserving Manim's lag -> reverse-rate -> rate-function
/// ordering and independent reverse-member-order behavior.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextFamilyProgressEvaluator {
    state: TextFamilyAnimationState,
    plan: UniformFamilyAnimationPlan,
}

impl TextFamilyProgressEvaluator {
    pub fn new(
        state: TextFamilyAnimationState,
        member_count: u32,
    ) -> Result<Self, TextFamilyAnimationError> {
        state.validate()?;
        let plan = UniformFamilyAnimationPlan::new(member_count, state.lag_ratio)?;
        Ok(Self { state, plan })
    }

    pub const fn member_count(self) -> u32 {
        self.plan.member_count()
    }

    pub const fn state(self) -> TextFamilyAnimationState {
        self.state
    }

    pub fn member_progress(self, member_index: u32) -> Result<f32, TextFamilyAnimationError> {
        let member_count = self.plan.member_count();
        let scheduled_index = if self.state.reverse_member_order {
            member_count
                .checked_sub(1)
                .and_then(|last| last.checked_sub(member_index))
                .ok_or(FamilyAnimationProgressError::InvalidMemberIndex {
                    index: member_index,
                    member_count,
                })?
        } else {
            member_index
        };

        Ok(self.plan.member_progress(
            self.state.overall_progress,
            scheduled_index,
            self.state.rate_function,
            self.state.reverse_rate_function,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use crate::{RateFunction, TextFamilyAnimationMode};

    use super::*;

    fn state(
        overall_progress: f64,
        reverse_rate_function: bool,
        reverse_member_order: bool,
    ) -> TextFamilyAnimationState {
        TextFamilyAnimationState {
            mode: TextFamilyAnimationMode::Reveal,
            overall_progress,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function,
            reverse_member_order,
        }
    }

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn create_progress_matches_manim_lag_one_without_rebuilding_plan_per_member() {
        let evaluator = TextFamilyProgressEvaluator::new(state(0.5, false, false), 5).unwrap();
        assert_eq!(evaluator.member_count(), 5);
        for (index, expected) in [1.0, 1.0, 0.5, 0.0, 0.0].into_iter().enumerate() {
            close(evaluator.member_progress(index as u32).unwrap(), expected);
        }
    }

    #[test]
    fn uncreate_reverses_rate_without_reversing_member_order() {
        let evaluator = TextFamilyProgressEvaluator::new(state(0.2, true, false), 5).unwrap();
        for (index, expected) in [0.0, 1.0, 1.0, 1.0, 1.0].into_iter().enumerate() {
            close(evaluator.member_progress(index as u32).unwrap(), expected);
        }
    }

    #[test]
    fn reverse_member_order_is_independent_from_reverse_rate() {
        let evaluator = TextFamilyProgressEvaluator::new(state(0.2, false, true), 5).unwrap();
        close(evaluator.member_progress(0).unwrap(), 0.0);
        close(evaluator.member_progress(4).unwrap(), 1.0);

        let unwrite = TextFamilyProgressEvaluator::new(state(0.2, true, true), 5).unwrap();
        close(unwrite.member_progress(0).unwrap(), 1.0);
        close(unwrite.member_progress(4).unwrap(), 0.0);
    }

    #[test]
    fn invalid_state_and_member_inputs_fail_closed() {
        let mut invalid = state(0.5, false, false);
        invalid.overall_progress = 1.5;
        assert_eq!(
            TextFamilyProgressEvaluator::new(invalid, 5),
            Err(TextFamilyAnimationError::InvalidOverallProgress(1.5))
        );

        assert!(matches!(
            TextFamilyProgressEvaluator::new(state(0.5, false, false), 0),
            Err(TextFamilyAnimationError::FamilyProgress(_))
        ));

        let evaluator = TextFamilyProgressEvaluator::new(state(0.5, false, false), 5).unwrap();
        assert!(matches!(
            evaluator.member_progress(5),
            Err(TextFamilyAnimationError::FamilyProgress(
                FamilyAnimationProgressError::InvalidMemberIndex {
                    index: 5,
                    member_count: 5,
                }
            ))
        ));
    }
}
