use noon_core::{SemanticNodeId, SemanticTransactionNodeRef};

use super::affine::SemanticAnimationCompletion;
use super::family_transform_channels::{
    PreparedDerivedFamilyTransformOccurrence, PreparedFamilyTransformStableTrack,
};
use super::matching_family_transform_activation::PreparedMatchingFamilyTransformActivation;
use super::matching_family_transform_channels::{
    lower_prepared_matching_family_transform_channels, PreparedMatchingFamilyTransformChannelError,
};
use super::matching_shape_leftover_fades::{
    prepare_matching_shape_leftover_fades, PreparedMatchingShapeLeftoverFadeError,
    PreparedMatchingShapeSourceLeftoverFade, PreparedMatchingShapeTargetLeftoverFade,
};
use super::prepared_composition::PreparedSemanticAnimationTrack;

/// One compiler-owned matching-family payload ready for the existing execution-session
/// family Transform path.
///
/// Matched groups use the ordinary stable/derived family Transform vocabulary. Source
/// leftovers are ordinary stable release tracks. Detached target leftovers remain
/// identity-free transient staging until activation materialization. The source/target
/// roots are retained explicitly so exact segment completion can replace the authored
/// source family with the authored target family instead of retaining hidden leftovers.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMatchingFamilyTransformPayload {
    source_root: SemanticNodeId,
    target_root: SemanticNodeId,
    stable_tracks: Vec<PreparedFamilyTransformStableTrack>,
    derived_occurrences: Vec<PreparedDerivedFamilyTransformOccurrence>,
    target_leftovers: Vec<PreparedMatchingShapeTargetLeftoverFade>,
    target_occurrence_index_start: u32,
}

impl PreparedMatchingFamilyTransformPayload {
    pub const fn source_root(&self) -> SemanticNodeId {
        self.source_root
    }

    pub const fn target_root(&self) -> SemanticNodeId {
        self.target_root
    }

    pub fn stable_tracks(&self) -> &[PreparedFamilyTransformStableTrack] {
        &self.stable_tracks
    }

    pub fn derived_occurrences(&self) -> &[PreparedDerivedFamilyTransformOccurrence] {
        &self.derived_occurrences
    }

    pub fn target_leftovers(&self) -> &[PreparedMatchingShapeTargetLeftoverFade] {
        &self.target_leftovers
    }

    pub const fn target_occurrence_index_start(&self) -> u32 {
        self.target_occurrence_index_start
    }

    pub fn is_empty(&self) -> bool {
        self.stable_tracks.is_empty()
            && self.derived_occurrences.is_empty()
            && self.target_leftovers.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedMatchingFamilyTransformPayloadError {
    Channels(PreparedMatchingFamilyTransformChannelError),
    Leftovers(PreparedMatchingShapeLeftoverFadeError),
    OccurrenceIndexExhausted(u32),
}

impl std::fmt::Display for PreparedMatchingFamilyTransformPayloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Channels(error) => error.fmt(formatter),
            Self::Leftovers(error) => error.fmt(formatter),
            Self::OccurrenceIndexExhausted(index) => write!(
                formatter,
                "matching-family transient occurrence index {index} cannot reserve a following target-leftover range"
            ),
        }
    }
}

impl std::error::Error for PreparedMatchingFamilyTransformPayloadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Channels(error) => Some(error),
            Self::Leftovers(error) => Some(error),
            Self::OccurrenceIndexExhausted(_) => None,
        }
    }
}

impl From<PreparedMatchingFamilyTransformChannelError>
    for PreparedMatchingFamilyTransformPayloadError
{
    fn from(value: PreparedMatchingFamilyTransformChannelError) -> Self {
        Self::Channels(value)
    }
}

impl From<PreparedMatchingShapeLeftoverFadeError> for PreparedMatchingFamilyTransformPayloadError {
    fn from(value: PreparedMatchingShapeLeftoverFadeError) -> Self {
        Self::Leftovers(value)
    }
}

/// Assemble the complete default `TransformMatchingShapes` payload below any public
/// adapter and above runtime publication.
///
/// Equal-key matches stay on the canonical family Transform channels. Unmatched source
/// members receive directional Position/Appearance tracks with `Release` completion;
/// exact completion is expected to remove the source family as part of the explicit
/// root swap carried by this payload. Unmatched target members remain detached transient
/// bases until the runtime resolves their painter placement.
pub fn lower_prepared_matching_family_transform_payload(
    prepared: &noon_core::PreparedSemanticMutationTransaction<'_>,
    activation: &PreparedMatchingFamilyTransformActivation,
) -> Result<PreparedMatchingFamilyTransformPayload, PreparedMatchingFamilyTransformPayloadError> {
    let matched = lower_prepared_matching_family_transform_channels(prepared, activation)?;
    let leftovers = prepare_matching_shape_leftover_fades(prepared.store(), activation)?;

    let target_occurrence_index_start =
        next_transient_occurrence_index(matched.derived_occurrences())?;
    let mut stable_tracks = matched.stable_tracks().to_vec();
    for source in leftovers.source_fades() {
        stable_tracks.extend(source_leftover_tracks(activation.animation, source));
    }

    Ok(PreparedMatchingFamilyTransformPayload {
        source_root: activation.source_root,
        target_root: activation.target_root,
        stable_tracks,
        derived_occurrences: matched.derived_occurrences().to_vec(),
        target_leftovers: leftovers.target_fades().to_vec(),
        target_occurrence_index_start,
    })
}

