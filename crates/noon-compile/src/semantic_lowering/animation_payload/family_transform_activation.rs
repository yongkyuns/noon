use std::collections::{HashMap, HashSet};

use noon_core::{
    resolve_uniform_composition_schedule, CompositionError, CompositionTimeMapStep, ObjectId,
    PreparedSemanticMutationTransaction, RateFunction, SemanticFamilyTransformMode,
    SemanticMutation, SemanticNodeId, SemanticNodeKind, SemanticStore, SemanticTransactionNodeRef,
};

use super::super::{PreparedSemanticAnimationScheduleProjection, SemanticExecutionIndex};
use super::affine::EffectiveAnimationProperties;
use super::family_transform::{
    derive_family_transform_correspondence, FamilyTransformCorrespondenceError,
};
use super::prepared_composition::authored_activation_properties;

/// One activation-time family Transform occurrence before execution publication.
///
/// The source is an authored leaf identity; the target may be transaction-local. `source_padding` and
/// `target_padding` describe a renderer-derived occurrence only; they do not allocate
/// semantic or execution identity. `source_execution_object_id` is the stable identity
/// of the source leaf's live or newly admitted execution row. Derived copies can only
/// use live rows as effective-state capture and ordering anchors.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedFamilyTransformOccurrence {
    pub animation: SemanticTransactionNodeRef,
    pub source: SemanticNodeId,
    pub target_state: SemanticTransactionNodeRef,
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
    FixedTopology {
        animation: SemanticTransactionNodeRef,
        reason: String,
    },
    Correspondence {
        animation: SemanticTransactionNodeRef,
        error: FamilyTransformCorrespondenceError,
    },
    MemberTiming {
        animation: SemanticTransactionNodeRef,
        error: CompositionError,
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
    UnsupportedMode {
        animation: SemanticTransactionNodeRef,
        mode: SemanticFamilyTransformMode,
    },
    TooManyOccurrences(usize),
}

