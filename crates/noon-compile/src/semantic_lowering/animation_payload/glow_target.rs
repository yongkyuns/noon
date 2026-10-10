//! Existing-attachment target edits use ordinary parameter channels and driver
//! ownership. This finite slice does not enroll/remove attachments or create a
//! separate effect scheduler, runtime state, or completion policy.
use super::affine::{driver_key, transform_driver_conflict, SemanticAnimationCompletion};
use noon_core::{
    EffectDefinition, GlowParameterError, GlowTrackValue, GlowUpdate, ObjectId, Property,
    SemanticAnimationEffectSnapshot, SemanticNodeId, SemanticStore, SemanticTransactionReadError,
    SemanticTransformEffectSnapshot, TimelineError, TrackTiming, TrackValues,
};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum GlowTargetLoweringError {
    Read(SemanticTransactionReadError),
    Store(noon_core::SemanticStoreError),
    Profile(super::super::projection::SemanticLoweringError),
    UnsupportedTopology,
    StaleAttachment,
    MissingEffectiveValue,
    UnsupportedContentChange,
    Parameter(GlowParameterError),
    Timeline(TimelineError),
}
impl std::fmt::Display for GlowTargetLoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "glow target activation rejected: {self:?}")
    }
}
impl std::error::Error for GlowTargetLoweringError {}

pub(super) fn channels(
    snapshot: Option<&SemanticTransformEffectSnapshot>,
    source: &[SemanticAnimationEffectSnapshot],
    target: &[SemanticAnimationEffectSnapshot],
    effective: Option<crate::CompiledGlow>,
) -> Result<Vec<(Property, TrackValues)>, GlowTargetLoweringError> {
    let Some(snapshot) = snapshot else {
        return if source.is_empty() && target.is_empty() && effective.is_none() {
            Ok(Vec::new())
        } else {
            Err(GlowTargetLoweringError::StaleAttachment)
        };
    };
    if !same_identity(&snapshot.source, source) || !same_identity(&snapshot.target, target) {
        return Err(GlowTargetLoweringError::StaleAttachment);
    }
    let ([initial], [destination]) = (source, target) else {
        return Err(GlowTargetLoweringError::UnsupportedTopology);
    };
    if initial.name != destination.name {
        return Err(GlowTargetLoweringError::UnsupportedTopology);
    }
    let effective = effective.ok_or(GlowTargetLoweringError::MissingEffectiveValue)?;
    if effective.attachment != initial.attachment {
        return Err(GlowTargetLoweringError::StaleAttachment);
    }
    let EffectDefinition::Glow(authored_source) = initial.definition;
    let initial = effective.definition;
    let EffectDefinition::Glow(destination) = destination.definition;
    // Validate discrete source/radius schema even if only another scalar changes.
    GlowUpdate::default()
        .source(destination.source())
        .radius(destination.radius())
        .prepare(effective.definition)
        .map_err(GlowTargetLoweringError::Parameter)?;
    GlowUpdate::default()
        .source(destination.source())
        .radius(destination.radius())
        .prepare(authored_source)
        .map_err(GlowTargetLoweringError::Parameter)?;
    let mut update = GlowUpdate::default();
    if initial.color() != destination.color() {
        update.color = Some(destination.color());
    }
    if initial.radius() != destination.radius() {
        update.radius = Some(destination.radius());
    }
    if initial.intensity() != destination.intensity() {
        update.intensity = Some(destination.intensity());
    }
    effective
        .parameter_channels(update)
        .map_err(GlowTargetLoweringError::Parameter)
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
) -> Result<(), GlowTargetLoweringError> {
    for (id, state) in [(source_id, source), (target_id, target)] {
        super::super::projection::validate_glow_source_profile(id, state, store)
            .map_err(GlowTargetLoweringError::Profile)?;
    }
    if source.content != target.content {
        return Err(GlowTargetLoweringError::UnsupportedContentChange);
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
) -> Result<SemanticAnimationCompletion, GlowTargetLoweringError> {
    let TrackValues::Glow {
        attachment,
        from,
        to,
    } = values
    else {
        unreachable!("glow channel");
    };
    let update = from
        .sample(*to, timing.terminal_progress_f64())
        .map_err(GlowTargetLoweringError::Timeline)?;
    let endpoint = update
        .apply_to(noon_core::Glow::default())
        .map_err(GlowTargetLoweringError::Parameter)?;
    Ok(SemanticAnimationCompletion::Glow {
        attachment: *attachment,
        value: GlowTrackValue::from_definition(from.property(), endpoint).expect("same property"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glow_driver_ownership_is_parameter_local_and_symmetric_with_umbrella_tracks() {
        let object = ObjectId::new(17);
        for glow in [
            Property::GlowColor,
            Property::GlowRadius,
            Property::GlowIntensity,
        ] {
            for umbrella in [
                Property::Transform,
                Property::Morph,
                Property::WorldTransform,
                Property::CameraProfile,
            ] {
                let mut driven = HashMap::new();
                reserve(&mut driven, object, glow, 1_u32).unwrap();
                assert_eq!(
                    transform_driver_conflict(&driven, object, umbrella, 2),
                    Some(1)
                );
                driven.clear();
                driven.insert(driver_key(object, umbrella), 1);
                assert_eq!(reserve(&mut driven, object, glow, 2), Err(1));
                assert!(!driven.contains_key(&driver_key(object, glow)));
            }
        }
        let mut driven = HashMap::new();
        driven.insert(driver_key(object, Property::Position), 1_u32);
        for (owner, glow) in [
            (2, Property::GlowIntensity),
            (3, Property::GlowColor),
            (4, Property::GlowRadius),
        ] {
            reserve(&mut driven, object, glow, owner).unwrap();
        }
        assert_eq!(driven.len(), 4);
        assert_eq!(
            reserve(&mut driven, object, Property::GlowIntensity, 5),
            Err(2)
        );
        assert_eq!(
            reserve(&mut driven, ObjectId::new(18), Property::GlowIntensity, 5),
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
            transform_driver_conflict(&driven, object, Property::GlowIntensity, 1),
            Some(2)
        );
        let driven = HashMap::from([
            (driver_key(object, Property::Position), 1_u32),
            (driver_key(object, Property::GlowIntensity), 2_u32),
        ]);
        assert_eq!(
            transform_driver_conflict(&driven, object, Property::WorldTransform, 1),
            Some(2)
        );
    }
}
