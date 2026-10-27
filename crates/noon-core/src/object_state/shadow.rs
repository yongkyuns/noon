//! Colored Gaussian of painted alpha behind the original source.
use super::effect_parameters::{interpolate, sealed};
use crate::{
    Color, EffectOffset, EffectParameter, EffectParameterError, EffectRadius, EffectUpdate,
    EffectValue,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    color: Color,
    radius: EffectRadius,
    offset: EffectOffset,
}
impl Default for Shadow {
    fn default() -> Self {
        Self {
            color: Color::rgba(0.0, 0.0, 0.0, 0.5),
            radius: EffectRadius::Scene(0.15),
            offset: EffectOffset::Scene([0.1, -0.1]),
        }
    }
}
impl Shadow {
    pub fn new(update: EffectUpdate) -> Result<Self, EffectParameterError> {
        update.apply_to(Self::default())
    }
    pub const fn color(self) -> Color {
        self.color
    }
    pub const fn radius(self) -> EffectRadius {
        self.radius
    }
    pub const fn offset(self) -> EffectOffset {
        self.offset
    }
    /// Zero sigma is a hard shadow, including at a nonzero offset.
    pub fn is_neutral(self) -> bool {
        self.color.alpha == 0.0
    }
}
impl sealed::Sealed for Shadow {}
impl EffectValue for Shadow {
    fn apply_effect_update(self, update: EffectUpdate) -> Result<Self, EffectParameterError> {
        for parameter in [EffectParameter::Intensity, EffectParameter::Source] {
            if update.writes(parameter) {
                return Err(EffectParameterError::UnsupportedParameter(parameter));
            }
        }
        update.validate()?;
        Ok(Self {
            color: update.color.unwrap_or(self.color),
            radius: update.radius.unwrap_or(self.radius),
            offset: update.offset.unwrap_or(self.offset),
        })
    }
    fn validate_effect_transition(self, target: Self) -> Result<(), EffectParameterError> {
        if !self.radius.same_unit(target.radius) {
            return Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Radius,
            ));
        }
        if !self.offset.same_unit(target.offset) {
            return Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Offset,
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
        let channel = |a: f32, b: f32| interpolate(f64::from(a), f64::from(b), alpha) as f32;
        let update = EffectUpdate {
            color: request.color.map(|_| {
                Color::rgba(
                    channel(self.color.red, target.color.red),
                    channel(self.color.green, target.color.green),
                    channel(self.color.blue, target.color.blue),
                    channel(self.color.alpha, target.color.alpha),
                )
            }),
            radius: request
                .radius
                .map(|_| self.radius.interpolate(target.radius, alpha)),
            offset: request
                .offset
                .map(|_| self.offset.interpolate(target.offset, alpha)),
            ..EffectUpdate::default()
        };
        update.apply_to(self)?;
        Ok(update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_shadow_units_opacity_and_kind_are_checked() {
        let shadow = Shadow::new(
            EffectUpdate::default()
                .radius(0.0)
                .offset(EffectOffset::Pixels([-3.25, 4.5])),
        )
        .unwrap();
        assert!(!shadow.is_neutral());
        assert_eq!(
            shadow.offset().to_output_pixels(100, 10.0),
            Ok([-3.25, 4.5])
        );
        assert_eq!(
            EffectOffset::Scene([-0.325, 0.45]).to_output_pixels(100, 10.0),
            Ok([-3.25, 4.5])
        );
        assert!(EffectUpdate::default()
            .color(Color::rgba(0.2, 0.3, 0.4, 0.0))
            .apply_to(shadow)
            .unwrap()
            .is_neutral());
        for offset in [
            [f64::NAN, 0.0],
            [0.0, f64::INFINITY],
            [f64::NEG_INFINITY, 0.0],
        ] {
            assert_eq!(
                Shadow::new(EffectUpdate::default().offset(offset)),
                Err(EffectParameterError::InvalidOffset)
            );
        }
        for update in [
            EffectUpdate::default().intensity(0.0),
            EffectUpdate::default().source(crate::GlowSource::Painted),
        ] {
            assert!(update.apply_to(shadow).is_err());
        }
        assert!(EffectUpdate::default()
            .offset([0.0, 0.0])
            .apply_to(crate::Glow::default())
            .is_err());
        assert!(EffectUpdate::default()
            .offset([0.0, 0.0])
            .apply_to(crate::Blur::default())
            .is_err());
    }
    #[test]
    fn offset_sampling_is_exact_stateless_and_rejects_unit_changes() {
        let source = Shadow::new(
            EffectUpdate::default().offset(EffectOffset::Pixels([1.000000000000123, -4.0])),
        )
        .unwrap();
        let target = EffectUpdate::default()
            .offset(EffectOffset::Pixels([5.000000000000123, 8.0]))
            .prepare(source)
            .unwrap();
        for t in [0.0, 1.0, 0.5, 0.25, 0.5] {
            assert_eq!(
                target.sample(t).unwrap().offset,
                Some(EffectOffset::Pixels([
                    interpolate(1.000000000000123, 5.000000000000123, t),
                    -4.0 + 12.0 * t
                ]))
            );
        }
        assert!(EffectUpdate::default()
            .offset([1.0, 2.0])
            .prepare(source)
            .is_err());
        for t in [-0.01, 1.01, f64::NAN] {
            assert!(target.sample(t).is_err());
        }
    }
}