impl std::fmt::Display for PreparedFamilyTransformActivationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PendingFamilyEndpoint {
                animation,
                endpoint,
            } => write!(
                formatter,
                "prepared family Transform {animation:?} retains pending family endpoint {endpoint:?}"
            ),
            Self::FixedTopology { animation, reason } => write!(
                formatter, "prepared family Transform {animation:?} requires matching ordinary family topology: {reason}"
            ),
            Self::Correspondence { animation, error } => write!(
                formatter,
                "prepared family Transform {animation:?} correspondence failed: {error}"
            ),
            Self::MemberTiming { animation, error } => write!(
                formatter,
                "prepared family Transform {animation:?} member timing failed: {error}"
            ),
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
            Self::UnsupportedMode { animation, mode } => write!(
                formatter,
                "prepared structural family Transform {animation:?} cannot lower correspondence mode {mode:?}"
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
            Self::MemberTiming { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Combine shared family scheduling with compiler-derived correspondence and capture
/// each distinct source leaf's coherent effective value exactly once.
///
/// Family endpoints used by the current live-session API are existing semantic
/// source families. Entering families and local copies use exact tree correspondence.
/// Existing flat endpoints may instead pad their correspondence for structural completion.
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
    let multiple_families = schedule.family_transforms().len() > 1;
    let mut source_groups = HashSet::new();
    // Only staged membership permits a family to begin from authored state.
    let admissions: HashSet<_> = prepared
        .candidate_mutations()
        .filter_map(|mutation| match mutation {
            SemanticMutation::AddMember { member, .. } => member.existing(),
            _ => None,
        })
        .collect();

    for family in schedule.family_transforms() {
        if !family.mode.is_structural() {
            return Err(PreparedFamilyTransformActivationError::UnsupportedMode {
                animation: family.animation,
                mode: family.mode.clone(),
            });
        }
        let source_family = existing_endpoint(family.animation, family.source)?;
        let entering = admissions.contains(&source_family);
        let mut register_group = |id| !multiple_families || source_groups.insert(id);
        let topology_error = |reason| PreparedFamilyTransformActivationError::FixedTopology {
            animation: family.animation,
            reason,
        };
        if !register_group(source_family) {
            return Err(topology_error(
                "family Transforms share a source group".into(),
            ));
        }
        let endpoints: Vec<_> = match (family.target_state.existing(), entering, &family.mode) {
            (Some(target), false, SemanticFamilyTransformMode::Structural) => {
                if ![source_family, target]
                    .into_iter()
                    .all(|id| is_flat(prepared.store(), id))
                {
                    return Err(topology_error(
                        "existing structural endpoints must be flat families".into(),
                    ));
                }
                derive_family_transform_correspondence(prepared.store(), source_family, target)
                    .map_err(
                        |error| PreparedFamilyTransformActivationError::Correspondence {
                            animation: family.animation,
                            error,
                        },
                    )?
                    .occurrences()
                    .iter()
                    .map(|pair| {
                        (
                            pair.source(),
                            pair.target().into(),
                            pair.source_is_padding(),
                            pair.target_is_padding(),
                        )
                    })
                    .collect()
            }
            _ => fixed_family_leaf_pairs(
                prepared,
                source_family,
                family.target_state,
                family.mode == SemanticFamilyTransformMode::FixedTopology,
                register_group,
            )
            .map_err(topology_error)?
            .into_iter()
            .map(|(source, target)| (source, target, false, false))
            .collect(),
        };
        let member_schedule = (family.options.lag_ratio != 0.0)
            .then(|| {
                resolve_uniform_composition_schedule(endpoints.len(), family.options.lag_ratio, 1.0)
                    .map_err(
                        |error| PreparedFamilyTransformActivationError::MemberTiming {
                            animation: family.animation,
                            error,
                        },
                    )
            })
            .transpose()?;

        for (member_index, (source, target_state, source_padding, target_padding)) in
            endpoints.into_iter().enumerate()
        {
            let execution_object_id = index
                .execution_object_id(source)
                .or_else(|| entering.then(|| super::super::semantic_execution_object_id(source)))
                .ok_or(
                    PreparedFamilyTransformActivationError::MissingExecutionSource {
                        animation: family.animation,
                        source,
                    },
                )?;
            let effective_source = if let Some(captured) = captures.get(&execution_object_id) {
                captured.clone()
            } else {
                let captured = effective_properties(execution_object_id)
                    .or_else(|| {
                        if !entering {
                            return None;
                        }
                        authored_activation_properties(prepared.object_state(source).ok()?)
                    })
                    .ok_or(
                        PreparedFamilyTransformActivationError::MissingEffectiveSource {
                            animation: family.animation,
                            source,
                            execution_object_id,
                        },
                    )?;
                captures.insert(execution_object_id, captured.clone());
                captured
            };
            let occurrence_index = u32::try_from(occurrences.len()).map_err(|_| {
                PreparedFamilyTransformActivationError::TooManyOccurrences(occurrences.len())
            })?;
            let mut time_map = family.time_map.clone();
            if let Some(member_schedule) = member_schedule.as_ref() {
                let interval = member_schedule.intervals[member_index];
                time_map.push(CompositionTimeMapStep::new(
                    interval.start_time,
                    interval.duration,
                    RateFunction::Linear,
                ));
            }
            occurrences.push(PreparedFamilyTransformOccurrence {
                animation: family.animation,
                source,
                target_state,
                source_execution_object_id: execution_object_id,
                source_padding,
                target_padding,
                occurrence_index,
                timing: family.timing,
                time_map,
                finish_time_map: family.finish_time_map.clone(),
                options: family.options,
                effective_source,
            });
        }
    }

    Ok(PreparedFamilyTransformActivationProjection { occurrences })
}

fn is_flat(store: &SemanticStore, family: SemanticNodeId) -> bool {
    store
        .semantic_family_members_checked(family)
        .is_ok_and(|members| {
            members.iter().all(|&member| {
                store.node(member).is_some_and(|node| {
                    matches!(node.kind(), SemanticNodeKind::AuthoringObject)
                        && node.member_count() == 0
                })
            })
        })
}

/// Walk the authoritative member edges in order, including candidate-local targets.
/// As with Manim's point-bearing family order, aliases keep their last occurrence.
fn fixed_family_leaf_pairs(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    root: SemanticNodeId,
    target: SemanticTransactionNodeRef,
    allow_owned_geometry: bool,
    mut register_group: impl FnMut(SemanticNodeId) -> bool,
) -> Result<Vec<(SemanticNodeId, SemanticTransactionNodeRef)>, String> {
    let mut pending = vec![(root, target, false)];
    let mut sources = HashMap::new();
    let mut targets = HashSet::new();
    let mut leaves = Vec::new();
    while let Some((source, target, emit)) = pending.pop() {
        if emit {
            leaves.push((source, target));
            continue;
        }
        if sources.get(&source) == Some(&target) {
            continue;
        }
        if sources.insert(source, target).is_some() || !targets.insert(target) {
            return Err("alias structure differs".into());
        }
        let source_is_family = prepared.family_style(source).is_ok();
        if source_is_family != prepared.family_style(target).is_ok() {
            return Err("group and leaf positions differ".into());
        }
        let read_error = |error: noon_core::SemanticTransactionReadError| error.to_string();
        let source_members = prepared.family_members(source).map_err(read_error)?;
        let target_members = prepared.family_members(target).map_err(read_error)?;
        if source_members.len() != target_members.len() {
            return Err("member counts differ".into());
        }
        if source_is_family {
            if source != root && !register_group(source) {
                return Err("family Transforms share a source group".into());
            }
            let source_draws = prepared.object_state(source).is_ok();
            let target_draws = prepared.object_state(target).is_ok();
            if source_draws != target_draws || (source_draws && !allow_owned_geometry) {
                return Err("family geometry must match at both endpoints".into());
            }
            if source_draws {
                pending.push((source, target, true));
            }
            for (source_member, target_member) in source_members.into_iter().zip(target_members) {
                let source_member = source_member
                    .existing()
                    .ok_or("source members must be existing nodes")?;
                pending.push((source_member, target_member, false));
            }
        } else {
            prepared.object_state(source).map_err(read_error)?;
            prepared.object_state(target).map_err(read_error)?;
            if !source_members.is_empty() {
                return Err("object leaves cannot have members".into());
            }
            leaves.push((source, target));
        }
    }
    if leaves.is_empty() {
        return Err("family animation requires at least one leaf".into());
    }
    leaves.reverse();
    Ok(leaves)
}

fn existing_endpoint(
    animation: SemanticTransactionNodeRef,
    endpoint: SemanticTransactionNodeRef,
) -> Result<SemanticNodeId, PreparedFamilyTransformActivationError> {
    endpoint.existing().ok_or(
        PreparedFamilyTransformActivationError::PendingFamilyEndpoint {
            animation,
            endpoint,
        },
    )
}
