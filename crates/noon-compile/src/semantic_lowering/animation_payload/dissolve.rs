//! Visibility uses the common effect channels and completion barrier. It owns
//! no clock, semantic identity allocator, source capture or frontend state.
use super::{affine::SemanticAnimationCompletion, glow_target::EffectTargetLoweringError};
use noon_core::{
    DissolveDirection, DissolveStyle, EffectTrackEndBehavior, EffectTrackValue, NoiseDissolve,
    Property, SemanticAnimationEffectSnapshot, TrackTiming, TrackValues,
};

pub(super) fn channels(
    attachments: &[SemanticAnimationEffectSnapshot],
    effective: Option<&crate::CompiledEffectStack>,
    direction: DissolveDirection,
    style: DissolveStyle,
    timing: TrackTiming,
    remover: bool,
) -> Result<(Vec<(Property, TrackValues)>, SemanticAnimationCompletion), EffectTargetLoweringError>
{
    let binding = attachments
        .iter()
        .find(|value| value.name.is_visibility())
        .ok_or(EffectTargetLoweringError::UnsupportedTopology)?;
    let initial = effective
        .and_then(|values| values.get(binding.attachment))
        .map_or(binding.definition, |value| value.definition)
        .as_noise_dissolve()
        .ok_or(EffectTargetLoweringError::UnsupportedTopology)?;
    let target = style
        .resolve(initial)
        .with_progress(match direction {
            DissolveDirection::In => 0.0,
            DissolveDirection::Out => 1.0,
        })
        .expect("finite dissolve endpoint");
    let pairs = [
        (
            EffectTrackValue::Progress(initial.progress()),
            EffectTrackValue::Progress(target.progress()),
        ),
        (
            EffectTrackValue::Softness(target.softness()),
            EffectTrackValue::Softness(target.softness()),
        ),
        (
            EffectTrackValue::Seed(target.seed()),
            EffectTrackValue::Seed(target.seed()),
        ),
    ];
    let channels: Vec<_> = pairs
        .into_iter()
        .map(|(from, to)| {
            (
                from.property(),
                TrackValues::Effect {
                    attachment: binding.attachment,
                    from,
                    to,
                    end_behavior: EffectTrackEndBehavior::Sample,
                },
            )
        })
        .collect();
    let SemanticAnimationCompletion::Effect {
        value: EffectTrackValue::Progress(progress),
        ..
    } = super::glow_target::completion(&channels[0].1, timing)?
    else {
        unreachable!("progress channel")
    };
    let endpoint: NoiseDissolve = target
        .with_progress(progress)
        .expect("checked terminal progress");
    Ok((
        channels,
        SemanticAnimationCompletion::Visibility {
            attachment: binding.attachment,
            endpoint,
            remover,
        },
    ))
}
