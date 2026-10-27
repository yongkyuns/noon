//! Existing-attachment target edits use ordinary parameter channels and driver
//! ownership. This finite slice does not enroll/remove attachments or create a
//! separate effect scheduler, runtime state, or completion policy.
use super::affine::{
    channel_driver_key, transform_driver_conflict, DriverMap, SemanticAnimationCompletion,
};
#[cfg(test)]
use noon_core::EffectDefinition;
use noon_core::{
    EffectParameterError, EffectUpdate, ObjectId, Property, SemanticAnimationEffectSnapshot,
    SemanticNodeId, SemanticStore, SemanticTransactionReadError, SemanticTransformEffectSnapshot,
    TimelineError, TrackTiming, TrackValues,
};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub enum EffectTargetLoweringError {
    Read(SemanticTransactionReadError),
    Store(noon_core::SemanticStoreError),
    Profile(super::super::projection::SemanticLoweringError),
    UnsupportedTopology,
    StaleAttachment,
    MissingEffectiveValue,
    UnsupportedContentChange,
    UnsupportedPulseOptions,
    UnsupportedTargetOptions,
    Parameter(EffectParameterError),
    Timeline(TimelineError),
}
impl std::fmt::Display for EffectTargetLoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPulseOptions => f.write_str(
                "GlowPulse does not support introducer/remover flags, nonzero lag, or path arcs",
            ),
            _ => write!(f, "effect animation activation rejected: {self:?}"),
        }
    }
}
impl std::error::Error for EffectTargetLoweringError {}

/// Common frozen-generation and activation-value projection for object and
/// composed-family targets. Scheduling and track installation stay with the
/// ordinary prepared animation path.
#[allow(clippy::too_many_arguments)]
pub(super) fn prepared_channels(
    prepared: &noon_core::PreparedSemanticMutationTransaction<'_>,
    animation: noon_core::SemanticTransactionNodeRef,
    owner: noon_core::SemanticTransactionNodeRef,
    target: noon_core::SemanticTransactionNodeRef,
    current: Option<std::sync::Arc<crate::CompiledEffectStack>>,
    newly_admitted: bool,
    method_target: bool,
    temporary: &HashSet<SemanticNodeId>,
) -> Result<Vec<(Property, TrackValues)>, EffectTargetLoweringError> {
    let source = prepared
        .animation_effect_snapshot(owner)
        .map_err(EffectTargetLoweringError::Read)?;
    let target = prepared
        .animation_effect_snapshot(target)
        .map_err(EffectTargetLoweringError::Read)?;
    let snapshot = prepared
        .transform_effect_snapshot(animation)
        .map_err(EffectTargetLoweringError::Read)?;
    let effective = prepared_effective(current, &source, newly_admitted, |attachment| {
        prepared.store().node(attachment).is_none()
    })?;
    channels(
        snapshot,
        &source,
        &target,
        effective.as_deref(),
        method_target,
        temporary,
    )
}

pub(super) fn channels(
    snapshot: Option<&SemanticTransformEffectSnapshot>,
    source: &[SemanticAnimationEffectSnapshot],
    target: &[SemanticAnimationEffectSnapshot],
    effective: Option<&crate::CompiledEffectStack>,
    method_target: bool,
    temporary_attachments: &HashSet<SemanticNodeId>,
) -> Result<Vec<(Property, TrackValues)>, EffectTargetLoweringError> {
    // A captured target can omit an exact temporary neutral glow, while still
    // describing independent persistent attachments on the same owner.
    let omitted: HashSet<_> = source
        .iter()
        .filter(|effect| {
            temporary_attachments.contains(&effect.attachment)
                && effect
                    .definition
                    .as_glow()
                    .is_some_and(|glow| glow.intensity() == 0.0)
                && effective.is_some_and(|stack| stack.get(effect.attachment).is_some())
                && !target
                    .iter()
                    .any(|destination| destination.name == effect.name)
        })
        .map(|effect| effect.attachment)
        .collect();
    let source: Vec<_> = source
        .iter()
        .filter(|effect| !omitted.contains(&effect.attachment))
        .cloned()
        .collect();
    let Some(snapshot) = snapshot else {
        return if source.is_empty()
            && target.is_empty()
            && effective
                .is_none_or(|stack| stack.attachments().iter().all(|id| omitted.contains(id)))
        {
            Ok(Vec::new())
        } else {
            Err(EffectTargetLoweringError::StaleAttachment)
        };
    };
    let expected: Vec<_> = snapshot
        .source
        .iter()
        .copied()
        .filter(|id| !omitted.contains(id))
        .collect();
    if !same_identity(&expected, &source) || !same_identity(&snapshot.target, target) {
        return Err(EffectTargetLoweringError::StaleAttachment);
    }
    if source.len() != target.len() {
        return Err(EffectTargetLoweringError::UnsupportedTopology);
    }
    let mut channels = Vec::new();
    for (initial, destination) in source.iter().zip(target) {
        let effect = effective
            .and_then(|stack| stack.get(initial.attachment))
            .ok_or(EffectTargetLoweringError::MissingEffectiveValue)?;
        channels.extend(attachment_channels(
            initial,
            destination,
            effect,
            method_target,
        )?);
    }
    Ok(channels)
}

