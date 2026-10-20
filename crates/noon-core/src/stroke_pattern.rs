//! Retained periodic stroke appearance in local scene units.
//!
//! Phase is unwrapped authored state. The ordinary timeline interpolates it;
//! only renderer packing reduces it modulo the period. This type owns no clock.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokePattern {
    dash_length: f32,
    gap_length: f32,
    phase: f32,
    gap_opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokePatternError {
    InvalidDashLength,
    InvalidGapLength,
    InvalidPhase,
    InvalidGapOpacity,
    InvalidPeriod,
}

impl std::fmt::Display for StrokePatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidDashLength => {
                "dash length must be finite and non-negative (zero selects dots)"
            }
            Self::InvalidGapLength => "gap or point spacing must be finite and positive",
            Self::InvalidPhase => "stroke phase must be finite and representable",
            Self::InvalidGapOpacity => "gap opacity must be finite and in [0, 1]",
            Self::InvalidPeriod => "stroke pattern period must be finite and positive",
        })
    }
}
impl std::error::Error for StrokePatternError {}

impl StrokePattern {
    /// A dash followed by a gap. Gaps retain 20% stroke alpha by default.
    pub fn dashed(dash_length: f32, gap_length: f32) -> Result<Self, StrokePatternError> {
        if dash_length <= 0.0 {
            return Err(StrokePatternError::InvalidDashLength);
        }
        let result = Self {
            dash_length,
            gap_length,
            phase: 0.0,
            gap_opacity: 0.2,
        };
        result.validate()?;
        Ok(result)
    }

