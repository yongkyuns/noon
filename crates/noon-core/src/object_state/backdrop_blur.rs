//! Checked Gaussian treatment of an explicit painter-prefix input.
//! Radius validation and interpolation use the same shared values as Blur.
use super::effect_parameters::sealed;
use crate::{Blur, EffectParameterError, EffectRadius, EffectUpdate, EffectValue};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackdropBlur(Blur);
impl BackdropBlur {
    pub fn new(update: EffectUpdate) -> Result<Self, EffectParameterError> {
        Blur::new(update).map(Self)
    }
    pub const fn radius(self) -> EffectRadius {
        self.0.radius()
    }
    pub fn is_neutral(self) -> bool {
        self.0.is_neutral()
    }
}
impl sealed::Sealed for BackdropBlur {}
impl EffectValue for BackdropBlur {
    fn apply_effect_update(self, update: EffectUpdate) -> Result<Self, EffectParameterError> {
        self.0.apply_effect_update(update).map(Self)
    }
    fn validate_effect_transition(self, target: Self) -> Result<(), EffectParameterError> {
        self.0.validate_effect_transition(target.0)
    }
    fn sample_effect_parameters(
        self,
        target: Self,
        request: EffectUpdate,
        alpha: f64,
    ) -> Result<EffectUpdate, EffectParameterError> {
        self.0.sample_effect_parameters(target.0, request, alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, EffectDefinition, EffectInputRole, EffectParameter, Pixels};
    #[test]
    fn backdrop_role_reuses_radius_without_conflating_object_blur() {
        let radius = 1.000000000000123;
        let blur = BackdropBlur::new(EffectUpdate::default().radius(Pixels(radius))).unwrap();
        let definition = EffectDefinition::from(blur);
        assert_eq!(definition.radius(), Some(EffectRadius::Pixels(radius)));
        assert_eq!(definition.as_backdrop_blur(), Some(blur));
        assert!(definition.as_blur().is_none());
        assert_eq!(definition.input_role(), EffectInputRole::PainterPrefix);
        assert_eq!(
            EffectDefinition::from(Blur::default()).input_role(),
            EffectInputRole::Painted
        );
        assert_eq!(
            EffectDefinition::from(Blur::default()).validate_effect_transition(definition),
            Err(EffectParameterError::IncompatibleDefinition)
        );
        assert_eq!(
            EffectUpdate::default().color(Color::RED).apply_to(blur),
            Err(EffectParameterError::UnsupportedParameter(
                EffectParameter::Color
            ))
        );
        for value in [f64::NAN, f64::INFINITY, -0.01] {
            assert_eq!(
                BackdropBlur::new(EffectUpdate::default().radius(value)),
                Err(EffectParameterError::InvalidRadius)
            );
        }
        assert!(BackdropBlur::new(EffectUpdate::default().radius(0.0))
            .unwrap()
            .is_neutral());
    }
    #[test]
    fn ordinary_captured_updates_keep_precision_stateless_seek_and_units() {
        let from =
            BackdropBlur::new(EffectUpdate::default().radius(Pixels(1.000000000000123))).unwrap();
        let update = EffectUpdate::default().radius(Pixels(5.000000000000123));
        let prepared = update.prepare(EffectDefinition::from(from)).unwrap();
        for alpha in [1.0, 0.5, 0.0, 0.75, 0.5] {
            let sampled = prepared.sample(alpha).unwrap();
            let actual = sampled.apply_to(EffectDefinition::from(from)).unwrap();
            assert_eq!(actual.input_role(), EffectInputRole::PainterPrefix);
            let expected = 1.000000000000123 + 4.0 * alpha;
            assert!(
                (actual.radius().unwrap().value() - expected).abs()
                    <= 2.0 * f64::EPSILON * expected.abs()
            );
        }
        assert_eq!(
            EffectUpdate::default().radius(0.5).prepare(from),
            Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Radius
            ))
        );
        assert_eq!(
            from.radius().to_output_pixels(200, 10.0),
            Ok(1.000000000000123)
        );
        assert_eq!(
            BackdropBlur::new(EffectUpdate::default().radius(0.5))
                .unwrap()
                .radius()
                .to_output_pixels(200, 10.0),
            Ok(10.0)
        );
    }
}
