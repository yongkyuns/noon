//! Visibility uses the common effect channels and completion barrier. It owns
//! no clock, semantic identity allocator, source capture or frontend state.
use super::{affine::SemanticAnimationCompletion, glow_target::EffectTargetLoweringError};
use noon_core::{
    DissolveDirection, DissolveStyle, EffectTrackEndBehavior, EffectTrackValue, Property,
    SemanticAnimationEffectSnapshot, TrackTiming, TrackValues,
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
        .map_or_else(
            || binding.definition.clone(),
            |value| value.definition.clone(),
        );
    let target = style
        .resolve_definition(&initial, direction)
        .map_err(|_| EffectTargetLoweringError::UnsupportedTopology)?;
    let target = noon_core::EffectUpdate::default()
        .progress(match direction {
            DissolveDirection::In => 0.0,
            DissolveDirection::Out => 1.0,
        })
        .apply_to(target)
        .expect("finite dissolve endpoint");
    let pairs = [
        (
            EffectTrackValue::Progress(initial.visibility_progress().expect("visibility progress")),
            EffectTrackValue::Progress(target.visibility_progress().expect("visibility progress")),
        ),
        (
            EffectTrackValue::Softness(target.visibility_softness().expect("visibility softness")),
            EffectTrackValue::Softness(target.visibility_softness().expect("visibility softness")),
        ),
        (
            EffectTrackValue::Seed(target.visibility_seed().expect("visibility seed")),
            EffectTrackValue::Seed(target.visibility_seed().expect("visibility seed")),
        ),
    ];
    let channels: Vec<_> = pairs
        .into_iter()
        .chain(
            target
                .as_radial_mask()
                .into_iter()
                .flat_map(|v| v.parameters().values())
                .map(|value| {
                    (
                        EffectTrackValue::Radial(value),
                        EffectTrackValue::Radial(value),
                    )
                }),
        )
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
    let endpoint = noon_core::EffectUpdate::default()
        .progress(progress)
        .apply_to(target)
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
