//! Existing-attachment target edits use ordinary parameter channels and driver
//! ownership. This finite slice does not enroll/remove attachments or create a
//! separate effect scheduler, runtime state, or completion policy.
use super::affine::{driver_key, transform_driver_conflict, SemanticAnimationCompletion};
use noon_core::{
    EffectDefinition, EffectParameterError, EffectTrackValue, EffectUpdate, ObjectId, Property,
    SemanticAnimationEffectSnapshot, SemanticNodeId, SemanticStore, SemanticTransactionReadError,
    SemanticTransformEffectSnapshot, TimelineError, TrackTiming, TrackValues,
};
use std::collections::{HashMap, HashSet};

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

pub(super) fn channels(
    snapshot: Option<&SemanticTransformEffectSnapshot>,
    source: &[SemanticAnimationEffectSnapshot],
    target: &[SemanticAnimationEffectSnapshot],
    effective: Option<crate::CompiledEffect>,
    method_target: bool,
    temporary_attachments: &HashSet<SemanticNodeId>,
) -> Result<Vec<(Property, TrackValues)>, EffectTargetLoweringError> {
    // A motion/style target captured before an absent pulse has no authored
    // effect destination. Only the activation owner's exact candidate-created
    // generation may account for that additional source attachment.
    if let [temporary] = source {
        let definition = temporary.definition;
        if target.is_empty()
            && definition
                .as_glow()
                .is_some_and(|effect| effect.intensity() == 0.0)
            && temporary_attachments.contains(&temporary.attachment)
            && effective.is_some_and(|value| value.attachment == temporary.attachment)
            && snapshot.is_none_or(|snapshot| {
                snapshot.target.is_empty()
                    && (snapshot.source.is_empty() || same_identity(&snapshot.source, source))
            })
        {
            return Ok(Vec::new());
        }
    }
    let Some(snapshot) = snapshot else {
        return if source.is_empty() && target.is_empty() && effective.is_none() {
            Ok(Vec::new())
        } else {
            Err(EffectTargetLoweringError::StaleAttachment)
        };
    };
    if !same_identity(&snapshot.source, source) || !same_identity(&snapshot.target, target) {
        return Err(EffectTargetLoweringError::StaleAttachment);
    }
    let ([initial], [destination]) = (source, target) else {
        return Err(EffectTargetLoweringError::UnsupportedTopology);
    };
    if initial.name != destination.name {
        return Err(EffectTargetLoweringError::UnsupportedTopology);
    }
    let effective = effective.ok_or(EffectTargetLoweringError::MissingEffectiveValue)?;
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
    let (baseline, writes_color, writes_radius, writes_intensity) = match copy_intent {
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
            )
        }
        None => (initial, false, false, false),
    };
    if let (EffectDefinition::Glow(baseline), EffectDefinition::Glow(destination)) =
        (baseline, destination)
    {
        if writes_color || baseline.color() != destination.color() {
            update.color = Some(destination.color());
        }
        if writes_intensity || baseline.intensity() != destination.intensity() {
            update.intensity = Some(destination.intensity());
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

/// Same bounded ownership table as ordinary object channels; the supported
/// single attachment is generation-checked before any key is reserved.
pub(super) fn reserve<T: Copy + PartialEq>(
    driven: &mut HashMap<(u64, u8), T>,
    object: ObjectId,
    property: Property,
    animation: T,
) -> Result<(), T> {
    if let Some(owner) = transform_driver_conflict(driven, object, property, animation)
        .or_else(|| driven.get(&driver_key(object, property)).copied())
    {
        return Err(owner);
    }
    driven.insert(driver_key(object, property), animation);
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
    let update = from
        .sample(*to, values.terminal_progress_f64(timing))
        .map_err(EffectTargetLoweringError::Timeline)?;
    let endpoint = update
        .apply_to(noon_core::Glow::default())
        .map_err(EffectTargetLoweringError::Parameter)?;
    Ok(SemanticAnimationCompletion::Effect {
        attachment: *attachment,
        value: EffectTrackValue::from_definition(from.property(), endpoint).expect("same property"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let effective = Some(crate::CompiledEffect {
            attachment,
            definition: definition.into(),
        });
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
            Some(crate::CompiledEffect {
                attachment: SemanticNodeId::new(3, 6),
                definition: definition.into(),
            }),
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
        for effect in [
            Property::EffectColor,
            Property::EffectRadius,
            Property::EffectIntensity,
        ] {
            for umbrella in [
                Property::Transform,
                Property::Morph,
                Property::WorldTransform,
                Property::CameraProfile,
            ] {
                let mut driven = HashMap::new();
                reserve(&mut driven, object, effect, 1_u32).unwrap();
                assert_eq!(
                    transform_driver_conflict(&driven, object, umbrella, 2),
                    Some(1)
                );
                driven.clear();
                driven.insert(driver_key(object, umbrella), 1);
                assert_eq!(reserve(&mut driven, object, effect, 2), Err(1));
                assert!(!driven.contains_key(&driver_key(object, effect)));
            }
        }
        let mut driven = HashMap::new();
        driven.insert(driver_key(object, Property::Position), 1_u32);
        for (owner, effect) in [
            (2, Property::EffectIntensity),
            (3, Property::EffectColor),
            (4, Property::EffectRadius),
        ] {
            reserve(&mut driven, object, effect, owner).unwrap();
        }
        assert_eq!(driven.len(), 4);
        assert_eq!(
            reserve(&mut driven, object, Property::EffectIntensity, 5),
            Err(2)
        );
        assert_eq!(
            reserve(&mut driven, ObjectId::new(18), Property::EffectIntensity, 5),
            Ok(())
        );
    }

    #[test]
    fn same_declaration_umbrella_cannot_hide_another_declarations_conflict() {
        let object = ObjectId::new(17);
        let driven = HashMap::from([
            (driver_key(object, Property::Morph), 1_u32),
            (driver_key(object, Property::WorldTransform), 2_u32),
        ]);
        assert_eq!(
            transform_driver_conflict(&driven, object, Property::EffectIntensity, 1),
            Some(2)
        );
        let driven = HashMap::from([
            (driver_key(object, Property::Position), 1_u32),
            (driver_key(object, Property::EffectIntensity), 2_u32),
        ]);
        assert_eq!(
            transform_driver_conflict(&driven, object, Property::WorldTransform, 1),
            Some(2)
        );
    }
}
