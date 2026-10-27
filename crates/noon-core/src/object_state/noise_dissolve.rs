//! Inert deterministic mask parameters. The shared runtime supplies progress;
//! this value has no clock, attachment identity, resource or scene membership.
use super::effect_parameters::{interpolate, sealed};
use crate::{EffectParameter, EffectParameterError, EffectUpdate, EffectValue};

/// Seeded value-noise dissolve over normalized source bounds. Zero progress is
/// exactly the original premultiplied input; one is exactly transparent. The
/// version-one field has sixteen lattice cells per normalized source axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseDissolve {
    seed: u32,
    softness: f64,
    progress: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseDissolveError {
    InvalidSoftness,
    InvalidProgress,
}
impl std::fmt::Display for NoiseDissolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSoftness => "dissolve softness must be finite and in [0, 1]",
            Self::InvalidProgress => "dissolve progress must be finite and in [0, 1]",
        })
    }
}
impl std::error::Error for NoiseDissolveError {}
impl Default for NoiseDissolve {
    fn default() -> Self {
        Self {
            seed: 0,
            softness: 0.08,
            progress: 0.0,
        }
    }
}
impl NoiseDissolve {
    pub fn new(seed: u32, softness: f64) -> Result<Self, NoiseDissolveError> {
        if !softness.is_finite() || !(0.0..=1.0).contains(&softness) {
            return Err(NoiseDissolveError::InvalidSoftness);
        }
        Ok(Self {
            seed,
            softness,
            progress: 0.0,
        })
    }
    pub fn with_progress(mut self, progress: f64) -> Result<Self, NoiseDissolveError> {
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return Err(NoiseDissolveError::InvalidProgress);
        }
        self.progress = progress;
        Ok(self)
    }
    pub const fn seed(self) -> u32 {
        self.seed
    }
    pub const fn softness(self) -> f64 {
        self.softness
    }
    pub const fn progress(self) -> f64 {
        self.progress
    }
    pub fn is_neutral(self) -> bool {
        self.progress == 0.0
    }
}

impl sealed::Sealed for NoiseDissolve {}
impl EffectValue for NoiseDissolve {
    fn apply_effect_update(self, update: EffectUpdate) -> Result<Self, EffectParameterError> {
        update.require_parameters(&[
            EffectParameter::Progress,
            EffectParameter::Softness,
            EffectParameter::Seed,
        ])?;
        update.validate()?;
        Ok(Self {
            seed: update.seed.unwrap_or(self.seed),
            softness: update.softness.unwrap_or(self.softness),
            progress: update.progress.unwrap_or(self.progress),
        })
    }
    fn validate_effect_value_update(self, _target: Self) -> Result<(), EffectParameterError> {
        Ok(())
    }
    fn validate_effect_transition(self, target: Self) -> Result<(), EffectParameterError> {
        if self.seed != target.seed {
            return Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Seed,
            ));
        }
        Ok(())
    }
    fn sample_effect_parameters(
        self,
        target: Self,
        request: EffectUpdate,
        alpha: f64,
    ) -> Result<EffectUpdate, EffectParameterError> {
        let update = EffectUpdate {
            progress: request
                .progress
                .map(|_| interpolate(self.progress, target.progress, alpha)),
            softness: request
                .softness
                .map(|_| interpolate(self.softness, target.softness, alpha)),
            seed: request.seed,
            ..EffectUpdate::default()
        };
        update.apply_to(self)?;
        Ok(update)
    }
}

/// Direction of a high-level visibility transition, independent of rate reversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DissolveDirection {
    In,
    Out,
}

