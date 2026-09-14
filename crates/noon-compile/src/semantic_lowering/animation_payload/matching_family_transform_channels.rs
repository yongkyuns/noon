use std::collections::{hash_map::Entry, HashMap};

use noon_core::{
    ObjectId, PreparedSemanticMutationTransaction, Property, SemanticNodeId,
    SemanticObjectProperty, SemanticTransactionNodeRef, TrackValues,
};

use crate::transform::compile_transform_geometry_values;

use super::affine::{
    completion_at_endpoint, driver_key, lower_transform_channels, transform_driver_conflict,
    validate_affine_payload, AffinePayloadIssue, LoweredAffineChannel, SemanticAnimationCompletion,
};
use super::family_transform_channels::{
    PreparedDerivedFamilyTransformOccurrence, PreparedDerivedFamilyTransformTrack,
    PreparedFamilyTransformChannelError, PreparedFamilyTransformStableTrack,
};
use super::matching_family_transform_activation::PreparedMatchingFamilyTransformActivation;
use super::prepared_composition::{
    PreparedSemanticAnimationLoweringError, PreparedSemanticAnimationTrack,
};

/// Existing family-Transform execution vocabulary populated only from equal-key
/// matching-shape groups.
///
/// Unmatched source/target members are intentionally absent. Their directional
/// FadeOut/FadeIn behavior belongs to the following lowering slice rather than being
/// disguised as arbitrary positional Transform pairs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreparedMatchingFamilyTransformChannelProjection {
    stable_tracks: Vec<PreparedFamilyTransformStableTrack>,
    derived_occurrences: Vec<PreparedDerivedFamilyTransformOccurrence>,
}

impl PreparedMatchingFamilyTransformChannelProjection {
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
pub enum PreparedMatchingFamilyTransformChannelError {
    Channel(PreparedFamilyTransformChannelError),
    InvalidSourceIndex(usize),
    InvalidTargetIndex(usize),
    TooManyOccurrences(usize),
}

impl std::fmt::Display for PreparedMatchingFamilyTransformChannelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Channel(error) => error.fmt(formatter),
            Self::InvalidSourceIndex(index) => {
                write!(formatter, "matching-shape source index {index} is out of bounds")
            }
            Self::InvalidTargetIndex(index) => {
                write!(formatter, "matching-shape target index {index} is out of bounds")
            }
            Self::TooManyOccurrences(count) => write!(
                formatter,
                "matching-shape transform has {count} matched occurrences, exceeding u32 occurrence indexing"
            ),
        }
    }
}

impl std::error::Error for PreparedMatchingFamilyTransformChannelError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Channel(error) => Some(error),
            Self::InvalidSourceIndex(_)
            | Self::InvalidTargetIndex(_)
            | Self::TooManyOccurrences(_) => None,
        }
    }
}

impl From<PreparedFamilyTransformChannelError> for PreparedMatchingFamilyTransformChannelError {
    fn from(value: PreparedFamilyTransformChannelError) -> Self {
        Self::Channel(value)
    }
}

/// Lower only deterministic equal-key groups through the canonical family Transform
/// execution vocabulary.
///
/// Duplicate equal-key groups are aligned as flat Manim VGroups: the shorter side is
/// expanded with the same repeat-index rule used by ordinary family Transform. Repeated
/// source members become identity-free derived display occurrences; repeated targets
/// fade the corresponding real source occurrence to transparent. Activation-effective
/// source content from the matching projection replaces only the compiler-local source
/// clone before canonical Transform lowering, so a prior completed Morph in Succession
/// is animated from the geometry actually visible when this matching transform begins.
pub fn lower_prepared_matching_family_transform_channels(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    activation: &PreparedMatchingFamilyTransformActivation,
) -> Result<
    PreparedMatchingFamilyTransformChannelProjection,
    PreparedMatchingFamilyTransformChannelError,
