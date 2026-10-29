//! Inert square-cell sampling over a painted object or composed source.
use super::effect_parameters::sealed;
use crate::{
    EffectParameter, EffectParameterError, EffectRadius, EffectUpdate, EffectValue, Pixels,
};

/// Square screen-axis cells anchored at the original source's top-left bounds.
/// Each cell takes the nearest input texel at its centre, with transparent edges.
/// Size uses scene units or final output pixels and is independent of object scale.
/// A resolved size of at most one pixel bypasses sampling exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pixelate {
    size: EffectRadius,
}
impl Default for Pixelate {
    fn default() -> Self {
        Self {
            size: Pixels(12.0).into(),
        }
    }
}
impl Pixelate {
    pub fn new(update: EffectUpdate) -> Result<Self, EffectParameterError> {
        update.apply_to(Self::default())
    }
    pub const fn size(self) -> EffectRadius {
        self.size
    }
    pub fn is_neutral(self) -> bool {
        matches!(self.size, EffectRadius::Pixels(v) if v <= 1.0) || self.size.value() == 0.0
    }
}
impl sealed::Sealed for Pixelate {}
impl EffectValue for Pixelate {
    fn apply_effect_update(self, update: EffectUpdate) -> Result<Self, EffectParameterError> {
        update.require_parameters(&[EffectParameter::Size])?;
        update.validate()?;
        Ok(Self {
            size: update.size.unwrap_or(self.size),
        })
    }
    fn validate_effect_value_update(&self, _target: &Self) -> Result<(), EffectParameterError> {
        Ok(())
    }
    fn validate_effect_transition(&self, target: &Self) -> Result<(), EffectParameterError> {
        if !self.size.same_unit(target.size) {
            return Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Size,
            ));
        }
        Ok(())
    }
    fn sample_effect_parameters(
        &self,
        target: &Self,
        request: EffectUpdate,
        alpha: f64,
    ) -> Result<EffectUpdate, EffectParameterError> {
        let update = EffectUpdate {
            size: request
                .size
                .map(|_| self.size.interpolate(target.size, alpha)),
            ..EffectUpdate::default()
        };
        update.apply_to(*self)?;
        Ok(update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_size_has_exact_identity_partial_updates_and_unit_safe_interpolation() {
        let a = Pixelate::new(EffectUpdate::default().size(Pixels(13.0))).unwrap();
        let request = EffectUpdate::default().size(Pixels(1.0));
        let p = request.prepare(a).unwrap();
        assert_eq!(
            p.sample(0.5).unwrap().apply_to(a).unwrap().size(),
            EffectRadius::Pixels(7.0)
        );
        assert!(p.sample(1.0).unwrap().apply_to(a).unwrap().is_neutral());
        assert_eq!(EffectUpdate::default().apply_to(a).unwrap(), a);
        assert!(EffectUpdate::default().size(0.2).prepare(a).is_err());
        for v in [f64::NAN, f64::INFINITY, -1.0] {
            assert_eq!(
                Pixelate::new(EffectUpdate::default().size(Pixels(v))),
                Err(EffectParameterError::InvalidSize)
            );
        }
        assert!(Pixelate::new(EffectUpdate::default().radius(1.0)).is_err());
        assert!(EffectUpdate::default()
            .size(Pixels(2.0))
            .apply_to(crate::Blur::default())
            .is_err());
        assert_eq!(
            request.parameters().collect::<Vec<_>>(),
            vec![EffectParameter::Size]
        );
    }
}