/// Checked partial style captured by an inert dissolve declaration. Progress is
/// owned by the animation, while omitted style fields preserve an existing mask.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DissolveStyle {
    seed: Option<u32>,
    softness: Option<f64>,
}
impl DissolveStyle {
    pub fn new(update: EffectUpdate) -> Result<Self, EffectParameterError> {
        update.require_parameters(&[EffectParameter::Seed, EffectParameter::Softness])?;
        update.validate()?;
        Ok(Self {
            seed: update.seed,
            softness: update.softness,
        })
    }
    pub fn parameters(self) -> EffectUpdate {
        EffectUpdate {
            seed: self.seed,
            softness: self.softness,
            ..EffectUpdate::default()
        }
    }
    pub fn resolve(self, previous: NoiseDissolve) -> NoiseDissolve {
        self.parameters()
            .apply_to(previous)
            .expect("checked dissolve style")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dissolve_style_keeps_omitted_parameters_and_cannot_own_progress() {
        let previous = NoiseDissolve::new(7, 0.2)
            .unwrap()
            .with_progress(0.5)
            .unwrap();
        assert_eq!(DissolveStyle::default().resolve(previous), previous);
        assert_eq!(
            DissolveStyle::new(EffectUpdate::default().seed(0))
                .unwrap()
                .resolve(previous),
            NoiseDissolve::new(0, 0.2)
                .unwrap()
                .with_progress(0.5)
                .unwrap()
        );
        for update in [
            EffectUpdate::default().progress(0.0),
            EffectUpdate::default().intensity(1.0),
            EffectUpdate::default().softness(f64::NAN),
            EffectUpdate::default().softness(-0.1),
            EffectUpdate::default().softness(1.1),
        ] {
            assert!(DissolveStyle::new(update).is_err());
        }
    }

    #[test]
    fn mask_updates_are_partial_typed_and_seed_transitions_are_discrete() {
        let value = NoiseDissolve::new(7, 0.08)
            .unwrap()
            .with_progress(0.25)
            .unwrap();
        let zero = EffectUpdate::default()
            .progress(0.0)
            .apply_to(value)
            .unwrap();
        assert_eq!(
            (zero.seed(), zero.softness(), zero.progress()),
            (7, 0.08, 0.0)
        );
        let request = EffectUpdate::default().progress(0.75).softness(0.4);
        let prepared = request.prepare(value).unwrap();
        assert_eq!(
            prepared.sample(0.0).unwrap().apply_to(value).unwrap(),
            value
        );
        let middle = prepared.sample(0.5).unwrap().apply_to(value).unwrap();
        assert_eq!(
            (middle.progress(), middle.softness(), middle.seed()),
            (0.5, 0.08 * 0.5 + 0.4 * 0.5, 7)
        );
        assert_eq!(
            EffectUpdate::default()
                .seed(0)
                .apply_to(value)
                .unwrap()
                .seed(),
            0
        );
        assert_eq!(
            EffectUpdate::default().seed(0).prepare(value),
            Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Seed
            ))
        );
        assert_eq!(
            EffectUpdate::default()
                .progress(0.0)
                .softness(0.0)
                .seed(0)
                .parameters()
                .collect::<Vec<_>>(),
            vec![
                EffectParameter::Progress,
                EffectParameter::Softness,
                EffectParameter::Seed
            ]
        );
        let definition: crate::EffectDefinition = value.into();
        assert_eq!(definition.radius(), None);
        assert_eq!(definition.color(), None);
        assert_eq!(
            EffectUpdate::default()
                .progress(0.0)
                .apply_to(crate::Glow::default()),
            Err(EffectParameterError::UnsupportedParameter(
                EffectParameter::Progress
            ))
        );
        assert_eq!(
            EffectUpdate::default()
                .softness(0.0)
                .apply_to(crate::Blur::default()),
            Err(EffectParameterError::UnsupportedParameter(
                EffectParameter::Softness
            ))
        );
        assert_eq!(
            EffectUpdate::default()
                .seed(0)
                .apply_to(crate::Shadow::default()),
            Err(EffectParameterError::UnsupportedParameter(
                EffectParameter::Seed
            ))
        );
    }
    #[test]
    fn checked_parameters_preserve_precision_and_reject_nonfinite_values() {
        let softness = 0.0800000000000123;
        let progress = 0.4000000000000123;
        let value = NoiseDissolve::new(u32::MAX, softness)
            .unwrap()
            .with_progress(progress)
            .unwrap();
        assert_eq!(
            (value.seed(), value.softness(), value.progress()),
            (u32::MAX, softness, progress)
        );
        for invalid in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -f64::EPSILON,
            1.0 + f64::EPSILON,
        ] {
            assert_eq!(
                NoiseDissolve::new(0, invalid),
                Err(NoiseDissolveError::InvalidSoftness)
            );
            assert_eq!(
                value.with_progress(invalid),
                Err(NoiseDissolveError::InvalidProgress)
            );
        }
        for softness in [0.0, 1.0] {
            let full = NoiseDissolve::new(7, softness).unwrap();
            assert!(full.is_neutral());
            assert!(!full.with_progress(1.0).unwrap().is_neutral());
        }
    }
}
