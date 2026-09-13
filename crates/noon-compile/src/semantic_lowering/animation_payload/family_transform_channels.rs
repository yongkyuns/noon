use std::collections::{hash_map::Entry, HashMap};

use noon_core::{
    ObjectId, PreparedSemanticMutationTransaction, Property, SemanticNodeId,
    SemanticObjectProperty, SemanticTransactionNodeRef, TimelineError, TrackDefinition, TrackId,
    TrackValues,
};

use super::affine::{
    completion_at_endpoint, driver_key, lower_transform_channels, transform_driver_conflict,
    validate_affine_payload, AffinePayloadIssue, LoweredAffineChannel, SemanticAnimationCompletion,
};
use super::family_transform_activation::{
    PreparedFamilyTransformActivationProjection, PreparedFamilyTransformOccurrence,
};
use super::prepared_composition::{
    PreparedSemanticAnimationLoweringError, PreparedSemanticAnimationTrack,
};

/// One ordinary stable-source family Transform track plus its completion policy.
///
/// `retain_effective` is execution-session release metadata, not authored animation
/// meaning. It is used only for a real source occurrence that maps to a padded target:
/// the padding fade has no authored property to receive its endpoint, so completion
/// must leave that exact presentation value effective instead of reconciling it to
/// the stable row's base appearance.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedFamilyTransformStableTrack {
    pub track: PreparedSemanticAnimationTrack,
    pub retain_effective: bool,
}

/// One identity-free execution channel for a repeated source occurrence.
///
/// The channel intentionally owns no `ObjectId`, semantic node, completion action, or
/// persistent topology. `anchor_execution_object_id` points only to the real source
/// row whose activation-effective state was copied before this derived occurrence
/// began evolving independently.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedDerivedFamilyTransformTrack {
    pub occurrence_index: u32,
    pub anchor_execution_object_id: ObjectId,
    pub property: Property,
    pub values: TrackValues,
    pub timing: noon_core::TrackTiming,
    pub time_map: noon_core::CompositionTimeMap,
}

/// One source-padding display occurrence ready for runtime-only evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedDerivedFamilyTransformOccurrence {
    pub occurrence_index: u32,
    pub anchor_execution_object_id: ObjectId,
    pub source: SemanticNodeId,
    pub target_state: SemanticNodeId,
    pub effective_source: super::EffectiveAnimationProperties,
    pub tracks: Vec<PreparedDerivedFamilyTransformTrack>,
}

/// Compiler-owned channel projection for a prepared unequal-family Transform.
///
/// Real source occurrences reuse the ordinary prepared animation-track vocabulary so
/// execution publication/completion stays on the existing path. Repeated source
/// occurrences are retained separately as plan-local display channels and never enter
/// the stable object-slot domain.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreparedFamilyTransformChannelProjection {
    stable_tracks: Vec<PreparedFamilyTransformStableTrack>,
    derived_occurrences: Vec<PreparedDerivedFamilyTransformOccurrence>,
}

impl PreparedFamilyTransformChannelProjection {
    pub fn stable_tracks(&self) -> &[PreparedFamilyTransformStableTrack] {
        &self.stable_tracks
    }

    pub fn derived_occurrences(&self) -> &[PreparedDerivedFamilyTransformOccurrence] {
        &self.derived_occurrences
    }

    pub fn is_empty(&self) -> bool {
        self.stable_tracks.is_empty() && self.derived_occurrences.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedFamilyTransformChannelError {
    Target {
        animation: SemanticTransactionNodeRef,
        node: SemanticTransactionNodeRef,
        error: noon_core::SemanticTransactionReadError,
    },
    Payload(PreparedSemanticAnimationLoweringError),
    MultipleDrivers {
        first_animation: SemanticTransactionNodeRef,
        next_animation: SemanticTransactionNodeRef,
        target: SemanticNodeId,
        property: SemanticObjectProperty,
    },
}

impl std::fmt::Display for PreparedFamilyTransformChannelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Target {
                animation,
                node,
                error,
            } => write!(
                formatter,
                "prepared family Transform {animation:?} cannot read object {node:?}: {error}"
            ),
            Self::Payload(error) => error.fmt(formatter),
            Self::MultipleDrivers {
                first_animation,
                next_animation,
                target,
                property,
            } => write!(
                formatter,
                "prepared family Transform {next_animation:?} conflicts with {first_animation:?} on {property:?} for source {}:{}",
                target.slot(),
                target.generation()
            ),
        }
    }
}

impl std::error::Error for PreparedFamilyTransformChannelError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Target { error, .. } => Some(error),
            Self::Payload(error) => Some(error),
            Self::MultipleDrivers { .. } => None,
        }
    }
}

