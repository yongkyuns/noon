use serde::{Deserialize, Serialize};

use crate::{FamilyAnimationProgressError, ObjectId, RateFunction, UniformFamilyAnimationPlan};

/// Renderer-visible behavior for one retained Text family animation.
///
/// `Reveal` is the pointwise partial-path behavior used by Create/Uncreate.
/// `DrawBorderThenFill` is intentionally distinct for Write/Unwrite; it must not be
/// approximated by the same reveal mode merely because both animate family members.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFamilyAnimationMode {
    Reveal,
    DrawBorderThenFill,
}

/// One source-level retained Text family animation.
///
/// The overall timeline remains object-level, while lag/easing are preserved until
/// the renderer evaluates individual shaped members. This is required because Manim
/// applies `rate_function` after per-member lag mapping rather than to one global
/// reveal scalar.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextFamilyAnimationDefinition {
    pub object: ObjectId,
    pub mode: TextFamilyAnimationMode,
    pub start_time: f64,
    pub duration: f64,
    pub lag_ratio: f64,
    pub rate_function: RateFunction,
    pub reverse_rate_function: bool,
    pub reverse_member_order: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextFamilyAnimationError {
    InvalidStartTime(f64),
    InvalidDuration(f64),
    InvalidEndTime(f64),
    InvalidLagRatio(f64),
    InvalidEvaluationTime(f64),
    InvalidOverallProgress(f64),
    FamilyProgress(FamilyAnimationProgressError),
}

impl std::fmt::Display for TextFamilyAnimationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStartTime(value) => {
                write!(
                    formatter,
                    "invalid Text family animation start time {value}"
                )
            }
            Self::InvalidDuration(value) => {
                write!(formatter, "invalid Text family animation duration {value}")
            }
            Self::InvalidEndTime(value) => {
                write!(formatter, "invalid Text family animation end time {value}")
            }
            Self::InvalidLagRatio(value) => {
                write!(formatter, "invalid Text family animation lag ratio {value}")
            }
            Self::InvalidEvaluationTime(value) => {
                write!(
                    formatter,
                    "invalid Text family animation evaluation time {value}"
                )
            }
            Self::InvalidOverallProgress(value) => {
                write!(formatter, "invalid Text family animation progress {value}")
            }
            Self::FamilyProgress(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TextFamilyAnimationError {}

impl From<FamilyAnimationProgressError> for TextFamilyAnimationError {
    fn from(value: FamilyAnimationProgressError) -> Self {
        Self::FamilyProgress(value)
    }
}

impl TextFamilyAnimationDefinition {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        object: ObjectId,
        mode: TextFamilyAnimationMode,
        start_time: f64,
        duration: f64,
        lag_ratio: f64,
        rate_function: RateFunction,
        reverse_rate_function: bool,
        reverse_member_order: bool,
    ) -> Result<Self, TextFamilyAnimationError> {
        let definition = Self {
            object,
            mode,
            start_time,
            duration,
            lag_ratio,
            rate_function,
            reverse_rate_function,
            reverse_member_order,
        };
        definition.validate()?;
        Ok(definition)
    }

    /// Revalidate a definition after any serialization boundary.
    pub fn validate(self) -> Result<(), TextFamilyAnimationError> {
        if !self.start_time.is_finite() {
            return Err(TextFamilyAnimationError::InvalidStartTime(self.start_time));
        }
        if !self.duration.is_finite() || self.duration <= 0.0 {
            return Err(TextFamilyAnimationError::InvalidDuration(self.duration));
        }
        let end_time = self.start_time + self.duration;
        if !end_time.is_finite() {
            return Err(TextFamilyAnimationError::InvalidEndTime(end_time));
        }
        if !self.lag_ratio.is_finite() || self.lag_ratio < 0.0 {
            return Err(TextFamilyAnimationError::InvalidLagRatio(self.lag_ratio));
        }
        Ok(())
    }

    pub fn end_time(self) -> f64 {
        self.start_time + self.duration
    }

    /// Evaluate only the object-level timeline position. Per-member lag and easing
    /// remain encoded in the returned state and are applied later against the shaped
    /// resource's actual animation-member count.
    pub fn state_at(self, time: f64) -> Result<TextFamilyAnimationState, TextFamilyAnimationError> {
        self.validate()?;
        if !time.is_finite() {
            return Err(TextFamilyAnimationError::InvalidEvaluationTime(time));
        }
        let overall_progress = ((time - self.start_time) / self.duration).clamp(0.0, 1.0);
        Ok(TextFamilyAnimationState {
            mode: self.mode,
            overall_progress,
            lag_ratio: self.lag_ratio,
            rate_function: self.rate_function,
            reverse_rate_function: self.reverse_rate_function,
            reverse_member_order: self.reverse_member_order,
        })
    }
}