> {
    let matching = activation.matching();
    let mut stable_tracks = Vec::new();
    let mut derived_occurrences = Vec::new();
    let mut driven = HashMap::<(u64, u8), SemanticTransactionNodeRef>::new();
    let mut occurrence_count = 0usize;

    for group in &matching.correspondence().matched_groups {
        let count = group.source_indices.len().max(group.target_indices.len());
        let source_occurrences = expand_group_indices(&group.source_indices, count);
        let target_occurrences = expand_group_indices(&group.target_indices, count);

        for ((source_index, source_padding), (target_index, target_padding)) in
            source_occurrences.into_iter().zip(target_occurrences)
        {
            let source_member = matching.source_members().get(source_index).ok_or(
                PreparedMatchingFamilyTransformChannelError::InvalidSourceIndex(source_index),
            )?;
            let target_member = matching.target_members().get(target_index).ok_or(
                PreparedMatchingFamilyTransformChannelError::InvalidTargetIndex(target_index),
            )?;
            let occurrence_index = u32::try_from(occurrence_count).map_err(|_| {
                PreparedMatchingFamilyTransformChannelError::TooManyOccurrences(occurrence_count)
            })?;
            occurrence_count += 1;

            lower_matched_occurrence(
                prepared,
                activation,
                source_member,
                target_member.node,
                source_padding,
                target_padding,
                occurrence_index,
                &mut driven,
                &mut stable_tracks,
                &mut derived_occurrences,
            )?;
        }
    }

    Ok(PreparedMatchingFamilyTransformChannelProjection {
        stable_tracks,
        derived_occurrences,
    })
}

