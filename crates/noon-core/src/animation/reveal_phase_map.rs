//! Immutable authored phase knots for the existing retained-path reveal channel.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One phase interval per drawable curve, with both endpoint knots included.
pub const MAX_REVEAL_PHASE_KEYS: usize = 65_537;

/// Maps authored phase to retained curve-index progress without owning time.
///
/// Knots include exactly zero and one and increase strictly. Each interval maps
/// linearly to one drawable curve, independent of its length. Lowering verifies
/// that the target path has exactly [`Self::curve_count`] drawable curves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "PhaseMapInput")]
pub struct RevealPhaseMap {
    phases: Box<[f64]>,
}

#[derive(Deserialize)]
struct PhaseMapInput {
    phases: Vec<f64>,
}

impl TryFrom<PhaseMapInput> for RevealPhaseMap {
    type Error = RevealPhaseMapError;

    fn try_from(value: PhaseMapInput) -> Result<Self, Self::Error> {
        Self::new(value.phases)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevealPhaseMapError {
    InvalidCardinality,
    InvalidPhases,
}

impl fmt::Display for RevealPhaseMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCardinality => "reveal phase map requires 2 to 65537 phase knots",
            Self::InvalidPhases => {
                "reveal phase knots must be finite, strictly increasing, and include exactly 0 and 1"
            }
        })
    }
}

impl std::error::Error for RevealPhaseMapError {}

impl RevealPhaseMap {
    pub fn new(phases: Vec<f64>) -> Result<Self, RevealPhaseMapError> {
        if !(2..=MAX_REVEAL_PHASE_KEYS).contains(&phases.len()) {
            return Err(RevealPhaseMapError::InvalidCardinality);
        }
        if phases[0] != 0.0
            || phases[phases.len() - 1] != 1.0
            || !phases.iter().all(|phase| phase.is_finite())
            || phases.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(RevealPhaseMapError::InvalidPhases);
        }
        Ok(Self {
            phases: phases.into_boxed_slice(),
        })
    }

    pub fn phases(&self) -> &[f64] {
        &self.phases
    }

    pub fn len(&self) -> usize {
        self.phases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.phases.is_empty()
    }

    pub fn curve_count(&self) -> usize {
        self.len() - 1
    }

    /// Logical immutable allocation bytes, excluding allocator overhead.
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + std::mem::size_of_val(&*self.phases)
    }

    /// Sample in O(log N), retaining f64 precision until the execution channel.
    /// Finite out-of-range phases clamp to the endpoints; NaN remains NaN.
    pub fn sample(&self, phase: f64) -> f64 {
        if phase <= 0.0 {
            return 0.0;
        }
        if phase >= 1.0 {
            return 1.0;
        }
        let next = self.phases.partition_point(|&knot| knot < phase).max(1);
        let previous = next - 1;
        let fraction =
            (phase - self.phases[previous]) / (self.phases[next] - self.phases[previous]);
        (previous as f64 + fraction) / self.curve_count() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_reveal_track_rejects_wrong_channel_and_unbounded_rate() {
        use crate::{
            validate_track_definition, CompositionTimeMap, ObjectId, Property, RateFunction,
            TimelineError, TrackDefinition, TrackId, TrackTiming, TrackValues,
        };
        let mut track = TrackDefinition {
            id: TrackId::new(1),
            object: ObjectId::new(1),
            property: Property::Reveal,
            values: TrackValues::PhaseReveal {
                phase_map: std::sync::Arc::new(RevealPhaseMap::new(vec![0.0, 1.0]).unwrap()),
            },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        validate_track_definition(&track).unwrap();
        track.property = Property::Opacity;
        assert_eq!(
            validate_track_definition(&track),
            Err(TimelineError::InvalidPhaseRevealValues(Property::Opacity))
        );
        track.property = Property::Reveal;
        track.timing.easing = RateFunction::EaseOutBack;
        assert_eq!(
            validate_track_definition(&track),
            Err(TimelineError::UnsupportedPhaseRevealRateFunction(
                RateFunction::EaseOutBack
            ))
        );
    }

    #[test]
    fn phase_reveal_options_keep_precedence_and_reject_path_and_lag() {
        use crate::{
            resolve_phase_reveal_animation_options as resolve, AnimationDefaults, AnimationOptions,
            RateFunction,
        };
        let local = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::EaseInQuad)
            .reverse_rate_function(true);
        let resolved = resolve(
            AnimationDefaults {
                introducer: true,
                ..AnimationDefaults::MANIM
            },
            local,
            AnimationOptions::new().run_time(3.0),
        )
        .unwrap();
        assert_eq!(resolved.run_time, 3.0);
        assert_eq!(resolved.rate_func, RateFunction::EaseInQuad);
        assert!(resolved.reverse_rate_function && resolved.introducer);
        for invalid in [
            AnimationOptions::new().path_arc(1e-14),
            AnimationOptions::new().lag_ratio(1e-14),
            AnimationOptions::new().run_time(0.0),
        ] {
            assert!(resolve(AnimationDefaults::MANIM, local, invalid).is_err());
        }
    }

    #[test]
    fn reveal_phase_map_rejects_invalid_knots() {
        for phases in [vec![], vec![0.0], vec![1.0]] {
            assert_eq!(
                RevealPhaseMap::new(phases),
                Err(RevealPhaseMapError::InvalidCardinality)
            );
        }
        for phases in [
            vec![0.1, 1.0],
            vec![0.0, 0.9],
            vec![0.0, 0.0, 1.0],
            vec![0.0, 0.8, 0.2, 1.0],
            vec![0.0, f64::NAN, 1.0],
            vec![0.0, f64::INFINITY, 1.0],
            vec![0.0, -0.1, 1.0],
        ] {
            assert_eq!(
                RevealPhaseMap::new(phases),
                Err(RevealPhaseMapError::InvalidPhases)
            );
        }
    }

    #[test]
    fn reveal_phase_map_preserves_nonuniform_knots_and_sub_f32_intervals() {
        let middle = 0.5 + 2.0_f64.powi(-40);
        let map = RevealPhaseMap::new(vec![0.0, 0.1, 0.5, middle, 1.0]).unwrap();
        for (index, &phase) in map.phases().iter().enumerate() {
            assert_eq!(map.sample(phase), index as f64 / 4.0);
        }
        assert_eq!(map.sample((0.5 + middle) * 0.5), 0.625);
        assert_eq!(map.sample(0.05), 0.125);
        assert_eq!(map.sample(-1.0), 0.0);
        assert_eq!(map.sample(2.0), 1.0);
        assert!(map.sample(f64::NAN).is_nan());
        assert_eq!(map.curve_count(), 4);
        assert_eq!(
            map.retained_bytes(),
            std::mem::size_of::<RevealPhaseMap>() + 5 * 8
        );
    }

    #[test]
    fn reveal_phase_map_enforces_bounded_allocation() {
        let phases: Vec<_> = (0..MAX_REVEAL_PHASE_KEYS)
            .map(|index| index as f64 / (MAX_REVEAL_PHASE_KEYS - 1) as f64)
            .collect();
        let map = RevealPhaseMap::new(phases).unwrap();
        assert_eq!(map.curve_count(), 65_536);
        assert_eq!(map.sample(0.5), 0.5);
        assert_eq!(
            RevealPhaseMap::new(vec![0.0; MAX_REVEAL_PHASE_KEYS + 1]),
            Err(RevealPhaseMapError::InvalidCardinality)
        );
    }
}
