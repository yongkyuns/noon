//! Inert checked Gaussian blur value; attachments and time remain scene-owned.
use super::effect_parameters::sealed;
use crate::{EffectParameter, EffectParameterError, EffectRadius, EffectUpdate, EffectValue};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blur {
    radius: EffectRadius,
}

impl Default for Blur {
    fn default() -> Self {
        Self {
            radius: EffectRadius::Scene(0.15),
        }
    }
}

impl Blur {
    pub fn new(update: EffectUpdate) -> Result<Self, EffectParameterError> {
        update.apply_to(Self::default())
    }
    pub const fn radius(self) -> EffectRadius {
        self.radius
    }
    pub fn is_neutral(self) -> bool {
        self.radius.value() == 0.0
    }
}

impl sealed::Sealed for Blur {}
impl EffectValue for Blur {
    fn apply_effect_update(self, update: &EffectUpdate) -> Result<Self, EffectParameterError> {
        update.require_parameters(&[EffectParameter::Radius])?;
        if let Some(radius) = update.radius {
            radius.validate()?;
        }
        Ok(Self {
            radius: update.radius.unwrap_or(self.radius),
        })
    }
    fn validate_effect_transition(&self, target: &Self) -> Result<(), EffectParameterError> {
        if !self.radius.same_unit(target.radius) {
            return Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Radius,
            ));
        }
        Ok(())
    }
    fn sample_effect_parameters(
        &self,
        target: &Self,
        request: &EffectUpdate,
        alpha: f64,
    ) -> Result<EffectUpdate, EffectParameterError> {
        let update = EffectUpdate {
            radius: request
                .radius
                .map(|_| self.radius.interpolate(target.radius, alpha)),
            ..EffectUpdate::default()
        };
        update.apply_to(*self)?;
        Ok(update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EffectDefinition, Pixels};

    #[test]
    fn blur_uses_shared_units_and_only_accepts_radius() {
        let blur = Blur::new(EffectUpdate::default().radius(Pixels(3.25))).unwrap();
        assert_eq!(blur.radius().to_output_pixels(100, 10.0), Ok(3.25));
        for (update, parameter) in [
            (
                EffectUpdate::default().color(crate::Color::WHITE),
                EffectParameter::Color,
            ),
            (
                EffectUpdate::default().offset([0.0, 0.0]),
                EffectParameter::Offset,
            ),
            (
                EffectUpdate::default().intensity(0.0),
                EffectParameter::Intensity,
            ),
            (
                EffectUpdate::default().source(crate::GlowSource::Painted),
                EffectParameter::Source,
            ),
        ] {
            assert_eq!(
                update.apply_to(blur),
                Err(EffectParameterError::UnsupportedParameter(parameter))
            );
        }
        for value in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                Blur::new(EffectUpdate::default().radius(value)),
                Err(EffectParameterError::InvalidRadius)
            );
        }
    }

    #[test]
    fn captured_radius_samples_are_precise_stateless_and_kind_checked() {
        let initial = Blur::new(EffectUpdate::default().radius(Pixels(1.000000000000123))).unwrap();
        let update = EffectUpdate::default().radius(Pixels(5.000000000000123));
        let prepared = update.prepare(EffectDefinition::Blur(initial)).unwrap();
        for alpha in [1.0, 0.5, 0.0, 0.75, 0.5] {
            let sample = prepared.sample(alpha).unwrap();
            assert_eq!(
                sample.parameters().collect::<Vec<_>>(),
                [EffectParameter::Radius]
            );
            let expected = super::super::effect_parameters::interpolate(
                initial.radius().value(),
                5.000000000000123,
                alpha,
            );
            assert_eq!(sample.radius, Some(EffectRadius::Pixels(expected)));
        }
        assert!(EffectUpdate::default()
            .radius(1.0)
            .prepare(initial)
            .is_err());
        assert_eq!(
            EffectDefinition::Blur(initial)
                .validate_effect_transition(&crate::Glow::default().into()),
            Err(EffectParameterError::IncompatibleDefinition)
        );
        assert!(Blur::new(EffectUpdate::default().radius(0.0))
            .unwrap()
            .is_neutral());
        for alpha in [-0.1, 1.1, f64::NAN] {
            assert_eq!(
                prepared.sample(alpha),
                Err(EffectParameterError::InvalidProgress)
            );
        }
    }
}