#[allow(clippy::too_many_arguments)]
fn lower_matched_occurrence(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    activation: &PreparedMatchingFamilyTransformActivation,
    source_member: &super::PreparedMatchingShapeSourceMember,
    target_node: SemanticNodeId,
    source_padding: bool,
    target_padding: bool,
    occurrence_index: u32,
    driven: &mut HashMap<(u64, u8), SemanticTransactionNodeRef>,
    stable_tracks: &mut Vec<PreparedFamilyTransformStableTrack>,
    derived_occurrences: &mut Vec<PreparedDerivedFamilyTransformOccurrence>,
) -> Result<(), PreparedMatchingFamilyTransformChannelError> {
    let source_ref: SemanticTransactionNodeRef = source_member.node.into();
    let target_ref: SemanticTransactionNodeRef = target_node.into();
    let mut source = prepared
        .object_state(source_ref)
        .map_err(|error| PreparedFamilyTransformChannelError::Target {
            animation: activation.animation,
            node: source_ref,
            error,
        })?
        .clone();
    let target = prepared.object_state(target_ref).map_err(|error| {
        PreparedFamilyTransformChannelError::Target {
            animation: activation.animation,
            node: target_ref,
            error,
        }
    })?;

    // The authored source node can lag a completed earlier Succession child because
    // candidate-local preparation does not mutate SemanticStore. Use the content
    // endpoint captured by #1579 while retaining the source node's authored binding
    // topology and non-content semantic policy for canonical validation/completion.
    source.content = source_member.content;

    validate_affine_payload(&source, target, activation.options).map_err(|issue| {
        matching_payload_error(activation, source_member.node, target_node, issue)
    })?;
    let channels = lower_transform_channels(
        prepared.store(),
        &source,
        target,
        source_member.effective,
        noon_core::SemanticTransformInterpolation::Affine,
        activation.options.path_arc,
    )
    .map_err(|issue| matching_payload_error(activation, source_member.node, target_node, issue))?;

    let target_appearance = if target_padding { 0.0 } else { 1.0 };
    if source_padding {
        let mut tracks = channels
            .into_iter()
            .map(|channel| {
                let transform_geometry_plan =
                    compile_transform_geometry_values(channel.property, &channel.values).map_err(
                        |failure| PreparedFamilyTransformChannelError::DerivedGeometryPlan {
                            occurrence_index,
                            property: channel.property,
                            failure: failure.into(),
                        },
                    )?;
                Ok(PreparedDerivedFamilyTransformTrack {
                    occurrence_index,
                    anchor_execution_object_id: source_member.execution_object_id,
                    property: channel.property,
                    values: channel.values,
                    transform_geometry_plan,
                    timing: activation.timing,
                    time_map: activation.time_map.clone(),
                })
            })
            .collect::<Result<Vec<_>, PreparedFamilyTransformChannelError>>()?;
        tracks.push(PreparedDerivedFamilyTransformTrack {
            occurrence_index,
            anchor_execution_object_id: source_member.execution_object_id,
            property: Property::Appearance,
            values: TrackValues::Scalar {
                from: 0.0,
                to: target_appearance,
            },
            transform_geometry_plan: None,
            timing: activation.timing,
            time_map: activation.time_map.clone(),
        });
        derived_occurrences.push(PreparedDerivedFamilyTransformOccurrence {
            occurrence_index,
            anchor_execution_object_id: source_member.execution_object_id,
            source: source_member.node,
            target_state: target_node,
            effective_source: source_member.effective,
            tracks,
        });
        return Ok(());
    }

    for channel in channels {
        push_matching_stable_channel(
            activation,
            source_member.node,
            source_member.execution_object_id,
            channel,
            false,
            driven,
            stable_tracks,
        )?;
    }
    if source_member.effective.appearance != target_appearance {
        push_matching_stable_channel(
            activation,
            source_member.node,
            source_member.execution_object_id,
            LoweredAffineChannel {
                property: Property::Appearance,
                conflict_property: SemanticObjectProperty::Presence,
                completion: SemanticAnimationCompletion::Release,
                values: TrackValues::Scalar {
                    from: source_member.effective.appearance,
                    to: target_appearance,
                },
            },
            target_padding,
            driven,
            stable_tracks,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_matching_stable_channel(
    activation: &PreparedMatchingFamilyTransformActivation,
    source: SemanticNodeId,
    execution_object_id: ObjectId,
    channel: LoweredAffineChannel,
    retain_effective: bool,
    driven: &mut HashMap<(u64, u8), SemanticTransactionNodeRef>,
    tracks: &mut Vec<PreparedFamilyTransformStableTrack>,
) -> Result<(), PreparedMatchingFamilyTransformChannelError> {
    if let Some(first_animation) = transform_driver_conflict(
        driven,
        execution_object_id,
        channel.property,
        activation.animation,
    ) {
        return Err(PreparedFamilyTransformChannelError::MultipleDrivers {
            first_animation,
            next_animation: activation.animation,
            target: source,
            property: channel.conflict_property,
        }
        .into());
    }
    match driven.entry(driver_key(execution_object_id, channel.property)) {
        Entry::Occupied(entry) => {
            return Err(PreparedFamilyTransformChannelError::MultipleDrivers {
                first_animation: *entry.get(),
                next_animation: activation.animation,
                target: source,
                property: channel.conflict_property,
            }
            .into());
        }
        Entry::Vacant(entry) => {
            entry.insert(activation.animation);
        }
    }

    tracks.push(PreparedFamilyTransformStableTrack {
        track: PreparedSemanticAnimationTrack {
            animation: activation.animation,
            target: source.into(),
            execution_object_id,
            property: channel.property,
            completion: completion_at_endpoint(channel.completion, activation.timing.easing),
            values: channel.values,
            timing: activation.timing,
            time_map: activation.time_map.clone(),
        },
        retain_effective,
    });
    Ok(())
}

fn matching_payload_error(
    activation: &PreparedMatchingFamilyTransformActivation,
    source: SemanticNodeId,
    target_node: SemanticNodeId,
    issue: AffinePayloadIssue,
) -> PreparedFamilyTransformChannelError {
    let animation = activation.animation;
    let target: SemanticTransactionNodeRef = source.into();
    let target_state: SemanticTransactionNodeRef = target_node.into();
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

fn expand_group_indices(indices: &[usize], target_len: usize) -> Vec<(usize, bool)> {
    debug_assert!(!indices.is_empty());
    debug_assert!(target_len >= indices.len());
    if indices.len() == target_len {
        return indices
            .iter()
            .copied()
            .map(|index| (index, false))
            .collect();
    }

    let mut seen = vec![false; indices.len()];
    (0..target_len)
        .map(|position| {
            let slot = ((position as u128 * indices.len() as u128) / target_len as u128) as usize;
            let padding = std::mem::replace(&mut seen[slot], true);
            (indices[slot], padding)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_group_expansion_uses_manim_repeat_indices() {
        assert_eq!(
            expand_group_indices(&[0, 2], 3),
            vec![(0, false), (0, true), (2, false)]
        );
        assert_eq!(
            expand_group_indices(&[4, 6], 3),
            vec![(4, false), (4, true), (6, false)]
        );
    }

    #[test]
    fn equal_group_cardinality_creates_no_padding() {
        assert_eq!(
            expand_group_indices(&[1, 3, 5], 3),
            vec![(1, false), (3, false), (5, false)]
        );
    }
}