/// Lower activation-effective family correspondence through the canonical Transform
/// payload lowerer.
///
/// Real source leaves become ordinary stable execution tracks. Repeated source
/// occurrences receive identity-free derived channels. Manim's alignment fade is
/// represented in the execution-only Appearance domain:
///
/// - source padding starts at appearance 0;
/// - target padding ends at appearance 0;
/// - a real target ends at appearance 1.
///
/// For a real source mapping to target padding, the appearance channel is marked
/// `retain_effective`; the session completion layer must keep only that presentation
/// endpoint while canonical geometry/transform/style channels complete normally.
/// This function remains pure and publishes nothing.
pub fn lower_prepared_family_transform_channels(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    activation: &PreparedFamilyTransformActivationProjection,
) -> Result<PreparedFamilyTransformChannelProjection, PreparedFamilyTransformChannelError> {
    let mut stable_tracks = Vec::new();
    let mut derived_occurrences = Vec::new();
    let mut driven = HashMap::<(u64, u8), SemanticTransactionNodeRef>::new();

    for occurrence in activation.occurrences() {
        let source_ref: SemanticTransactionNodeRef = occurrence.source.into();
        let target_ref: SemanticTransactionNodeRef = occurrence.target_state.into();
        let source = prepared.object_state(source_ref).map_err(|error| {
            PreparedFamilyTransformChannelError::Target {
                animation: occurrence.animation,
                node: source_ref,
                error,
            }
        })?;
        let target = prepared.object_state(target_ref).map_err(|error| {
            PreparedFamilyTransformChannelError::Target {
                animation: occurrence.animation,
                node: target_ref,
                error,
            }
        })?;

        validate_affine_payload(source, target, occurrence.options)
            .map_err(|issue| payload_error(occurrence, issue))?;
        let channels = lower_transform_channels(
            prepared.store(),
            source,
            target,
            occurrence.effective_source,
            noon_core::SemanticTransformInterpolation::Affine,
        )
        .map_err(|issue| payload_error(occurrence, issue))?;

        let target_appearance = if occurrence.target_padding { 0.0 } else { 1.0 };
        if occurrence.source_padding {
            let mut tracks = channels
                .into_iter()
                .map(|channel| PreparedDerivedFamilyTransformTrack {
                    occurrence_index: occurrence.occurrence_index,
                    anchor_execution_object_id: occurrence.source_execution_object_id,
                    property: channel.property,
                    values: channel.values,
                    timing: occurrence.timing,
                    time_map: occurrence.time_map.clone(),
                })
                .collect::<Vec<_>>();
            tracks.push(PreparedDerivedFamilyTransformTrack {
                occurrence_index: occurrence.occurrence_index,
                anchor_execution_object_id: occurrence.source_execution_object_id,
                property: Property::Appearance,
                values: TrackValues::Scalar {
                    from: 0.0,
                    to: target_appearance,
                },
                timing: occurrence.timing,
                time_map: occurrence.time_map.clone(),
            });
            derived_occurrences.push(PreparedDerivedFamilyTransformOccurrence {
                occurrence_index: occurrence.occurrence_index,
                anchor_execution_object_id: occurrence.source_execution_object_id,
                source: occurrence.source,
                target_state: occurrence.target_state,
                effective_source: occurrence.effective_source,
                tracks,
            });
            continue;
        }

        for channel in channels {
            push_stable_channel(occurrence, channel, false, &mut driven, &mut stable_tracks)?;
        }
        if occurrence.effective_source.appearance != target_appearance {
            push_stable_channel(
                occurrence,
                LoweredAffineChannel {
                    property: Property::Appearance,
                    conflict_property: SemanticObjectProperty::Presence,
                    completion: SemanticAnimationCompletion::Release,
                    values: TrackValues::Scalar {
                        from: occurrence.effective_source.appearance,
                        to: target_appearance,
                    },
                },
                occurrence.target_padding,
                &mut driven,
                &mut stable_tracks,
            )?;
        }
    }

    Ok(PreparedFamilyTransformChannelProjection {
        stable_tracks,
        derived_occurrences,
    })
}