fn attachment_channels(
    initial: &SemanticAnimationEffectSnapshot,
    destination: &SemanticAnimationEffectSnapshot,
    effective: crate::CompiledEffect,
    method_target: bool,
) -> Result<Vec<(Property, TrackValues)>, EffectTargetLoweringError> {
    if initial.name != destination.name {
        return Err(EffectTargetLoweringError::UnsupportedTopology);
    }
    if effective.attachment != initial.attachment {
        return Err(EffectTargetLoweringError::StaleAttachment);
    }
    let authored_source = initial.definition;
    let copy_intent = destination
        .copy_provenance
        .filter(|copy| copy.source_attachment() == initial.attachment);
    let initial = effective.definition;
    let destination = destination.definition;
    // The attachment kind and discrete parameters cannot change in-place.
    use noon_core::EffectValue;
    effective
        .definition
        .validate_effect_transition(destination)
        .map_err(EffectTargetLoweringError::Parameter)?;
    authored_source
        .validate_effect_transition(destination)
        .map_err(EffectTargetLoweringError::Parameter)?;
    let mut update = EffectUpdate::default();
    // A copied method target describes edits relative to its captured baseline.
    // Current effective differences can come from another driver and must not
    // become additional target writes. The explicit mask also retains writes
    // whose authored value happens to equal their copied or effective value.
    let (baseline, writes_color, writes_radius, writes_intensity, writes_offset) = match copy_intent
    {
        Some(copy) => {
            let copied_baseline = copy.copied_definition();
            (
                if method_target {
                    copied_baseline
                } else {
                    initial
                },
                copy.writes_parameter(noon_core::EffectParameter::Color),
                copy.writes_parameter(noon_core::EffectParameter::Radius),
                copy.writes_parameter(noon_core::EffectParameter::Intensity),
                copy.writes_parameter(noon_core::EffectParameter::Offset),
            )
        }
        None => (initial, false, false, false, false),
    };
    if let (Some(a), Some(b)) = (baseline.color(), destination.color()) {
        if writes_color || a != b {
            update.color = Some(b);
        }
    }
    if let (Some(a), Some(b)) = (baseline.as_glow(), destination.as_glow()) {
        if writes_intensity || a.intensity() != b.intensity() {
            update.intensity = Some(b.intensity());
        }
    }
    if let (Some(a), Some(b)) = (baseline.as_shadow(), destination.as_shadow()) {
        if writes_offset || a.offset() != b.offset() {
            update.offset = Some(b.offset());
        }
    }
    if writes_radius || baseline.radius() != destination.radius() {
        update.radius = Some(destination.radius());
    }
    effective
        .parameter_channels(update)
        .map_err(EffectTargetLoweringError::Parameter)
}

fn same_identity(a: &[SemanticNodeId], b: &[SemanticAnimationEffectSnapshot]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| *a == b.attachment)
}

/// Fill only allocator-proven newly staged neutral glows (or a newly admitted
/// owner's authored values). Existing generations always use runtime captures.
pub(super) fn prepared_effective(
    current: Option<std::sync::Arc<crate::CompiledEffectStack>>,
    source: &[SemanticAnimationEffectSnapshot],
    newly_admitted: bool,
    unpublished: impl Fn(SemanticNodeId) -> bool,
) -> Result<Option<std::sync::Arc<crate::CompiledEffectStack>>, EffectTargetLoweringError> {
    if current
        .as_ref()
        .is_some_and(|stack| same_identity(stack.attachments(), source))
    {
        return Ok(current);
    }
    if current.as_ref().is_some_and(|stack| {
        stack
            .attachments()
            .iter()
            .any(|id| !source.iter().any(|effect| effect.attachment == *id))
    }) {
        return Err(EffectTargetLoweringError::StaleAttachment);
    }
    if source.is_empty() {
        return Ok(None);
    }
    let mut values = Vec::with_capacity(source.len().min(noon_core::MAX_EFFECT_STACK_LEN));
    for effect in source {
        let value = current
            .as_ref()
            .and_then(|stack| stack.get(effect.attachment));
        if let Some(value) = value {
            values.push(value);
        } else if newly_admitted
            || (unpublished(effect.attachment)
                && effect
                    .definition
                    .as_glow()
                    .is_some_and(|glow| glow.intensity() == 0.0))
        {
            values.push(crate::CompiledEffect {
                attachment: effect.attachment,
                definition: effect.definition,
            });
        } else {
            return Err(EffectTargetLoweringError::MissingEffectiveValue);
        }
    }
    let stack = crate::CompiledEffectStack::new(values)
        .map_err(|_| EffectTargetLoweringError::UnsupportedTopology)?;
    Ok(Some(std::sync::Arc::new(stack)))
}