/// Evaluated frame state for one retained Text family animation.
///
/// This structure is transport-friendly but contains no glyph IDs. The renderer
/// combines it with derived [`crate::TextAnimationMember`] data from the immutable
/// Text resource, preserving one semantic scene object and avoiding per-frame Python
/// callbacks.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextFamilyAnimationState {
    pub mode: TextFamilyAnimationMode,
    pub overall_progress: f64,
    pub lag_ratio: f64,
    pub rate_function: RateFunction,
    pub reverse_rate_function: bool,
    pub reverse_member_order: bool,
}

impl TextFamilyAnimationState {
    /// Revalidate evaluated state after transport/deserialization.
    pub fn validate(self) -> Result<(), TextFamilyAnimationError> {
        if !self.overall_progress.is_finite() || !(0.0..=1.0).contains(&self.overall_progress) {
            return Err(TextFamilyAnimationError::InvalidOverallProgress(
                self.overall_progress,
            ));
        }
        if !self.lag_ratio.is_finite() || self.lag_ratio < 0.0 {
            return Err(TextFamilyAnimationError::InvalidLagRatio(self.lag_ratio));
        }
        Ok(())
    }

    pub fn member_progress(
        self,
        member_index: u32,
        member_count: u32,
    ) -> Result<f32, TextFamilyAnimationError> {
        self.validate()?;
        let scheduled_index = if self.reverse_member_order {
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
        let plan = UniformFamilyAnimationPlan::new(member_count, self.lag_ratio)?;
        Ok(plan.member_progress(
            self.overall_progress,
            scheduled_index,
            self.rate_function,
            self.reverse_rate_function,
        )?)
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

    fn definition(
        reverse_rate_function: bool,
        reverse_member_order: bool,
    ) -> TextFamilyAnimationDefinition {
        TextFamilyAnimationDefinition::new(
            ObjectId::new(7),
            TextFamilyAnimationMode::Reveal,
            2.0,
            4.0,
            1.0,
            RateFunction::Linear,
            reverse_rate_function,
            reverse_member_order,
        )
        .unwrap()
    }

    #[test]
    fn object_timeline_progress_is_clamped_but_not_eased() {
        let animation = definition(false, false);
        assert_eq!(animation.state_at(1.0).unwrap().overall_progress, 0.0);
        assert_eq!(animation.state_at(4.0).unwrap().overall_progress, 0.5);
        assert_eq!(animation.state_at(7.0).unwrap().overall_progress, 1.0);
        assert_eq!(animation.end_time(), 6.0);
    }

    #[test]
    fn create_member_progress_matches_manim_lag_one() {
        let state = definition(false, false).state_at(4.0).unwrap();
        let values = (0..5)
            .map(|index| state.member_progress(index, 5).unwrap())
            .collect::<Vec<_>>();
        for (actual, expected) in values.into_iter().zip([1.0, 1.0, 0.5, 0.0, 0.0]) {
            close(actual, expected);
        }
    }

    #[test]
    fn uncreate_reverses_rate_without_reversing_member_order() {
        let state = definition(true, false).state_at(4.0).unwrap();
        let values = (0..5)
            .map(|index| state.member_progress(index, 5).unwrap())
            .collect::<Vec<_>>();
        for (actual, expected) in values.into_iter().zip([0.0, 0.0, 0.5, 1.0, 1.0]) {
            close(actual, expected);
        }
    }

    #[test]
    fn reversed_write_order_is_independent_from_rate_reversal() {
        let state = definition(false, true).state_at(2.8).unwrap();
        close(state.member_progress(0, 5).unwrap(), 0.0);
        close(state.member_progress(4, 5).unwrap(), 1.0);

        let unwrite = definition(true, true).state_at(2.8).unwrap();
        close(unwrite.member_progress(0, 5).unwrap(), 1.0);
        close(unwrite.member_progress(4, 5).unwrap(), 0.0);
    }

    #[test]
    fn serialized_definition_and_state_are_revalidated() {
        let mut invalid_definition = definition(false, false);
        invalid_definition.duration = f64::INFINITY;
        assert_eq!(
            invalid_definition.validate(),
            Err(TextFamilyAnimationError::InvalidDuration(f64::INFINITY))
        );
        assert_eq!(
            invalid_definition.state_at(3.0),
            Err(TextFamilyAnimationError::InvalidDuration(f64::INFINITY))
        );

        let mut invalid_state = definition(false, false).state_at(3.0).unwrap();
        invalid_state.overall_progress = 1.5;
        assert_eq!(
            invalid_state.validate(),
            Err(TextFamilyAnimationError::InvalidOverallProgress(1.5))
        );
        assert_eq!(
            invalid_state.member_progress(0, 1),
            Err(TextFamilyAnimationError::InvalidOverallProgress(1.5))
        );
    }

    #[test]
    fn invalid_timing_and_member_inputs_fail_closed() {
        assert!(matches!(
            TextFamilyAnimationDefinition::new(
                ObjectId::new(1),
                TextFamilyAnimationMode::Reveal,
                f64::NAN,
                1.0,
                0.0,
                RateFunction::Linear,
                false,
                false,
            ),
            Err(TextFamilyAnimationError::InvalidStartTime(value)) if value.is_nan()
        ));
        assert_eq!(
            TextFamilyAnimationDefinition::new(
                ObjectId::new(1),
                TextFamilyAnimationMode::Reveal,
                0.0,
                0.0,
                0.0,
                RateFunction::Linear,
                false,
                false,
            ),
            Err(TextFamilyAnimationError::InvalidDuration(0.0))
        );
        assert_eq!(
            TextFamilyAnimationDefinition::new(
                ObjectId::new(1),
                TextFamilyAnimationMode::Reveal,
                f64::MAX,
                f64::MAX,
                0.0,
                RateFunction::Linear,
                false,
                false,
            ),
            Err(TextFamilyAnimationError::InvalidEndTime(f64::INFINITY))
        );
        assert_eq!(
            TextFamilyAnimationDefinition::new(
                ObjectId::new(1),
                TextFamilyAnimationMode::Reveal,
                0.0,
                1.0,
                -0.1,
                RateFunction::Linear,
                false,
                false,
            ),
            Err(TextFamilyAnimationError::InvalidLagRatio(-0.1))
        );

        let state = definition(false, false).state_at(3.0).unwrap();
        assert!(matches!(
            state.member_progress(0, 0),
            Err(TextFamilyAnimationError::FamilyProgress(
                FamilyAnimationProgressError::Composition(_)
            ))
        ));
        assert!(matches!(
            state.member_progress(5, 5),
            Err(TextFamilyAnimationError::FamilyProgress(
                FamilyAnimationProgressError::InvalidMemberIndex {
                    index: 5,
                    member_count: 5,
                }
            ))
        ));
        assert!(matches!(
            definition(false, false).state_at(f64::INFINITY),
            Err(TextFamilyAnimationError::InvalidEvaluationTime(value)) if value.is_infinite()
        ));
    }
}
