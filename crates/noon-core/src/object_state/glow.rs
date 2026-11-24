//! Checked glow value over the shared finite effect parameter vocabulary.
use super::effect_parameters::{interpolate, sealed};
use crate::{
    Color, EffectParameter, EffectParameterError, EffectRadius, EffectUpdate, EffectValue,
};

/// What supplies the halo mask; source mode is not an animatable scalar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlowSource {
    #[default]
    Painted,
    Silhouette,
}

/// Complete validated glow definition. Copying it allocates no resources.
///
/// This is not an effect handle or a mutable copy of an attached object's state.
/// Fields are private so invalid intermediate values cannot become a definition.
///
/// ```
/// use noon_core::{Glow, EffectParameterError, EffectUpdate, Pixels};
/// # fn main() -> Result<(), EffectParameterError> {
/// let halo = Glow::new(EffectUpdate::default().radius(Pixels(12.0)).intensity(0.25))?;
/// let brighter = EffectUpdate::default().intensity(1.4).apply_to(halo)?;
/// assert_eq!(brighter.radius(), halo.radius());
/// assert_eq!(halo.intensity(), 0.25);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    color: Color,
    radius: EffectRadius,
    intensity: f64,
    source: GlowSource,
}

impl Default for Glow {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            radius: EffectRadius::Scene(0.15),
            intensity: 0.35,
            source: GlowSource::Painted,
        }
    }
}

impl Glow {
    /// Construct from documented defaults and one checked partial update.
    pub fn new(update: EffectUpdate) -> Result<Self, EffectParameterError> {
        update.apply_to(Self::default())
    }

    pub const fn color(self) -> Color {
        self.color
    }

    pub const fn radius(self) -> EffectRadius {
        self.radius
    }

    pub const fn intensity(self) -> f64 {
        self.intensity
    }

    pub const fn source(self) -> GlowSource {
        self.source
    }

    /// Neutral for the specified artistic LDR halo; not an HDR-emission rule.
    pub fn is_neutral(self) -> bool {
        self.intensity == 0.0 || self.radius.value() == 0.0 || self.color.alpha == 0.0
    }
}

impl sealed::Sealed for Glow {}
impl EffectValue for Glow {
    fn apply_effect_update(self, update: &EffectUpdate) -> Result<Self, EffectParameterError> {
        update.require_parameters(&[
            EffectParameter::Color,
            EffectParameter::Radius,
            EffectParameter::Intensity,
            EffectParameter::Source,
        ])?;
        update.validate()?;
        Ok(Glow {
            color: update.color.unwrap_or(self.color),
            radius: update.radius.unwrap_or(self.radius),
            intensity: update.intensity.unwrap_or(self.intensity),
            source: update.source.unwrap_or(self.source),
        })
    }
    fn validate_effect_transition(&self, target: &Self) -> Result<(), EffectParameterError> {
        if !self.radius.same_unit(target.radius) {
            return Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Radius,
            ));
        }
        if self.source != target.source {
            return Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Source,
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
        let color = |from: f32, to: f32| interpolate(f64::from(from), f64::from(to), alpha) as f32;
        let update = EffectUpdate {
            color: request.color.map(|_| {
                Color::rgba(
                    color(self.color.red, target.color.red),
                    color(self.color.green, target.color.green),
                    color(self.color.blue, target.color.blue),
                    color(self.color.alpha, target.color.alpha),
                )
            }),
            radius: request
                .radius
                .map(|_| self.radius.interpolate(target.radius, alpha)),
            intensity: request
                .intensity
                .map(|_| interpolate(self.intensity, target.intensity, alpha)),
            source: request.source,
            offset: None,
            ..EffectUpdate::default()
        };
        // Keep even extreme finite radius arithmetic fail-closed.
        update.apply_to(*self)?;
        Ok(update)
    }
}

#[cfg(test)]
mod tests;
