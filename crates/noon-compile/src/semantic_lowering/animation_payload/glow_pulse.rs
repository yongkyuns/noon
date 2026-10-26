//! Pulse lowering produces one ordinary intensity channel. Enrollment and
//! lifetime ownership remain at the atomic execution-session boundary.
use super::glow_target::EffectTargetLoweringError;
use noon_core::{
    EffectTrackEndBehavior, EffectTrackValue, EffectUpdate, SemanticAnimationEffectSnapshot,
    SemanticNodeId, SemanticObjectState, SemanticStore, TrackValues,
};

pub(super) fn validate_options(
    options: noon_core::ResolvedAnimationOptions,
) -> Result<(), EffectTargetLoweringError> {
    if options.lag_ratio != 0.0 || options.path_arc != 0.0 || options.remover || options.introducer
    {
        return Err(EffectTargetLoweringError::UnsupportedPulseOptions);
    }
    Ok(())
}

pub(super) fn values(
    store: &SemanticStore,
    target: SemanticNodeId,
    state: &SemanticObjectState,
    attachments: &[SemanticAnimationEffectSnapshot],
    effective: Option<crate::CompiledEffect>,
    intensity: f64,
) -> Result<TrackValues, EffectTargetLoweringError> {
    super::super::projection::validate_effect_source_profile(target, state, store)
        .map_err(EffectTargetLoweringError::Profile)?;
    let [attachment] = attachments else {
        return Err(EffectTargetLoweringError::UnsupportedTopology);
    };
    if attachment.name.as_ref() != "glow" {
        return Err(EffectTargetLoweringError::UnsupportedTopology);
    }
    let effective = effective.ok_or(EffectTargetLoweringError::MissingEffectiveValue)?;
    if effective.attachment != attachment.attachment {
        return Err(EffectTargetLoweringError::StaleAttachment);
    }
    EffectUpdate::default()
        .intensity(intensity)
        .prepare(effective.definition)
        .map_err(EffectTargetLoweringError::Parameter)?;
    Ok(TrackValues::Effect {
        attachment: attachment.attachment,
        from: EffectTrackValue::Intensity(
            effective
                .definition
                .as_glow()
                .ok_or(EffectTargetLoweringError::UnsupportedTopology)?
                .intensity(),
        ),
        to: EffectTrackValue::Intensity(intensity),
        end_behavior: EffectTrackEndBehavior::RestoreSource,
    })
}
