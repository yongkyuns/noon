use serde::{Deserialize, Serialize};

use crate::{FamilyAnimationProgressError, ObjectId, RateFunction, UniformFamilyAnimationPlan};

/// Content-independent retained family animation operation.
///
/// Concrete resources decide how each operation is realized for one animation
/// member; scheduling, lag mapping, reversal, and easing remain shared semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FamilyAnimationMode {
    Reveal,
    DrawBorderThenFill,
}

/// Target-independent timing and scheduling semantics for one family animation.
///
/// Semantic families such as `VGroup`, axes, or graphs do not need to fabricate a
/// render `ObjectId` merely to own timing. Higher semantic layers pair this spec with
/// their authoritative target identity, while retained-object compatibility paths can
/// continue using [`FamilyAnimationDefinition`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyAnimationSpec {
    pub mode: FamilyAnimationMode,
    pub start_time: f64,
    pub duration: f64,
    pub lag_ratio: f64,
    pub rate_function: RateFunction,
    pub reverse_rate_function: bool,
    pub reverse_member_order: bool,
}

/// One retained-object family animation definition.
///
/// This keeps the existing object-targeted compatibility/wire shape while delegating
/// all timing and member-scheduling semantics to target-independent
/// [`FamilyAnimationSpec`]. Semantic family targets should pair their own identity
/// with the spec instead of inventing a fake retained object.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyAnimationDefinition {
    pub object: ObjectId,
    pub mode: FamilyAnimationMode,
    pub start_time: f64,
    pub duration: f64,
    pub lag_ratio: f64,
    pub rate_function: RateFunction,
    pub reverse_rate_function: bool,
    pub reverse_member_order: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FamilyAnimationError {
    InvalidStartTime(f64),
    InvalidDuration(f64),
    InvalidEndTime(f64),
    InvalidLagRatio(f64),
    InvalidEvaluationTime(f64),
    InvalidOverallProgress(f64),
    FamilyProgress(FamilyAnimationProgressError),
}

impl std::fmt::Display for FamilyAnimationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStartTime(value) => {
                write!(formatter, "invalid family animation start time {value}")
            }
            Self::InvalidDuration(value) => {
                write!(formatter, "invalid family animation duration {value}")
            }
            Self::InvalidEndTime(value) => {
                write!(formatter, "invalid family animation end time {value}")
            }
            Self::InvalidLagRatio(value) => {
                write!(formatter, "invalid family animation lag ratio {value}")
            }
            Self::InvalidEvaluationTime(value) => {
                write!(
                    formatter,
                    "invalid family animation evaluation time {value}"
                )
            }
            Self::InvalidOverallProgress(value) => {
                write!(formatter, "invalid family animation progress {value}")
            }
            Self::FamilyProgress(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for FamilyAnimationError {}

impl From<FamilyAnimationProgressError> for FamilyAnimationError {
    fn from(value: FamilyAnimationProgressError) -> Self {
        Self::FamilyProgress(value)
    }
}

impl FamilyAnimationSpec {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mode: FamilyAnimationMode,
        start_time: f64,
        duration: f64,
        lag_ratio: f64,
        rate_function: RateFunction,
        reverse_rate_function: bool,
        reverse_member_order: bool,
    ) -> Result<Self, FamilyAnimationError> {
        let spec = Self {
            mode,
            start_time,
            duration,
            lag_ratio,
            rate_function,
            reverse_rate_function,
            reverse_member_order,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Revalidate a spec after any serialization or semantic-lowering boundary.
    pub fn validate(self) -> Result<(), FamilyAnimationError> {
        if !self.start_time.is_finite() {
            return Err(FamilyAnimationError::InvalidStartTime(self.start_time));
        }
        if !self.duration.is_finite() || self.duration <= 0.0 {
            return Err(FamilyAnimationError::InvalidDuration(self.duration));
        }
        let end_time = self.start_time + self.duration;
        if !end_time.is_finite() {
            return Err(FamilyAnimationError::InvalidEndTime(end_time));
        }
        if !self.lag_ratio.is_finite() || self.lag_ratio < 0.0 {
            return Err(FamilyAnimationError::InvalidLagRatio(self.lag_ratio));
        }
        Ok(())
    }

    pub fn end_time(self) -> f64 {
        self.start_time + self.duration
    }

    /// Evaluate only family-level timeline position. Per-member lag and easing are
    /// preserved in the returned state and applied after lowering supplies the
    /// authoritative global member count/order.
    pub fn state_at(self, time: f64) -> Result<FamilyAnimationState, FamilyAnimationError> {
        self.validate()?;
        if !time.is_finite() {
            return Err(FamilyAnimationError::InvalidEvaluationTime(time));
        }
        let overall_progress = ((time - self.start_time) / self.duration).clamp(0.0, 1.0);
        Ok(FamilyAnimationState {
            mode: self.mode,
            overall_progress,
            lag_ratio: self.lag_ratio,
            rate_function: self.rate_function,
            reverse_rate_function: self.reverse_rate_function,
            reverse_member_order: self.reverse_member_order,
        })
    }
}

impl FamilyAnimationDefinition {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        object: ObjectId,
        mode: FamilyAnimationMode,
        start_time: f64,
        duration: f64,
        lag_ratio: f64,
        rate_function: RateFunction,
        reverse_rate_function: bool,
        reverse_member_order: bool,
    ) -> Result<Self, FamilyAnimationError> {
        let spec = FamilyAnimationSpec::new(
            mode,
            start_time,
            duration,
            lag_ratio,
            rate_function,
            reverse_rate_function,
            reverse_member_order,
        )?;
        Ok(Self::from_spec(object, spec))
    }

    pub const fn from_spec(object: ObjectId, spec: FamilyAnimationSpec) -> Self {
        Self {
            object,
            mode: spec.mode,
            start_time: spec.start_time,
            duration: spec.duration,
            lag_ratio: spec.lag_ratio,
            rate_function: spec.rate_function,
            reverse_rate_function: spec.reverse_rate_function,
            reverse_member_order: spec.reverse_member_order,
        }
    }

    pub const fn spec(self) -> FamilyAnimationSpec {
        FamilyAnimationSpec {
            mode: self.mode,
            start_time: self.start_time,
            duration: self.duration,
            lag_ratio: self.lag_ratio,
            rate_function: self.rate_function,
            reverse_rate_function: self.reverse_rate_function,
            reverse_member_order: self.reverse_member_order,
        }
    }

    /// Revalidate a definition after any serialization boundary.
    pub fn validate(self) -> Result<(), FamilyAnimationError> {
        self.spec().validate()
    }

    pub fn end_time(self) -> f64 {
        self.spec().end_time()
    }

    pub fn state_at(self, time: f64) -> Result<FamilyAnimationState, FamilyAnimationError> {
        self.spec().state_at(time)
    }
}

/// Evaluated content-independent frame state for one retained family animation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyAnimationState {
    pub mode: FamilyAnimationMode,
    pub overall_progress: f64,
    pub lag_ratio: f64,
    pub rate_function: RateFunction,
    pub reverse_rate_function: bool,
    pub reverse_member_order: bool,
}

