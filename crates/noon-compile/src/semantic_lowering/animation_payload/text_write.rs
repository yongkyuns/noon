use noon_core::{
    FamilyAnimationMode, FamilyAnimationSpec, ObjectContentRef, ObjectId, RateFunction,
    RetainedAnimationMemberError, RetainedAnimationMembers, RetainedFamilyAnimationMemberPlanError,
    RetainedFamilyAnimationPlan, SemanticNodeId, SemanticStore, SemanticTransactionNodeRef,
};

use super::super::{
    PreparedSemanticAnimationScheduleProjection, PreparedSemanticScheduledAnimationPayload,
    SemanticAnimationScheduleProjection, SemanticScheduledAnimationPayload,
};

/// One immutable glyph plan and its shared composition timing.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledFamilyAnimation {
    pub target: ObjectId,
    pub plan: RetainedFamilyAnimationPlan,
    pub spec: FamilyAnimationSpec,
    pub time_map: noon_core::CompositionTimeMap,
}

/// One installed family-animation driver. Its immutable glyph plan lives in the
/// compiled scene's append-only plan resource table.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledFamilyAnimationChannel {
    pub target: ObjectId,
    pub object_index: u32,
    pub plan_index: u32,
    pub spec: FamilyAnimationSpec,
    pub time_map: noon_core::CompositionTimeMap,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextWriteLoweringError {
    MissingSemanticTarget(SemanticTransactionNodeRef),
    InvalidMembers(RetainedAnimationMemberError),
    InvalidPlan(RetainedFamilyAnimationMemberPlanError),
    InvalidSpec(noon_core::FamilyAnimationError),
    InvalidTimeMap(noon_core::CompositionTimeMapError),
    ConflictingObjectDrivers {
        target: ObjectId,
        first: SemanticTransactionNodeRef,
        second: SemanticTransactionNodeRef,
    },
}

impl std::fmt::Display for TextWriteLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "TextWrite lowering failed: {self:?}")
    }
}

impl std::error::Error for TextWriteLoweringError {}

fn plan(
    store: &SemanticStore,
    semantic_target: SemanticNodeId,
    target: ObjectId,
    spec: FamilyAnimationSpec,
    time_map: noon_core::CompositionTimeMap,
) -> Result<CompiledFamilyAnimation, TextWriteLoweringError> {
    let state = store
        .semantic_object_state_checked(semantic_target)
        .map_err(|_| TextWriteLoweringError::MissingSemanticTarget(semantic_target.into()))?;
    let noon_core::SemanticObjectContent::Text(handle) = state.content else {
        return Err(TextWriteLoweringError::MissingSemanticTarget(
            semantic_target.into(),
        ));
    };
    let members =
        RetainedAnimationMembers::resolve(&ObjectContentRef::Text(handle), store.text_resources())
            .map_err(TextWriteLoweringError::InvalidMembers)?;
    let plan = RetainedFamilyAnimationPlan::single_leaf(semantic_target, target, members)
        .map_err(TextWriteLoweringError::InvalidPlan)?;
    spec.validate()
        .map_err(TextWriteLoweringError::InvalidSpec)?;
    Ok(CompiledFamilyAnimation {
        target,
        plan,
        spec,
        time_map,
    })
}

pub fn lower_semantic_text_write_animations(
    store: &SemanticStore,
    schedule: &SemanticAnimationScheduleProjection,
) -> Result<Vec<CompiledFamilyAnimation>, TextWriteLoweringError> {
    let drivers = schedule
        .leaves()
        .iter()
        .map(|leaf| {
            let interval = noon_core::continuous_time_map_interval(leaf.timing, &leaf.time_map)
                .map_err(TextWriteLoweringError::InvalidTimeMap)?;
            Ok((
                leaf.animation.into(),
                leaf.execution_object_id,
                interval,
                matches!(
                    leaf.payload,
                    SemanticScheduledAnimationPayload::TextWrite { .. }
                ),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    reject_conflicts(&drivers)?;
    schedule
        .leaves()
        .iter()
        .filter_map(|leaf| {
            let SemanticScheduledAnimationPayload::TextWrite {
                reverse_member_order,
            } = leaf.payload
            else {
                return None;
            };
            Some(
                FamilyAnimationSpec::new(
                    FamilyAnimationMode::DrawBorderThenFill,
                    leaf.timing.start_time,
                    leaf.timing.duration,
                    leaf.options.lag_ratio,
                    leaf.options.rate_func,
                    leaf.options.reverse_rate_function,
                    reverse_member_order,
                )
                .map_err(TextWriteLoweringError::InvalidSpec)
                .and_then(|spec| {
                    plan(
                        store,
                        leaf.target,
                        leaf.execution_object_id,
                        spec,
                        leaf.time_map.clone(),
                    )
                }),
            )
        })
        .collect()
}

pub fn lower_prepared_text_write_animations(
    store: &SemanticStore,
    schedule: &PreparedSemanticAnimationScheduleProjection,
) -> Result<Vec<CompiledFamilyAnimation>, TextWriteLoweringError> {
    let drivers = schedule
        .leaves()
        .iter()
        .map(|leaf| {
            let interval = noon_core::continuous_time_map_interval(leaf.timing, &leaf.time_map)
                .map_err(TextWriteLoweringError::InvalidTimeMap)?;
            Ok((
                leaf.animation,
                leaf.execution_object_id,
                interval,
                matches!(
                    leaf.payload,
                    PreparedSemanticScheduledAnimationPayload::TextWrite { .. }
                ),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    reject_conflicts(&drivers)?;
    schedule
        .leaves()
        .iter()
        .filter_map(|leaf| {
            let PreparedSemanticScheduledAnimationPayload::TextWrite {
                reverse_member_order,
            } = leaf.payload
            else {
                return None;
            };
            let Some(target) = leaf.target.existing() else {
                return Some(Err(TextWriteLoweringError::MissingSemanticTarget(
                    leaf.target,
                )));
            };
            Some(
                FamilyAnimationSpec::new(
                    FamilyAnimationMode::DrawBorderThenFill,
                    leaf.timing.start_time,
                    leaf.timing.duration,
                    leaf.options.lag_ratio,
                    leaf.options.rate_func,
                    leaf.options.reverse_rate_function,
                    reverse_member_order,
                )
                .map_err(TextWriteLoweringError::InvalidSpec)
                .and_then(|spec| {
                    plan(
                        store,
                        target,
                        leaf.execution_object_id,
                        spec,
                        leaf.time_map.clone(),
                    )
                }),
            )
        })
        .collect()
}

fn reject_conflicts(
    drivers: &[(SemanticTransactionNodeRef, ObjectId, (f64, f64), bool)],
) -> Result<(), TextWriteLoweringError> {
    for left_index in 0..drivers.len() {
        let (first, target, (start, end), first_is_text_write) = drivers[left_index];
        for &(second, other_target, (other_start, other_end), second_is_text_write) in
            &drivers[left_index + 1..]
        {
            if target == other_target
                && (first_is_text_write || second_is_text_write)
                && start < other_end
                && other_start < end
            {
                return Err(TextWriteLoweringError::ConflictingObjectDrivers {
                    target,
                    first,
                    second,
                });
            }
        }
    }
    Ok(())
}
