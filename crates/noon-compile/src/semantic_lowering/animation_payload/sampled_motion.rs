//! One sampled semantic leaf lowers to existing affine execution channels.

use std::sync::Arc;

use noon_core::{
    PreparedSemanticMutationTransaction, Property, ResolvedAnimationOptions, SampledMotionData,
    SampledMotionError, SampledMotionPlan, SemanticAnimationIntent, SemanticObjectProperty,
    SemanticObjectState, SemanticSignalValue, SemanticStore, SemanticTransactionAnimationIntent,
    SemanticTransactionNodeRef, SemanticVec3, TrackValues, Transform2D, Vec2,
};

use super::affine::{
    EffectiveAnimationProperties, LoweredAffineChannel, SemanticAnimationCompletion,
};

#[derive(Clone, Debug, PartialEq)]
pub enum SampledMotionLoweringError {
    InvalidDeclaration,
    UnsupportedTarget,
    UnsupportedOptions,
    MissingEffectiveCenter,
    InvalidMotion(SampledMotionError),
}

impl std::fmt::Display for SampledMotionLoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDeclaration => f.write_str("sampled motion schedule does not match its declaration"),
            Self::UnsupportedTarget => f.write_str("sampled motion requires a retained planar leaf without affine reactive or host drivers"),
            Self::UnsupportedOptions => f.write_str("sampled motion supports bounded reversible timing without lifecycle, lag, or path-arc options"),
            Self::MissingEffectiveCenter => f.write_str("sampled motion requires the activation-effective center of retained content"),
            Self::InvalidMotion(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for SampledMotionLoweringError {}

pub(super) fn prepared_data(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    animation: SemanticTransactionNodeRef,
) -> Result<Arc<SampledMotionData>, SampledMotionLoweringError> {
    match animation {
        SemanticTransactionNodeRef::Existing(node) => {
            let state = prepared
                .store()
                .semantic_animation_state(node)
                .map_err(|_| SampledMotionLoweringError::InvalidDeclaration)?;
            match state.intent() {
                SemanticAnimationIntent::SampledMotion { motion, .. } => Ok(motion.clone()),
                _ => Err(SampledMotionLoweringError::InvalidDeclaration),
            }
        }
        SemanticTransactionNodeRef::Pending(token) => {
            let state = prepared
                .pending_animation(token)
                .map_err(|_| SampledMotionLoweringError::InvalidDeclaration)?;
            match state.intent() {
                SemanticTransactionAnimationIntent::SampledMotion { motion, .. } => {
                    Ok(motion.clone())
                }
                _ => Err(SampledMotionLoweringError::InvalidDeclaration),
            }
        }
    }
}

/// Cold typed lowering has no effective-content override. Live lowering instead
/// supplies Runtime's read-only retained-content bounds query at the captured pose.
pub(super) fn declared_center(
    source: &SemanticObjectState,
    store: &SemanticStore,
    transform: Transform2D,
) -> Option<Vec2> {
    let geometry = super::super::compiled_scene::lower_semantic_geometry_value(
        source.content.geometry()?,
        Some(store),
    )
    .ok()?;
    Some(geometry.world_bounds(transform)?.center())
}

pub(super) fn channels(
    store: &SemanticStore,
    source: &SemanticObjectState,
    from: &EffectiveAnimationProperties,
    center: Option<Vec2>,
    motion: Arc<SampledMotionData>,
    options: ResolvedAnimationOptions,
) -> Result<Vec<LoweredAffineChannel>, SampledMotionLoweringError> {
    if options.introducer
        || options.remover
        || options.lag_ratio != 0.0
        || options.path_arc != 0.0
        || !options.rate_func.preserves_unit_interval()
    {
        return Err(SampledMotionLoweringError::UnsupportedOptions);
    }
    if super::super::projection::object_requires_spatial_lowering(source, store)
        || source.signal_bindings().iter().any(|binding| {
            matches!(
                binding.property(),
                SemanticObjectProperty::Translation
                    | SemanticObjectProperty::RotationZ
                    | SemanticObjectProperty::Scale
            )
        })
        || from.world_transform.is_some()
    {
        return Err(SampledMotionLoweringError::UnsupportedTarget);
    }
    let plan = Arc::new(
        SampledMotionPlan::new(
            motion,
            from.transform,
            center.ok_or(SampledMotionLoweringError::MissingEffectiveCenter)?,
        )
        .map_err(SampledMotionLoweringError::InvalidMotion)?,
    );
    let terminal = options
        .rate_func
        .evaluate_f64(if options.reverse_rate_function {
            0.0
        } else {
            1.0
        });
    let (translation, rotation) = plan.sample_f64(terminal);
    Ok(vec![
        LoweredAffineChannel {
            property: Property::Position,
            conflict_property: SemanticObjectProperty::Translation,
            completion: SemanticAnimationCompletion::Property {
                property: SemanticObjectProperty::Translation,
                value: SemanticSignalValue::Vec3(SemanticVec3::new(
                    translation.x,
                    translation.y,
                    0.0,
                )),
            },
            values: TrackValues::SampledPosition { plan: plan.clone() },
        },
        LoweredAffineChannel {
            property: Property::Rotation,
            conflict_property: SemanticObjectProperty::RotationZ,
            completion: SemanticAnimationCompletion::Property {
                property: SemanticObjectProperty::RotationZ,
                value: SemanticSignalValue::Scalar(rotation),
            },
            values: TrackValues::SampledRotation { plan },
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{AnimationDefaults, AnimationOptions, RateFunction, StoredGeometry, Style};

    #[test]
    fn sampled_motion_completion_retains_authored_f64_below_runtime_precision() {
        let destination = SemanticVec3::new(1.0 + 1e-10, -2.0 - 2e-10, 0.0);
        let angle = 0.5 + 3e-10;
        let motion = Arc::new(
            SampledMotionData::new(
                vec![0.3, 1.0],
                vec![SemanticVec3::new(0.25, -0.5, 0.0), destination],
                vec![0.1, angle],
            )
            .unwrap(),
        );
        let store = SemanticStore::new();
        let source = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 });
        let from = EffectiveAnimationProperties {
            fill_paint: None,
            effects: None,
            z_index: 0.0,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            reveal: 1.0,
            world_transform: None,
            camera_profile: None,
        };
        let options = noon_core::resolve_sampled_motion_animation_options(
            AnimationDefaults::MANIM,
            AnimationOptions::new().rate_func(RateFunction::Linear),
            AnimationOptions::new(),
        )
        .unwrap();
        let channels = channels(&store, &source, &from, Some(Vec2::ZERO), motion, options).unwrap();
        assert_eq!(
            channels[0].completion,
            SemanticAnimationCompletion::Property {
                property: SemanticObjectProperty::Translation,
                value: SemanticSignalValue::Vec3(destination),
            }
        );
        assert_eq!(
            channels[1].completion,
            SemanticAnimationCompletion::Property {
                property: SemanticObjectProperty::RotationZ,
                value: SemanticSignalValue::Scalar(angle),
            }
        );
        let plan = channels[0].values.sampled_motion_plan().unwrap();
        assert_eq!(
            plan.sample_f64(1.0),
            (noon_core::DVec2::new(destination.x, destination.y), angle)
        );
        let (position, rotation) = plan.sample(1.0);
        assert_eq!(
            position,
            Vec2::new(destination.x as f32, destination.y as f32)
        );
        assert_eq!(rotation, angle as f32);
        assert_ne!(f64::from(position.x), destination.x);
        assert_ne!(f64::from(position.y), destination.y);
        assert_ne!(f64::from(rotation), angle);
        assert_eq!(plan.sample_f64(0.0), (noon_core::DVec2::ZERO, 0.0));
    }
}
