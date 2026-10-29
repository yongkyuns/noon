//! Inert checked Bloom declaration for the existing authored 2D camera view.
use super::effect_parameters::{interpolate, sealed};
use crate::{
    EffectParameter, EffectParameterError, EffectRadius, EffectUpdate, EffectValue, Pixels,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bloom {
    radius: EffectRadius,
    threshold: f64,
    intensity: f64,
}
impl Default for Bloom {
    fn default() -> Self {
        Self {
            radius: Pixels(8.0).into(),
            threshold: 1.0,
            intensity: 1.0,
        }
    }
}
impl Bloom {
    pub fn new(update: EffectUpdate) -> Result<Self, EffectParameterError> {
        update.apply_to(Self::default())
    }
    pub const fn radius(self) -> EffectRadius {
        self.radius
    }
    pub const fn threshold(self) -> f64 {
        self.threshold
    }
    pub const fn intensity(self) -> f64 {
        self.intensity
    }
    pub fn is_neutral(self) -> bool {
        self.intensity == 0.0
    }
}
impl sealed::Sealed for Bloom {}
impl EffectValue for Bloom {
    fn apply_effect_update(self, update: EffectUpdate) -> Result<Self, EffectParameterError> {
        update.require_parameters(&[
            EffectParameter::Radius,
            EffectParameter::Threshold,
            EffectParameter::Intensity,
        ])?;
        update.validate()?;
        Ok(Self {
            radius: update.radius.unwrap_or(self.radius),
            threshold: update.threshold.unwrap_or(self.threshold),
            intensity: update.intensity.unwrap_or(self.intensity),
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
        request: EffectUpdate,
        alpha: f64,
    ) -> Result<EffectUpdate, EffectParameterError> {
        let update = EffectUpdate {
            radius: request
                .radius
                .map(|_| self.radius.interpolate(target.radius, alpha)),
            threshold: request
                .threshold
                .map(|_| interpolate(self.threshold, target.threshold, alpha)),
            intensity: request
                .intensity
                .map(|_| interpolate(self.intensity, target.intensity, alpha)),
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
    fn pending_camera_bloom_admission_uses_final_staged_profile_before_allocation() {
        let mut store = crate::SemanticStore::new();
        let mut state = crate::SemanticObjectState::new(crate::StoredGeometry::Rectangle {
            size: crate::Vec2::new(8.0, 8.0),
        });
        state.set_role(crate::SemanticObjectRole::Camera2D);
        let mut tx = crate::SemanticMutationTransaction::new();
        let camera = tx.create_node(crate::SemanticNodeCreation::object(state));
        tx.create_effect(camera, "bloom", Bloom::default());
        tx.set_view_color_profile(camera, crate::ViewColorProfile::LinearHdr);
        tx.apply(&mut store).unwrap();
    }

    #[test]
    fn checked_bloom_threshold_and_sparse_sampling_keep_f64_precision() {
        for threshold in [-1.0, 64.0001, f64::NAN, f64::INFINITY] {
            assert_eq!(
                Bloom::new(EffectUpdate::default().threshold(threshold)),
                Err(EffectParameterError::InvalidThreshold)
            );
        }
        let a = Bloom::new(
            EffectUpdate::default()
                .radius(Pixels(1.0))
                .threshold(1.000000000001)
                .intensity(0.5),
        )
        .unwrap();
        let request = EffectUpdate::default().threshold(2.000000000001);
        let prepared = request.prepare(a).unwrap();
        for alpha in [0.0, 0.5, 1.0, 0.25, 0.5] {
            let sample = prepared.sample(alpha).unwrap();
            assert_eq!(
                sample.parameters().collect::<Vec<_>>(),
                [EffectParameter::Threshold]
            );
            assert_eq!(
                sample.threshold,
                Some(interpolate(a.threshold(), 2.000000000001, alpha))
            );
            let b = sample.apply_to(a).unwrap();
            assert_eq!(b.radius(), a.radius());
            assert_eq!(b.intensity(), a.intensity());
        }
        assert!(EffectUpdate::default()
            .threshold(0.0)
            .apply_to(crate::Glow::default())
            .is_err());
        assert!(EffectUpdate::default().radius(1.0).prepare(a).is_err());
        assert_eq!(
            EffectUpdate::default()
                .threshold(0.0)
                .apply_to(a)
                .unwrap()
                .threshold(),
            0.0
        );
    }
}
