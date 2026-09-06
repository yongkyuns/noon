use serde::{Deserialize, Serialize};

use crate::RateFunction;

/// Partial authoring-time animation options shared by every frontend.
///
/// This is transient authoring state. Resolved values are lowered into ordinary
/// explicit scene tracks; it is not a second persisted scene representation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnimationOptions {
    pub run_time: Option<f64>,
    pub rate_func: Option<RateFunction>,
    pub lag_ratio: Option<f64>,
    pub path_arc: Option<f64>,
    pub reverse_rate_function: Option<bool>,
    pub remover: Option<bool>,
    pub introducer: Option<bool>,
}

impl AnimationOptions {
    pub const fn new() -> Self {
        Self {
            run_time: None,
            rate_func: None,
            lag_ratio: None,
            path_arc: None,
            reverse_rate_function: None,
            remover: None,
            introducer: None,
        }
    }

    pub const fn run_time(mut self, value: f64) -> Self {
        self.run_time = Some(value);
        self
    }

    pub const fn rate_func(mut self, value: RateFunction) -> Self {
        self.rate_func = Some(value);
        self
    }

    pub const fn lag_ratio(mut self, value: f64) -> Self {
        self.lag_ratio = Some(value);
        self
    }

    pub const fn path_arc(mut self, value: f64) -> Self {
        self.path_arc = Some(value);
        self
    }

    pub const fn reverse_rate_function(mut self, value: bool) -> Self {
        self.reverse_rate_function = Some(value);
        self
    }

    pub const fn remover(mut self, value: bool) -> Self {
        self.remover = Some(value);
        self
    }

    pub const fn introducer(mut self, value: bool) -> Self {
        self.introducer = Some(value);
        self
    }
}

/// Concrete defaults applied before animation-local and `Scene.play` overrides.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationDefaults {
    pub run_time: f64,
    pub rate_func: RateFunction,
    pub lag_ratio: f64,
    pub path_arc: f64,
    pub reverse_rate_function: bool,
    pub remover: bool,
    pub introducer: bool,
}

impl AnimationDefaults {
    /// Common Manim `Animation` defaults. Subclasses may adjust fields such as
    /// `lag_ratio` before the same resolver is called.
    pub const MANIM: Self = Self {
        run_time: 1.0,
        rate_func: RateFunction::Smooth,
        lag_ratio: 0.0,
        path_arc: 0.0,
        reverse_rate_function: false,
        remover: false,
        introducer: false,
    };

    pub const fn lag_ratio(mut self, value: f64) -> Self {
        self.lag_ratio = value;
        self
    }

    pub const fn remover(mut self, value: bool) -> Self {
        self.remover = value;
        self
    }

    pub const fn introducer(mut self, value: bool) -> Self {
        self.introducer = value;
        self
    }
}

impl Default for AnimationDefaults {
    fn default() -> Self {
        Self::MANIM
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedAnimationOptions {
    pub run_time: f64,
    pub rate_func: RateFunction,
    pub lag_ratio: f64,
    pub path_arc: f64,
    pub reverse_rate_function: bool,
    pub remover: bool,
    pub introducer: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationOptionsError {
    InvalidRunTime(f64),
    InvalidLagRatio(f64),
    InvalidPathArc(f64),
    UnsupportedPathArc(f64),
    UnsupportedReverseRateFunction,
}

impl std::fmt::Display for AnimationOptionsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRunTime(value) => {
                write!(formatter, "run_time must be finite and positive, got {value}")
            }
            Self::InvalidLagRatio(value) => write!(
                formatter,
                "lag_ratio must be finite and non-negative, got {value}"
            ),
            Self::InvalidPathArc(value) => {
                write!(formatter, "path_arc must be finite, got {value}")
            }
            Self::UnsupportedPathArc(value) => write!(
                formatter,
                "non-zero path_arc={value} requires curved transform paths, which are not yet represented by Noon's deterministic Transform track"
            ),
            Self::UnsupportedReverseRateFunction => formatter.write_str(
                "reverse_rate_function=True is not yet represented by Noon's deterministic timing semantics",
            ),
        }
    }
}

impl std::error::Error for AnimationOptionsError {}

/// Apply the shared authoring precedence rule:
///
/// `defaults < animation-local options < Scene.play options`.
pub fn resolve_animation_options(
    defaults: AnimationDefaults,
    animation: AnimationOptions,
    play: AnimationOptions,
) -> Result<ResolvedAnimationOptions, AnimationOptionsError> {
    ResolvedAnimationOptions {
        run_time: play
            .run_time
            .or(animation.run_time)
            .unwrap_or(defaults.run_time),
        rate_func: play
            .rate_func
            .or(animation.rate_func)
            .unwrap_or(defaults.rate_func),
        lag_ratio: play
            .lag_ratio
            .or(animation.lag_ratio)
            .unwrap_or(defaults.lag_ratio),
        path_arc: play
            .path_arc
            .or(animation.path_arc)
            .unwrap_or(defaults.path_arc),
        reverse_rate_function: play
            .reverse_rate_function
            .or(animation.reverse_rate_function)
            .unwrap_or(defaults.reverse_rate_function),
        remover: play
            .remover
            .or(animation.remover)
            .unwrap_or(defaults.remover),
        introducer: play
            .introducer
            .or(animation.introducer)
            .unwrap_or(defaults.introducer),
    }
    .validate()
}

