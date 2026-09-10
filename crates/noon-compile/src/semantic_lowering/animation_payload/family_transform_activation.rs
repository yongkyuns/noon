use std::collections::HashMap;

use noon_core::{
    ObjectId, PreparedSemanticMutationTransaction, SemanticNodeId, SemanticTransactionNodeRef,
};

use super::affine::EffectiveAnimationProperties;
use super::family_transform::{
    derive_family_transform_correspondence, FamilyTransformCorrespondenceError,
};
use super::super::{
    PreparedSemanticAnimationScheduleProjection, SemanticExecutionIndex,
};

/// One activation-time family Transform occurrence before execution publication.
///
/// The source/target fields are real authored leaf identities. `source_padding` and
/// `target_padding` describe a renderer-derived occurrence only; they do not allocate
/// semantic or execution identity. `source_execution_object_id` is the existing source
/// leaf's stable execution row and serves only as the effective-state capture/ordering
/// anchor for a derived source copy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedFamilyTransformOccurrence {
    pub animation: SemanticTransactionNodeRef,
    pub source: SemanticNodeId,
    pub target_state: SemanticNodeId,
    pub source_execution_object_id: ObjectId,
    pub source_padding: bool,
    pub target_padding: bool,
    pub occurrence_index: u32,
    pub timing: noon_core::TrackTiming,
    pub time_map: noon_core::CompositionTimeMap,
    pub finish_time_map: noon_core::CompositionTimeMap,
    pub options: noon_core::ResolvedAnimationOptions,
    pub effective_source: EffectiveAnimationProperties,
}

/// Compiler-owned activation projection for every family Transform in one prepared
/// animation graph. It is pure preparation data: no Semantic Scene mutation, runtime
/// patch, derived display row, or completion state is published here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreparedFamilyTransformActivationProjection {
    occurrences: Vec<PreparedFamilyTransformOccurrence>,
}

impl PreparedFamilyTransformActivationProjection {
    pub fn occurrences(&self) -> &[PreparedFamilyTransformOccurrence] {
        &self.occurrences
    }

    pub fn len(&self) -> usize {
        self.occurrences.len()
    }

    pub fn is_empty(&self) -> bool {
        self.occurrences.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedFamilyTransformActivationError {
    PendingFamilyEndpoint {
        animation: SemanticTransactionNodeRef,
        endpoint: SemanticTransactionNodeRef,
    },
    Correspondence {
        animation: SemanticTransactionNodeRef,
        error: FamilyTransformCorrespondenceError,
    },
    MissingExecutionSource {
        animation: SemanticTransactionNodeRef,
        source: SemanticNodeId,
    },
    MissingEffectiveSource {
        animation: SemanticTransactionNodeRef,
        source: SemanticNodeId,
        execution_object_id: ObjectId,
    },
    TooManyOccurrences(usize),
}

impl std::fmt::Display for PreparedFamilyTransformActivationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PendingFamilyEndpoint { animation, endpoint } => write!(
                formatter,
                "prepared family Transform {animation:?} retains pending family endpoint {endpoint:?}"
            ),
            Self::Correspondence { animation, error } => {
                write!(formatter, "prepared family Transform {animation:?} correspondence failed: {error}")
            }
            Self::MissingExecutionSource { animation, source } => write!(
                formatter,
                "prepared family Transform {animation:?} source leaf {}:{} is not in the stable execution index",
                source.slot(),
                source.generation()
            ),
            Self::MissingEffectiveSource {
                animation,
                source,
                execution_object_id,
            } => write!(
                formatter,
                "prepared family Transform {animation:?} source leaf {}:{} has no activation-effective execution state for object {}",
                source.slot(),
                source.generation(),
                execution_object_id.get()
            ),
            Self::TooManyOccurrences(count) => write!(
                formatter,
                "prepared family Transform has {count} occurrences, exceeding u32 occurrence indexing"
            ),
        }
    }
}

impl std::error::Error for PreparedFamilyTransformActivationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Correspondence { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Combine shared family scheduling with compiler-derived correspondence and capture
/// each distinct source leaf's coherent effective value exactly once.
///
/// Family endpoints used by the current live-session API are existing semantic
/// families. Pending endpoints fail closed here rather than receiving guessed identity.
/// Repeated source padding occurrences deliberately reuse the same captured effective
/// source value; independent visual evolution is introduced only by the later
/// identity-free derived-display materializer.
pub fn prepare_family_transform_activations<F>(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
    schedule: &PreparedSemanticAnimationScheduleProjection,
    mut effective_properties: F,
) -> Result<PreparedFamilyTransformActivationProjection, PreparedFamilyTransformActivationError>
where
    F: FnMut(ObjectId) -> Option<EffectiveAnimationProperties>,
{
    let mut captures = HashMap::<ObjectId, EffectiveAnimationProperties>::new();
    let mut occurrences = Vec::new();

    for family in schedule.family_transforms() {
        let source_family = existing_endpoint(family.animation, family.source)?;
        let target_family = existing_endpoint(family.animation, family.target_state)?;
        let correspondence = derive_family_transform_correspondence(
            prepared.store(),
            source_family,
            target_family,
        )
        .map_err(|error| PreparedFamilyTransformActivationError::Correspondence {
            animation: family.animation,
            error,
        })?;

        for correspondence_member in correspondence.occurrences() {
            let source = correspondence_member.source();
            let execution_object_id = index.execution_object_id(source).ok_or(
                PreparedFamilyTransformActivationError::MissingExecutionSource {
                    animation: family.animation,
                    source,
                },
            )?;
            let effective_source = if let Some(captured) = captures.get(&execution_object_id) {
                *captured
            } else {
                let captured = effective_properties(execution_object_id).ok_or(
                    PreparedFamilyTransformActivationError::MissingEffectiveSource {
                        animation: family.animation,
                        source,
                        execution_object_id,
                    },
                )?;
                captures.insert(execution_object_id, captured);
                captured
            };
            let occurrence_index = u32::try_from(occurrences.len()).map_err(|_| {
                PreparedFamilyTransformActivationError::TooManyOccurrences(occurrences.len())
            })?;
            occurrences.push(PreparedFamilyTransformOccurrence {
                animation: family.animation,
                source,
                target_state: correspondence_member.target(),
                source_execution_object_id: execution_object_id,
                source_padding: correspondence_member.source_is_padding(),
                target_padding: correspondence_member.target_is_padding(),
                occurrence_index,
                timing: family.timing,
                time_map: family.time_map.clone(),
                finish_time_map: family.finish_time_map.clone(),
                options: family.options,
                effective_source,
            });
        }
    }

    Ok(PreparedFamilyTransformActivationProjection { occurrences })
}

fn existing_endpoint(
    animation: SemanticTransactionNodeRef,
    endpoint: SemanticTransactionNodeRef,
) -> Result<SemanticNodeId, PreparedFamilyTransformActivationError> {
    endpoint
        .existing()
        .ok_or(PreparedFamilyTransformActivationError::PendingFamilyEndpoint {
            animation,
            endpoint,
        })
}