    /// Dot diameter follows stroke width; spacing is center-to-center.
    pub fn dotted(spacing: f32) -> Result<Self, StrokePatternError> {
        let result = Self {
            dash_length: 0.0,
            gap_length: spacing,
            phase: 0.0,
            gap_opacity: 0.2,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn with_phase(mut self, phase: f32) -> Result<Self, StrokePatternError> {
        self.phase = phase;
        self.validate()?;
        Ok(self)
    }

    pub fn with_gap_opacity(mut self, opacity: f32) -> Result<Self, StrokePatternError> {
        self.gap_opacity = opacity;
        self.validate()?;
        Ok(self)
    }

    pub const fn dash_length(self) -> f32 {
        self.dash_length
    }
    pub const fn gap_length(self) -> f32 {
        self.gap_length
    }
    pub const fn phase(self) -> f32 {
        self.phase
    }
    pub const fn gap_opacity(self) -> f32 {
        self.gap_opacity
    }
    pub fn period(self) -> f32 {
        self.dash_length + self.gap_length
    }
    pub fn is_dotted(self) -> bool {
        self.dash_length == 0.0
    }

    pub fn validate(self) -> Result<(), StrokePatternError> {
        if !self.dash_length.is_finite() || self.dash_length < 0.0 {
            return Err(StrokePatternError::InvalidDashLength);
        }
        if !self.gap_length.is_finite() || self.gap_length <= 0.0 {
            return Err(StrokePatternError::InvalidGapLength);
        }
        if !self.period().is_finite() || self.period() <= 0.0 {
            return Err(StrokePatternError::InvalidPeriod);
        }
        if !self.phase.is_finite() {
            return Err(StrokePatternError::InvalidPhase);
        }
        if !self.gap_opacity.is_finite() || !(0.0..=1.0).contains(&self.gap_opacity) {
            return Err(StrokePatternError::InvalidGapOpacity);
        }
        Ok(())
    }

    /// Interpolate unwrapped phase and gap opacity for a fixed pattern shape.
    /// Changes of pattern shape or presence still switch at the end.
    pub fn interpolate(from: Option<Self>, to: Option<Self>, progress: f32) -> Option<Self> {
        if progress <= 0.0 {
            return from;
        }
        if progress >= 1.0 {
            return to;
        }
        match (from, to) {
            (Some(a), Some(b))
                if a.dash_length == b.dash_length && a.gap_length == b.gap_length =>
            {
                let p = f64::from(progress);
                let phase = ((1.0 - p) * f64::from(a.phase) + p * f64::from(b.phase)) as f32;
                let gap_opacity =
                    ((1.0 - p) * f64::from(a.gap_opacity) + p * f64::from(b.gap_opacity)) as f32;
                Some(Self {
                    phase,
                    gap_opacity,
                    ..a
                })
            }
            _ => from,
        }
    }

    pub fn packed(self) -> [f32; 4] {
        [
            self.period(),
            self.dash_length,
            self.phase.rem_euclid(self.period()),
            self.gap_opacity,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_pattern_keeps_unwrapped_phase_until_packing() {
        let a = StrokePattern::dashed(0.25, 0.25).unwrap();
        let b = a.with_phase(2.0).unwrap();
        let mid = StrokePattern::interpolate(Some(a), Some(b), 0.375).unwrap();
        assert_eq!(mid.phase(), 0.75);
        assert_eq!(mid.packed(), [0.5, 0.25, 0.25, 0.2]);
        assert_eq!(b.packed()[2], 0.0);
        assert_eq!(a.with_phase(-0.125).unwrap().packed()[2], 0.375);
    }

    #[test]
    fn stroke_pattern_out_of_order_samples_match_independent_phase_values() {
        let a = StrokePattern::dotted(0.5).unwrap();
        let b = a.with_phase(-8.0).unwrap();
        for p in [0.9, 0.125, 1.0, 0.0, 0.375, 0.125] {
            let value = StrokePattern::interpolate(Some(a), Some(b), p).unwrap();
            assert_eq!(value.phase(), -8.0 * p);
            assert!((0.0..0.5).contains(&value.packed()[2]));
        }
    }

    #[test]
    fn gap_opacity_changes_do_not_freeze_phase_during_repeated_samples() {
        for pattern in [
            StrokePattern::dashed(0.25, 0.25).unwrap(),
            StrokePattern::dotted(0.5).unwrap(),
        ] {
            for (start_opacity, end_opacity) in [(0.25, 0.75), (0.75, 0.25)] {
                let from = pattern
                    .with_phase(0.125)
                    .unwrap()
                    .with_gap_opacity(start_opacity)
                    .unwrap();
                let to = pattern
                    .with_phase(-1.875)
                    .unwrap()
                    .with_gap_opacity(end_opacity)
                    .unwrap();
                for p in [0.0, 0.125, 0.5, 1.0, 0.375, 0.125, 0.0] {
                    let value = StrokePattern::interpolate(Some(from), Some(to), p).unwrap();
                    let phase = 0.125 - 2.0 * p;
                    let opacity = start_opacity + (end_opacity - start_opacity) * p;
                    assert_eq!(value.phase(), phase);
                    assert_eq!(value.gap_opacity(), opacity);
                    assert_eq!(value.packed()[2], phase.rem_euclid(0.5));
                    assert_eq!(value.packed()[3], opacity);
                    value.validate().unwrap();
                }
            }
        }
    }

    #[test]
    fn gap_opacity_interpolation_preserves_shape_and_presence_switches() {
        let from = StrokePattern::dashed(0.25, 0.25).unwrap();
        for to in [
            None,
            Some(StrokePattern::dotted(0.5).unwrap()),
            Some(StrokePattern::dashed(0.5, 0.25).unwrap()),
            Some(StrokePattern::dashed(0.25, 0.5).unwrap()),
        ] {
            for p in [0.0, 0.125, 0.5, 0.875] {
                assert_eq!(StrokePattern::interpolate(Some(from), to, p), Some(from));
            }
            assert_eq!(StrokePattern::interpolate(Some(from), to, 1.0), to);
        }
        assert_eq!(StrokePattern::interpolate(None, Some(from), 0.5), None);
        assert_eq!(
            StrokePattern::interpolate(None, Some(from), 1.0),
            Some(from)
        );
    }

    #[test]
    fn gap_opacity_changes_preserve_finite_opposite_extreme_phase_interpolation() {
        let pattern = StrokePattern::dotted(0.5).unwrap();
        let from = pattern
            .with_phase(f32::MAX)
            .unwrap()
            .with_gap_opacity(0.0)
            .unwrap();
        let to = pattern
            .with_phase(-f32::MAX)
            .unwrap()
            .with_gap_opacity(1.0)
            .unwrap();
        let value = StrokePattern::interpolate(Some(from), Some(to), 0.5).unwrap();
        assert_eq!(value.phase(), 0.0);
        assert_eq!(value.gap_opacity(), 0.5);
        value.validate().unwrap();
    }

    #[test]
    fn stroke_pattern_rejects_invalid_parameters() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 0.0] {
            assert!(StrokePattern::dashed(value, 0.2).is_err());
            assert!(StrokePattern::dashed(0.2, value).is_err());
            assert!(StrokePattern::dotted(value).is_err());
        }
        assert!(StrokePattern::dashed(f32::MAX, f32::MAX).is_err());
        let a = StrokePattern::dotted(0.5).unwrap();
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(a.with_phase(value).is_err());
        }
        for value in [f32::NAN, -0.01, 1.01] {
            assert!(a.with_gap_opacity(value).is_err());
        }
    }
}
