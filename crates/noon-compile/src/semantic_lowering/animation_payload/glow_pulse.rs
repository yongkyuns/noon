//! Pulse lowering produces one ordinary intensity channel. Enrollment and
//! lifetime ownership remain at the atomic execution-session boundary.
use super::glow_target::GlowTargetLoweringError;
use noon_core::{
    GlowTrackEndBehavior, GlowTrackValue, GlowUpdate, SemanticAnimationEffectSnapshot,
    SemanticNodeId, SemanticObjectState, SemanticStore, TrackValues,
};

pub(super) fn validate_options(
    options: noon_core::ResolvedAnimationOptions,
) -> Result<(), GlowTargetLoweringError> {
    if options.lag_ratio != 0.0 || options.path_arc != 0.0 || options.remover || options.introducer
    {
        return Err(GlowTargetLoweringError::UnsupportedPulseOptions);
    }
    Ok(())
}

pub(super) fn values(
    store: &SemanticStore,
    target: SemanticNodeId,
    state: &SemanticObjectState,
    attachments: &[SemanticAnimationEffectSnapshot],
    effective: Option<crate::CompiledGlow>,
    intensity: f64,
) -> Result<TrackValues, GlowTargetLoweringError> {
    super::super::projection::validate_glow_source_profile(target, state, store)
        .map_err(GlowTargetLoweringError::Profile)?;
    let [attachment] = attachments else {
        return Err(GlowTargetLoweringError::UnsupportedTopology);
    };
    if attachment.name.as_ref() != "glow" {
        return Err(GlowTargetLoweringError::UnsupportedTopology);
    }
    let effective = effective.ok_or(GlowTargetLoweringError::MissingEffectiveValue)?;
    if effective.attachment != attachment.attachment {
        return Err(GlowTargetLoweringError::StaleAttachment);
    }
    GlowUpdate::default()
        .intensity(intensity)
        .prepare(effective.definition)
        .map_err(GlowTargetLoweringError::Parameter)?;
    Ok(TrackValues::Glow {
        attachment: attachment.attachment,
        from: GlowTrackValue::Intensity(effective.definition.intensity()),
        to: GlowTrackValue::Intensity(intensity),
        end_behavior: GlowTrackEndBehavior::RestoreSource,
    })
}