pub(super) fn validate_sources(
    store: &SemanticStore,
    source_id: SemanticNodeId,
    source: &noon_core::SemanticObjectState,
    target_id: SemanticNodeId,
    target: &noon_core::SemanticObjectState,
) -> Result<(), EffectTargetLoweringError> {
    for (id, state) in [(source_id, source), (target_id, target)] {
        super::super::projection::validate_effect_source_profile(id, state, store)
            .map_err(EffectTargetLoweringError::Profile)?;
    }
    if source.content != target.content {
        return Err(EffectTargetLoweringError::UnsupportedContentChange);
    }
    Ok(())
}

/// Reserve one generation-checked attachment parameter in the ordinary
/// object-local ownership table.
pub(super) fn reserve<T: Copy + PartialEq>(
    driven: &mut DriverMap<T>,
    object: ObjectId,
    property: Property,
    values: &TrackValues,
    animation: T,
) -> Result<(), T> {
    let key = channel_driver_key(object, property, values);
    if let Some(owner) = transform_driver_conflict(driven, object, property, animation)
        .or_else(|| driven.get(&key).copied())
    {
        return Err(owner);
    }
    driven.insert(key, animation);
    Ok(())
}

pub(super) fn completion(
    values: &TrackValues,
    timing: TrackTiming,
) -> Result<SemanticAnimationCompletion, EffectTargetLoweringError> {
    let TrackValues::Effect {
        attachment,
        from,
        to,
        ..
    } = values
    else {
        unreachable!("glow channel");
    };
    let endpoint = from
        .sample_value(*to, values.terminal_progress_f64(timing))
        .map_err(EffectTargetLoweringError::Timeline)?;
    Ok(SemanticAnimationCompletion::Effect {
        attachment: *attachment,
        value: endpoint,
    })
}

#[cfg(test)]
mod tests {
    use super::super::affine::driver_key;
    use super::*;

    fn parameter(attachment: SemanticNodeId, property: Property) -> TrackValues {
        let definition = if property == Property::EffectOffset {
            EffectDefinition::Shadow(noon_core::Shadow::default())
        } else {
            EffectDefinition::Glow(noon_core::Glow::default())
        };
        let value = noon_core::EffectTrackValue::from_definition(property, definition).unwrap();
        TrackValues::Effect {
            attachment,
            from: value,
            to: value,
            end_behavior: noon_core::EffectTrackEndBehavior::Sample,
        }
    }

    #[test]
    fn empty_target_requires_the_exact_candidate_temporary_generation() {
        let attachment = SemanticNodeId::new(3, 7);
        let definition = noon_core::Glow::new(EffectUpdate::default().intensity(0.0)).unwrap();
        let source = [SemanticAnimationEffectSnapshot {
            attachment,
            name: "glow".into(),
            definition: EffectDefinition::Glow(definition),
            copy_provenance: None,
        }];
        let stack = crate::CompiledEffectStack::new([crate::CompiledEffect {
            attachment,
            definition: definition.into(),
        }])
        .unwrap();
        let effective = Some(&stack);
        let proof = HashSet::from([attachment]);
        let current = SemanticTransformEffectSnapshot {
            source: vec![attachment].into_boxed_slice(),
            target: Box::new([]),
        };
        let stale = SemanticTransformEffectSnapshot {
            source: vec![SemanticNodeId::new(3, 6)].into_boxed_slice(),
            target: Box::new([]),
        };
        assert!(channels(None, &source, &[], effective, false, &HashSet::new()).is_err());
        assert!(channels(None, &source, &[], effective, false, &proof)
            .unwrap()
            .is_empty());
        assert!(
            channels(Some(&current), &source, &[], effective, false, &proof)
                .unwrap()
                .is_empty()
        );
        assert!(channels(Some(&stale), &source, &[], effective, false, &proof).is_err());
        assert!(channels(
            None,
            &source,
            &[],
            Some(
                &crate::CompiledEffectStack::new([crate::CompiledEffect {
                    attachment: SemanticNodeId::new(3, 6),
                    definition: definition.into(),
                }])
                .unwrap()
            ),
            false,
            &proof,
        )
        .is_err());
        let wrong_proof = HashSet::from([SemanticNodeId::new(3, 6)]);
        assert!(channels(None, &source, &[], effective, false, &wrong_proof).is_err());
        let mut non_neutral = source;
        non_neutral[0].definition = EffectDefinition::Glow(noon_core::Glow::default());
        assert!(channels(None, &non_neutral, &[], effective, false, &proof).is_err());
    }