impl FamilyAnimationState {
    /// Revalidate evaluated state after transport/deserialization.
    pub fn validate(self) -> Result<(), FamilyAnimationError> {
        if !self.overall_progress.is_finite() || !(0.0..=1.0).contains(&self.overall_progress) {
            return Err(FamilyAnimationError::InvalidOverallProgress(
                self.overall_progress,
            ));
        }
        if !self.lag_ratio.is_finite() || self.lag_ratio < 0.0 {
            return Err(FamilyAnimationError::InvalidLagRatio(self.lag_ratio));
        }
        Ok(())
    }

    pub fn member_progress(
        self,
        member_index: u32,
        member_count: u32,
    ) -> Result<f32, FamilyAnimationError> {
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

    fn spec(reverse_rate_function: bool, reverse_member_order: bool) -> FamilyAnimationSpec {
        FamilyAnimationSpec::new(
            FamilyAnimationMode::Reveal,
            2.0,
            4.0,
            1.0,
            RateFunction::Linear,
            reverse_rate_function,
            reverse_member_order,
        )
        .unwrap()
    }

    fn definition(
        reverse_rate_function: bool,
        reverse_member_order: bool,
    ) -> FamilyAnimationDefinition {
        FamilyAnimationDefinition::from_spec(
            ObjectId::new(7),
            spec(reverse_rate_function, reverse_member_order),
        )
    }

    #[test]
    fn target_free_spec_and_object_definition_share_identical_timing() {
        let spec = spec(false, false);
        let first = FamilyAnimationDefinition::from_spec(ObjectId::new(7), spec);
        let second = FamilyAnimationDefinition::from_spec(ObjectId::new(99), spec);

        assert_eq!(first.spec(), spec);
        assert_eq!(second.spec(), spec);
        assert_eq!(first.state_at(4.0), spec.state_at(4.0));
        assert_eq!(second.state_at(4.0), spec.state_at(4.0));
        assert_eq!(first.end_time(), spec.end_time());
        assert_eq!(second.end_time(), spec.end_time());
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
    fn lag_one_member_progress_is_content_independent() {
        let state = definition(false, false).state_at(4.0).unwrap();
        let values = (0..5)
            .map(|index| state.member_progress(index, 5).unwrap())
            .collect::<Vec<_>>();
        for (actual, expected) in values.into_iter().zip([1.0, 1.0, 0.5, 0.0, 0.0]) {
            close(actual, expected);
        }
    }

    #[test]
    fn rate_reversal_and_member_reversal_are_independent() {
        let reversed_rate = definition(true, false).state_at(4.0).unwrap();
        let rate_values = (0..5)
            .map(|index| reversed_rate.member_progress(index, 5).unwrap())
            .collect::<Vec<_>>();
        for (actual, expected) in rate_values.into_iter().zip([0.0, 0.0, 0.5, 1.0, 1.0]) {
            close(actual, expected);
        }

        let reversed_members = definition(false, true).state_at(2.8).unwrap();
        close(reversed_members.member_progress(0, 5).unwrap(), 0.0);
        close(reversed_members.member_progress(4, 5).unwrap(), 1.0);

        let both = definition(true, true).state_at(2.8).unwrap();
        close(both.member_progress(0, 5).unwrap(), 1.0);
        close(both.member_progress(4, 5).unwrap(), 0.0);
    }

    #[test]
    fn serialized_definition_spec_and_state_are_revalidated() {
        let mut invalid_definition = definition(false, false);
        invalid_definition.duration = f64::INFINITY;
        assert_eq!(
            invalid_definition.validate(),
            Err(FamilyAnimationError::InvalidDuration(f64::INFINITY))
        );
        assert_eq!(
            invalid_definition.state_at(3.0),
            Err(FamilyAnimationError::InvalidDuration(f64::INFINITY))
        );

        let mut invalid_spec = spec(false, false);
        invalid_spec.duration = f64::INFINITY;
        assert_eq!(
            invalid_spec.validate(),
            Err(FamilyAnimationError::InvalidDuration(f64::INFINITY))
        );
        assert_eq!(
            invalid_spec.state_at(3.0),
            Err(FamilyAnimationError::InvalidDuration(f64::INFINITY))
        );

        let mut invalid_state = definition(false, false).state_at(3.0).unwrap();
        invalid_state.overall_progress = 1.5;
        assert_eq!(
            invalid_state.validate(),
            Err(FamilyAnimationError::InvalidOverallProgress(1.5))
        );
        assert_eq!(
            invalid_state.member_progress(0, 1),
            Err(FamilyAnimationError::InvalidOverallProgress(1.5))
        );
    }

    #[test]
    fn invalid_timing_and_member_inputs_fail_closed() {
        assert!(matches!(
            FamilyAnimationSpec::new(
                FamilyAnimationMode::Reveal,
                f64::NAN,
                1.0,
                0.0,
                RateFunction::Linear,
                false,
                false,
            ),
            Err(FamilyAnimationError::InvalidStartTime(value)) if value.is_nan()
        ));
        assert_eq!(
            FamilyAnimationDefinition::new(
                ObjectId::new(1),
                FamilyAnimationMode::Reveal,
                0.0,
                0.0,
                0.0,
                RateFunction::Linear,
                false,
                false,
            ),
            Err(FamilyAnimationError::InvalidDuration(0.0))
        );
        assert_eq!(
            FamilyAnimationSpec::new(
                FamilyAnimationMode::Reveal,
                f64::MAX,
                f64::MAX,
                0.0,
                RateFunction::Linear,
                false,
                false,
            ),
            Err(FamilyAnimationError::InvalidEndTime(f64::INFINITY))
        );
        assert_eq!(
            FamilyAnimationSpec::new(
                FamilyAnimationMode::Reveal,
                0.0,
                1.0,
                -0.1,
                RateFunction::Linear,
                false,
                false,
            ),
            Err(FamilyAnimationError::InvalidLagRatio(-0.1))
        );

        let state = definition(false, false).state_at(3.0).unwrap();
        assert!(matches!(
            state.member_progress(0, 0),
            Err(FamilyAnimationError::FamilyProgress(
                FamilyAnimationProgressError::Composition(_)
            ))
        ));
        assert!(matches!(
            state.member_progress(5, 5),
            Err(FamilyAnimationError::FamilyProgress(
                FamilyAnimationProgressError::InvalidMemberIndex {
                    index: 5,
                    member_count: 5,
                }
            ))
        ));
        assert!(matches!(
            spec(false, false).state_at(f64::INFINITY),
            Err(FamilyAnimationError::InvalidEvaluationTime(value)) if value.is_infinite()
        ));
    }
}