fn next_transient_occurrence_index(
    occurrences: &[PreparedDerivedFamilyTransformOccurrence],
) -> Result<u32, PreparedMatchingFamilyTransformPayloadError> {
    let Some(maximum) = occurrences
        .iter()
        .map(|occurrence| occurrence.occurrence_index)
        .max()
    else {
        return Ok(0);
    };
    maximum
        .checked_add(1)
        .ok_or(PreparedMatchingFamilyTransformPayloadError::OccurrenceIndexExhausted(maximum))
}

fn source_leftover_tracks(
    animation: SemanticTransactionNodeRef,
    source: &PreparedMatchingShapeSourceLeftoverFade,
) -> Vec<PreparedFamilyTransformStableTrack> {
    source
        .tracks
        .iter()
        .map(|track| PreparedFamilyTransformStableTrack {
            track: PreparedSemanticAnimationTrack {
                animation,
                target: source.node.into(),
                execution_object_id: source.execution_object_id,
                property: track.property,
                completion: SemanticAnimationCompletion::Release,
                values: track.values.clone(),
                timing: track.timing,
                time_map: track.time_map.clone(),
            },
            retain_effective: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use noon_core::{
        CompositionTimeMap, ObjectId, Property, SemanticNodeId, TrackTiming, TrackValues, Vec2,
    };

    use super::*;

    #[test]
    fn target_occurrence_range_starts_after_highest_derived_occurrence() {
        let occurrence = |index| PreparedDerivedFamilyTransformOccurrence {
            occurrence_index: index,
            anchor_execution_object_id: ObjectId::new(1),
            source: SemanticNodeId::new(1, 0),
            target_state: SemanticNodeId::new(2, 0),
            effective_source: super::super::EffectiveAnimationProperties {
                z_index: 0.0,
                transform: noon_core::Transform2D::IDENTITY,
                style: noon_core::Style::default(),
                appearance: 1.0,
                reveal: 1.0,
            },
            tracks: Vec::new(),
        };

        assert_eq!(next_transient_occurrence_index(&[]).unwrap(), 0);
        assert_eq!(
            next_transient_occurrence_index(&[occurrence(2), occurrence(7)]).unwrap(),
            8
        );
        assert_eq!(
            next_transient_occurrence_index(&[occurrence(u32::MAX)]).unwrap_err(),
            PreparedMatchingFamilyTransformPayloadError::OccurrenceIndexExhausted(u32::MAX)
        );
    }

    #[test]
    fn source_leftover_tracks_release_instead_of_retaining_hidden_state() {
        let source = PreparedMatchingShapeSourceLeftoverFade {
            source_index: 0,
            node: SemanticNodeId::new(3, 0),
            execution_object_id: ObjectId::new(9),
            tracks: vec![
                super::super::PreparedMatchingShapeLeftoverFadeTrack {
                    property: Property::Position,
                    values: TrackValues::Vec2 {
                        from: Vec2::ZERO,
                        to: Vec2::new(4.0, 0.0),
                    },
                    timing: TrackTiming::new(1.0, 2.0, noon_core::RateFunction::Linear),
                    time_map: CompositionTimeMap::identity(),
                },
                super::super::PreparedMatchingShapeLeftoverFadeTrack {
                    property: Property::Appearance,
                    values: TrackValues::Scalar { from: 1.0, to: 0.0 },
                    timing: TrackTiming::new(1.0, 2.0, noon_core::RateFunction::Linear),
                    time_map: CompositionTimeMap::identity(),
                },
            ],
        };
        let animation: SemanticTransactionNodeRef = SemanticNodeId::new(5, 0).into();

        let tracks = source_leftover_tracks(animation, &source);
        assert_eq!(tracks.len(), 2);
        assert!(tracks.iter().all(|track| !track.retain_effective));
        assert!(tracks
            .iter()
            .all(|track| track.track.completion == SemanticAnimationCompletion::Release));
        assert!(tracks
            .iter()
            .all(|track| track.track.animation == animation));
    }
}
