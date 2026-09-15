use noon_core::{
    continuous_time_map_interval, ObjectId, PreparedSemanticMutationTransaction,
    SemanticFamilyTransformMode, SemanticNodeId, SemanticTransactionNodeRef,
};

use super::super::{PreparedSemanticScheduledFamilyTransform, SemanticExecutionIndex};
use super::affine::EffectiveAnimationProperties;
use super::matching_shape_activation::{
    prepare_matching_shape_activation_correspondence, PreparedMatchingShapeActivationError,
    PreparedMatchingShapeActivationProjection,
};
use super::prepared_composition::PreparedSemanticAnimationTrack;

/// Scheduled family-transform metadata paired with activation-time matching-shape state.
///
/// This is compiler preparation data only. It does not replace the existing structural
/// family-Transform path, publish runtime identity, or choose matching semantics for an
/// authored request. A later semantic/public adapter can select this projection without
/// creating a second geometry or live-state model.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMatchingFamilyTransformActivation {
    pub animation: SemanticTransactionNodeRef,
    pub source_root: SemanticNodeId,
    pub target_root: SemanticNodeId,
    pub timing: noon_core::TrackTiming,
    pub time_map: noon_core::CompositionTimeMap,
    pub finish_time_map: noon_core::CompositionTimeMap,
    pub options: noon_core::ResolvedAnimationOptions,
    matching: PreparedMatchingShapeActivationProjection,
}

impl PreparedMatchingFamilyTransformActivation {
    pub const fn matching(&self) -> &PreparedMatchingShapeActivationProjection {
        &self.matching
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedMatchingFamilyTransformActivationError {
    PendingFamilyEndpoint {
        animation: SemanticTransactionNodeRef,
        endpoint: SemanticTransactionNodeRef,
    },
    InvalidActivationInterval {
        animation: SemanticTransactionNodeRef,
        error: noon_core::CompositionTimeMapError,
    },
    UnsupportedMode {
        animation: SemanticTransactionNodeRef,
        mode: SemanticFamilyTransformMode,
    },
    Matching {
        animation: SemanticTransactionNodeRef,
        error: PreparedMatchingShapeActivationError,
    },
}

impl std::fmt::Display for PreparedMatchingFamilyTransformActivationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PendingFamilyEndpoint {
                animation,
                endpoint,
            } => write!(
                formatter,
                "prepared matching family Transform {animation:?} retains pending family endpoint {endpoint:?}"
            ),
            Self::InvalidActivationInterval { animation, error } => write!(
                formatter,
                "prepared matching family Transform {animation:?} has an invalid activation interval: {error}"
            ),
            Self::UnsupportedMode { animation, mode } => write!(
                formatter,
                "prepared matching family Transform {animation:?} cannot lower correspondence mode {mode:?}"
            ),
            Self::Matching { animation, error } => write!(
                formatter,
                "prepared matching family Transform {animation:?} activation matching failed: {error}"
            ),
        }
    }
}

impl std::error::Error for PreparedMatchingFamilyTransformActivationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidActivationInterval { error, .. } => Some(error),
            Self::Matching { error, .. } => Some(error),
            Self::PendingFamilyEndpoint { .. } | Self::UnsupportedMode { .. } => None,
        }
    }
}

/// Bind one already-scheduled family Transform to deterministic activation-time
/// matching-shape correspondence.
///
/// `prior_tracks` are the prepared tracks that exist before this family leaf begins.
/// The matching projection therefore sees completed Succession content/affine endpoints
/// from #1579 while overlapping or opaque geometry state continues to fail closed.
pub fn prepare_matching_family_transform_activation<F>(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
    family: &PreparedSemanticScheduledFamilyTransform,
    prior_tracks: &[PreparedSemanticAnimationTrack],
    effective_properties: F,
) -> Result<PreparedMatchingFamilyTransformActivation, PreparedMatchingFamilyTransformActivationError>
where
    F: FnMut(ObjectId) -> Option<EffectiveAnimationProperties>,
{
    if family.mode != SemanticFamilyTransformMode::MatchingShapes {
        return Err(
            PreparedMatchingFamilyTransformActivationError::UnsupportedMode {
                animation: family.animation,
                mode: family.mode,
            },
        );
    }
    let source_root = existing_endpoint(family.animation, family.source)?;
    let target_root = existing_endpoint(family.animation, family.target_state)?;
    let (activation_start, _) = continuous_time_map_interval(family.timing, &family.time_map)
        .map_err(|error| {
            PreparedMatchingFamilyTransformActivationError::InvalidActivationInterval {
                animation: family.animation,
                error,
            }
        })?;
    let matching = prepare_matching_shape_activation_correspondence(
        prepared.store(),
        index,
        source_root,
        target_root,
        activation_start,
        prior_tracks,
        effective_properties,
    )
    .map_err(
        |error| PreparedMatchingFamilyTransformActivationError::Matching {
            animation: family.animation,
            error,
        },
    )?;

    Ok(PreparedMatchingFamilyTransformActivation {
        animation: family.animation,
        source_root,
        target_root,
        timing: family.timing,
        time_map: family.time_map.clone(),
        finish_time_map: family.finish_time_map.clone(),
        options: family.options,
        matching,
    })
}

fn existing_endpoint(
    animation: SemanticTransactionNodeRef,
    endpoint: SemanticTransactionNodeRef,
) -> Result<SemanticNodeId, PreparedMatchingFamilyTransformActivationError> {
    endpoint.existing().ok_or(
        PreparedMatchingFamilyTransformActivationError::PendingFamilyEndpoint {
            animation,
            endpoint,
        },
    )
}