    #[test]
    fn glow_driver_ownership_is_parameter_local_and_symmetric_with_umbrella_tracks() {
        let object = ObjectId::new(17);
        let attachment = SemanticNodeId::new(3, 7);
        for effect in [
            Property::EffectColor,
            Property::EffectRadius,
            Property::EffectIntensity,
            Property::EffectOffset,
        ] {
            for umbrella in [
                Property::Transform,
                Property::Morph,
                Property::WorldTransform,
                Property::CameraProfile,
            ] {
                let mut driven = DriverMap::new();
                let values = parameter(attachment, effect);
                reserve(&mut driven, object, effect, &values, 1_u32).unwrap();
                assert_eq!(
                    transform_driver_conflict(&driven, object, umbrella, 2),
                    Some(1)
                );
                driven.clear();
                driven.insert(driver_key(object, umbrella), 1);
                assert_eq!(reserve(&mut driven, object, effect, &values, 2), Err(1));
                assert!(!driven.contains_key(&channel_driver_key(object, effect, &values)));
            }
        }
        let mut driven = DriverMap::new();
        driven.insert(driver_key(object, Property::Position), 1_u32);
        for (owner, effect) in [
            (2, Property::EffectIntensity),
            (3, Property::EffectColor),
            (4, Property::EffectRadius),
        ] {
            reserve(
                &mut driven,
                object,
                effect,
                &parameter(attachment, effect),
                owner,
            )
            .unwrap();
        }
        assert_eq!(driven.len(), 4);
        assert_eq!(
            reserve(
                &mut driven,
                object,
                Property::EffectIntensity,
                &parameter(attachment, Property::EffectIntensity),
                5
            ),
            Err(2)
        );
        assert_eq!(
            reserve(
                &mut driven,
                ObjectId::new(18),
                Property::EffectIntensity,
                &parameter(attachment, Property::EffectIntensity),
                5
            ),
            Ok(())
        );
    }

    #[test]
    fn same_parameter_on_distinct_attachments_reserves_independent_drivers() {
        let object = ObjectId::new(17);
        let first = SemanticNodeId::new(3, 7);
        let second = SemanticNodeId::new(4, 2);
        let radius = Property::EffectRadius;
        let mut driven = DriverMap::new();
        let a = parameter(first, radius);
        let b = parameter(second, radius);
        reserve(&mut driven, object, radius, &a, 1_u32).unwrap();
        reserve(&mut driven, object, radius, &b, 2_u32).unwrap();
        assert_eq!(reserve(&mut driven, object, radius, &a, 3), Err(1));
        assert_eq!(reserve(&mut driven, object, radius, &b, 3), Err(2));
        assert_eq!(
            transform_driver_conflict(&driven, object, Property::WorldTransform, 1),
            Some(2)
        );
        assert_eq!(
            transform_driver_conflict(&driven, object, Property::Reveal, 3),
            None
        );
        // Object prefixes and full generations both participate in ownership.
        let reused = parameter(
            SemanticNodeId::new(first.slot(), first.generation() + 1),
            radius,
        );
        reserve(&mut driven, object, radius, &reused, 3).unwrap();
        reserve(&mut driven, ObjectId::new(18), radius, &a, 4).unwrap();
        assert_eq!(
            super::super::affine::object_drivers(&driven, object).count(),
            3
        );
        assert_eq!(
            super::super::affine::object_drivers(&driven, ObjectId::new(18)).count(),
            1
        );
    }

    #[test]
    fn same_declaration_umbrella_cannot_hide_another_declarations_conflict() {
        let object = ObjectId::new(17);
        let driven = DriverMap::from([
            (driver_key(object, Property::Morph), 1_u32),
            (driver_key(object, Property::WorldTransform), 2_u32),
        ]);
        assert_eq!(
            transform_driver_conflict(&driven, object, Property::EffectIntensity, 1),
            Some(2)
        );
        let driven = DriverMap::from([
            (driver_key(object, Property::Position), 1_u32),
            (
                channel_driver_key(
                    object,
                    Property::EffectIntensity,
                    &parameter(SemanticNodeId::new(3, 7), Property::EffectIntensity),
                ),
                2_u32,
            ),
        ]);
        assert_eq!(
            transform_driver_conflict(&driven, object, Property::WorldTransform, 1),
            Some(2)
        );
    }
}
