use noon_core::{Property, SemanticNodeId, SemanticTransactionNodeRef, TrackValues, Vec2};

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
    InvalidTextBounds(SemanticNodeId),
    OccurrenceIndexExhausted(u32),
}

impl std::fmt::Display for PreparedMatchingFamilyTransformPayloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Channels(error) => error.fmt(formatter),
            Self::Leftovers(error) => error.fmt(formatter),
            Self::InvalidTextBounds(node) => write!(formatter, "cannot resolve retained TeX bounds for {node:?}"),
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
            Self::OccurrenceIndexExhausted(_) | Self::InvalidTextBounds(_) => None,
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
    segment_start: f64,
) -> Result<PreparedMatchingFamilyTransformPayload, PreparedMatchingFamilyTransformPayloadError> {
    let matched = lower_prepared_matching_family_transform_channels(prepared, activation)?;

    let target_occurrence_index_start =
        next_transient_occurrence_index(matched.derived_occurrences())?;
    let mut stable_tracks = matched.stable_tracks().to_vec();
    // Matching cleanup restores its source; only the original detached target
    // enters the scene. Do not commit ordinary Transform destinations or
    // retain padding opacity on the removed source. Earlier Succession
    // children still publish their own endpoints, preserving the source
    // state captured when this matching child activated.
    for stable in &mut stable_tracks {
        stable.track.completion = SemanticAnimationCompletion::Release;
        stable.retain_effective = false;
    }
    let target_leftovers = if matches!(
        activation.mode,
        noon_core::SemanticFamilyTransformMode::MatchingMappedSourceKeys(_)
    ) {
        stable_tracks.extend(mapped_text_tracks(
            prepared.store(),
            activation,
            segment_start,
        )?);
        Vec::new()
    } else {
        let leftovers = prepare_matching_shape_leftover_fades(prepared.store(), activation)?;
        for source in leftovers.source_fades() {
            stable_tracks.extend(source_leftover_tracks(activation.animation, source));
        }
        leftovers.target_fades().to_vec()
    };

    Ok(PreparedMatchingFamilyTransformPayload {
        source_root: activation.source_root,
        target_root: activation.target_root,
        stable_tracks,
        derived_occurrences: matched.derived_occurrences().to_vec(),
        target_leftovers,
        target_occurrence_index_start,
    })
}

fn mapped_text_tracks(
    store: &noon_core::SemanticStore,
    activation: &PreparedMatchingFamilyTransformActivation,
    segment_start: f64,
) -> Result<Vec<PreparedFamilyTransformStableTrack>, PreparedMatchingFamilyTransformPayloadError> {
    let matching = activation.matching();
    let (start, _) =
        noon_core::continuous_time_map_interval(activation.timing, &activation.time_map)
            .expect("matching activation already validated its interval");
    let mut tracks = Vec::new();
    let mut push = |node: SemanticNodeId, property, values, timing, time_map| {
        tracks.push(PreparedFamilyTransformStableTrack {
            track: PreparedSemanticAnimationTrack {
                animation: activation.animation,
                target: node.into(),
                execution_object_id: super::super::semantic_execution_object_id(node),
                property,
                completion: SemanticAnimationCompletion::Release,
                values,
                timing,
                time_map,
            },
            retain_effective: false,
        });
    };
    let position = |from, to| {
        let arc_angle = activation.options.path_arc;
        if arc_angle.abs() < noon_core::MANIM_STRAIGHT_PATH_ARC_THRESHOLD {
            TrackValues::Vec2 { from, to }
        } else {
            TrackValues::ArcVec2 {
                from,
                to,
                arc_angle,
            }
        }
    };
    let mut target_motion = vec![None; matching.target_members().len()];
    for &(source_index, target_index) in matching.mapped_pairs() {
        let source = &matching.source_members()[source_index];
        let target = &matching.target_members()[target_index];
        let source_center = source.effective.transform.transform_point(text_center(
            store,
            source.node,
            source.content,
        )?);
        let target_center =
            target
                .transform
                .transform_point(text_center(store, target.node, target.content)?);
        let delta = target_center - source_center;
        let from = source.effective.transform.translation;
        target_motion[target_index] = Some(delta);
        if delta != Vec2::ZERO {
            push(
                source.node,
                Property::Position,
                position(from, from + delta),
                activation.timing,
                activation.time_map.clone(),
            );
        }
        push(
            source.node,
            Property::Appearance,
            TrackValues::Scalar {
                from: source.effective.appearance,
                to: 0.0,
            },
            activation.timing,
            activation.time_map.clone(),
        );
    }
    for &index in &matching.correspondence().unmatched_source_indices {
        let source = &matching.source_members()[index];
        push(
            source.node,
            Property::Appearance,
            TrackValues::Scalar {
                from: source.effective.appearance,
                to: 0.0,
            },
            activation.timing,
            activation.time_map.clone(),
        );
    }
    let mut fade_in = vec![true; matching.target_members().len()];
    for group in &matching.correspondence().matched_groups {
        for &index in &group.target_indices {
            fade_in[index] = false;
        }
    }
    for (index, target) in matching.target_members().iter().enumerate() {
        if start > segment_start {
            push(
                target.node,
                Property::Appearance,
                TrackValues::Scalar { from: 0.0, to: 0.0 },
                noon_core::TrackTiming::new(
                    segment_start,
                    start - segment_start,
                    noon_core::RateFunction::Linear,
                ),
                noon_core::CompositionTimeMap::identity(),
            );
        }
        push(
            target.node,
            Property::Appearance,
            TrackValues::Scalar {
                from: 0.0,
                to: if fade_in[index] { 1.0 } else { 0.0 },
            },
            activation.timing,
            activation.time_map.clone(),
        );
        if let Some(delta) = target_motion[index].filter(|&delta| delta != Vec2::ZERO) {
            let to = target.transform.translation;
            push(
                target.node,
                Property::Position,
                position(to - delta, to),
                activation.timing,
                activation.time_map.clone(),
            );
        }
    }
    Ok(tracks)
}

pub(super) fn text_center(
    store: &noon_core::SemanticStore,
    node: SemanticNodeId,
    content: noon_core::SemanticObjectContent,
) -> Result<Vec2, PreparedMatchingFamilyTransformPayloadError> {
    content
        .text()
        .and_then(|handle| store.text_resources().get(handle))
        .and_then(|resource| resource.bounds.center().lower_f32().ok())
        .ok_or(PreparedMatchingFamilyTransformPayloadError::InvalidTextBounds(node))
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
                fill_paint: None,
                effects: None,
                z_index: 0.0,
                transform: noon_core::Transform2D::IDENTITY,
                style: noon_core::Style::default(),
                appearance: 1.0,
                reveal: 1.0,
                world_transform: None,
                camera_profile: None,
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