fn push_stable_channel(
    occurrence: &PreparedFamilyTransformOccurrence,
    channel: LoweredAffineChannel,
    retain_effective: bool,
    driven: &mut HashMap<(u64, u8), SemanticTransactionNodeRef>,
    tracks: &mut Vec<PreparedFamilyTransformStableTrack>,
) -> Result<(), PreparedFamilyTransformChannelError> {
    if let Some(first_animation) = transform_driver_conflict(
        driven,
        occurrence.source_execution_object_id,
        channel.property,
        occurrence.animation,
    ) {
        return Err(PreparedFamilyTransformChannelError::MultipleDrivers {
            first_animation,
            next_animation: occurrence.animation,
            target: occurrence.source,
            property: channel.conflict_property,
        });
    }
    match driven.entry(driver_key(
        occurrence.source_execution_object_id,
        channel.property,
    )) {
        Entry::Occupied(entry) => {
            return Err(PreparedFamilyTransformChannelError::MultipleDrivers {
                first_animation: *entry.get(),
                next_animation: occurrence.animation,
                target: occurrence.source,
                property: channel.conflict_property,
            });
        }
        Entry::Vacant(entry) => {
            entry.insert(occurrence.animation);
        }
    }

    tracks.push(PreparedFamilyTransformStableTrack {
        track: PreparedSemanticAnimationTrack {
            animation: occurrence.animation,
            target: occurrence.source.into(),
            execution_object_id: occurrence.source_execution_object_id,
            property: channel.property,
            completion: completion_at_endpoint(channel.completion, occurrence.timing.easing),
            values: channel.values,
            timing: occurrence.timing,
            time_map: occurrence.time_map.clone(),
        },
        retain_effective,
    });
    Ok(())
}

fn payload_error(
    occurrence: &PreparedFamilyTransformOccurrence,
    issue: AffinePayloadIssue,
) -> PreparedFamilyTransformChannelError {
    let animation = occurrence.animation;
    let target: SemanticTransactionNodeRef = occurrence.source.into();
    let target_state: SemanticTransactionNodeRef = occurrence.target_state.into();
    let error = match issue {
        AffinePayloadIssue::InvalidEffectiveTransform => {
            PreparedSemanticAnimationLoweringError::InvalidEffectiveTransform { animation, target }
        }
        AffinePayloadIssue::InvalidEffectiveStyle => {
            PreparedSemanticAnimationLoweringError::InvalidEffectiveStyle { animation, target }
        }
        AffinePayloadIssue::InvalidEffectiveReveal => {
            PreparedSemanticAnimationLoweringError::InvalidEffectiveReveal { animation, target }
        }
        AffinePayloadIssue::UnsupportedContentChange => {
            PreparedSemanticAnimationLoweringError::UnsupportedContentChange {
                animation,
                target,
                target_state,
            }
        }
        AffinePayloadIssue::UnsupportedPointCorrespondence => {
            PreparedSemanticAnimationLoweringError::UnsupportedPointCorrespondence {
                animation,
                target,
                target_state,
            }
        }
        AffinePayloadIssue::UnsupportedStyleChange => {
            PreparedSemanticAnimationLoweringError::UnsupportedStyleChange {
                animation,
                target,
                target_state,
            }
        }
        AffinePayloadIssue::UnsupportedBindingChange => {
            PreparedSemanticAnimationLoweringError::UnsupportedBindingChange {
                animation,
                target,
                target_state,
            }
        }
        AffinePayloadIssue::UnsupportedDepthChange(field) => {
            PreparedSemanticAnimationLoweringError::UnsupportedDepthChange {
                animation,
                target,
                target_state,
                field,
            }
        }
        AffinePayloadIssue::UnsupportedLifecycle {
            remover,
            introducer,
        } => PreparedSemanticAnimationLoweringError::UnsupportedLifecycle {
            animation,
            remover,
            introducer,
        },
        AffinePayloadIssue::ReactiveDriverConflict(property) => {
            PreparedSemanticAnimationLoweringError::ReactiveDriverConflict {
                animation,
                target,
                property,
            }
        }
        AffinePayloadIssue::InvalidTargetValue { field, error } => {
            PreparedSemanticAnimationLoweringError::InvalidTargetValue {
                animation,
                target_state,
                field,
                error,
            }
        }
        AffinePayloadIssue::TargetValueOutOfRange(field) => {
            PreparedSemanticAnimationLoweringError::TargetValueOutOfRange {
                animation,
                target_state,
                field,
            }
        }
        AffinePayloadIssue::InvalidTargetStyle(error) => {
            PreparedSemanticAnimationLoweringError::InvalidTargetStyle {
                animation,
                target_state,
                error,
            }
        }
    };
    PreparedFamilyTransformChannelError::Payload(error)
}

/// Attach a stable execution-track ID at the same boundary used by ordinary prepared
/// animation tracks. This helper exists only to make the channel projection directly
/// consumable by the live-session activation layer without inventing another track
/// representation there.
pub fn materialize_family_transform_stable_track(
    track: &PreparedFamilyTransformStableTrack,
    id: TrackId,
) -> Result<TrackDefinition, TimelineError> {
    track.track.with_track_id(id)
}