/// Resolve timing for targetless/structural instant leaves. The shared precedence rule is
/// unchanged; only an exactly-zero duration is admitted so `Add()` can carry an exact boundary.
pub fn resolve_add_animation_options(
    defaults: AnimationDefaults,
    animation: AnimationOptions,
    play: AnimationOptions,
) -> Result<ResolvedAnimationOptions, AnimationOptionsError> {
    let options = ResolvedAnimationOptions {
        run_time: play
            .run_time
            .or(animation.run_time)
            .unwrap_or(defaults.run_time),
        rate_func: play
            .rate_func
            .or(animation.rate_func)
            .unwrap_or(defaults.rate_func),
        lag_ratio: play
            .lag_ratio
            .or(animation.lag_ratio)
            .unwrap_or(defaults.lag_ratio),
        path_arc: play
            .path_arc
            .or(animation.path_arc)
            .unwrap_or(defaults.path_arc),
        reverse_rate_function: play
            .reverse_rate_function
            .or(animation.reverse_rate_function)
            .unwrap_or(defaults.reverse_rate_function),
        remover: play
            .remover
            .or(animation.remover)
            .unwrap_or(defaults.remover),
        introducer: play
            .introducer
            .or(animation.introducer)
            .unwrap_or(defaults.introducer),
    };
    if !options.run_time.is_finite() || options.run_time < 0.0 {
        return Err(AnimationOptionsError::InvalidRunTime(options.run_time));
    }
    options.validate_non_timing()
}

impl ResolvedAnimationOptions {
    pub fn validate(self) -> Result<Self, AnimationOptionsError> {
        if !self.run_time.is_finite() || self.run_time <= 0.0 {
            return Err(AnimationOptionsError::InvalidRunTime(self.run_time));
        }
        self.validate_non_timing()
    }

    fn validate_non_timing(self) -> Result<Self, AnimationOptionsError> {
        if !self.lag_ratio.is_finite() || self.lag_ratio < 0.0 {
            return Err(AnimationOptionsError::InvalidLagRatio(self.lag_ratio));
        }
        if !self.path_arc.is_finite() {
            return Err(AnimationOptionsError::InvalidPathArc(self.path_arc));
        }
        if self.path_arc.abs() > 1e-12 {
            return Err(AnimationOptionsError::UnsupportedPathArc(self.path_arc));
        }
        if self.reverse_rate_function {
            return Err(AnimationOptionsError::UnsupportedReverseRateFunction);
        }
        Ok(self)
    }
}

impl RateFunction {
    /// Stable identifier used by thin cross-language adapters.
    pub const fn semantic_id(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Smooth => "smooth",
            Self::RushInto => "rush_into",
            Self::RushFrom => "rush_from",
            Self::ThereAndBack => "there_and_back",
            Self::EaseInOutCubic => "ease_in_out_cubic",
            Self::StepStart => "step_start",
            Self::StepEnd => "step_end",
        }
    }

    pub fn from_semantic_id(value: &str) -> Option<Self> {
        match value {
            "linear" => Some(Self::Linear),
            "smooth" => Some(Self::Smooth),
            "rush_into" => Some(Self::RushInto),
            "rush_from" => Some(Self::RushFrom),
            "there_and_back" => Some(Self::ThereAndBack),
            "ease_in_out_cubic" => Some(Self::EaseInOutCubic),
            "step_start" => Some(Self::StepStart),
            "step_end" => Some(Self::StepEnd),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manim_defaults_are_shared_semantics() {
        let resolved = resolve_animation_options(
            AnimationDefaults::MANIM,
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .unwrap();

        assert_eq!(resolved.run_time, 1.0);
        assert_eq!(resolved.rate_func, RateFunction::Smooth);
        assert_eq!(resolved.lag_ratio, 0.0);
        assert_eq!(resolved.path_arc, 0.0);
        assert!(!resolved.reverse_rate_function);
    }

    #[test]
    fn scene_play_overrides_animation_local_options() {
        let resolved = resolve_animation_options(
            AnimationDefaults::MANIM.lag_ratio(1.0),
            AnimationOptions::new()
                .run_time(3.0)
                .rate_func(RateFunction::Linear)
                .lag_ratio(0.5),
            AnimationOptions::new()
                .run_time(0.4)
                .rate_func(RateFunction::Smooth),
        )
        .unwrap();

        assert_eq!(resolved.run_time, 0.4);
        assert_eq!(resolved.rate_func, RateFunction::Smooth);
        assert_eq!(resolved.lag_ratio, 0.5);
    }

    #[test]
    fn validation_and_unsupported_policy_are_shared() {
        assert_eq!(
            resolve_animation_options(
                AnimationDefaults::MANIM,
                AnimationOptions::new().lag_ratio(-0.1),
                AnimationOptions::new(),
            ),
            Err(AnimationOptionsError::InvalidLagRatio(-0.1))
        );
        assert!(matches!(
            resolve_animation_options(
                AnimationDefaults::MANIM,
                AnimationOptions::new().path_arc(0.5),
                AnimationOptions::new(),
            ),
            Err(AnimationOptionsError::UnsupportedPathArc(value)) if value == 0.5
        ));
        assert_eq!(
            resolve_animation_options(
                AnimationDefaults::MANIM,
                AnimationOptions::new().reverse_rate_function(true),
                AnimationOptions::new(),
            ),
            Err(AnimationOptionsError::UnsupportedReverseRateFunction)
        );
    }

    #[test]
    fn rate_function_semantic_ids_round_trip() {
        for rate_func in [
            RateFunction::Linear,
            RateFunction::Smooth,
            RateFunction::RushInto,
            RateFunction::RushFrom,
            RateFunction::ThereAndBack,
            RateFunction::EaseInOutCubic,
            RateFunction::StepStart,
            RateFunction::StepEnd,
        ] {
            assert_eq!(
                RateFunction::from_semantic_id(rate_func.semantic_id()),
                Some(rate_func)
            );
        }
    }
}
