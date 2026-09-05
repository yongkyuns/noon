use noon_core::{
    FamilyAnimationState, RetainedFamilyAnimationLeafFrame, RetainedFamilyAnimationPlan,
};

use crate::{FrameState, RetainedFamilyFrame, RetainedFamilyFramePlanError};

/// Retained family frame with explicit immutable-plan ownership per active object.
///
/// Multiple family requests may reference the same retained object at different times.
/// The evaluated family state alone therefore cannot identify which immutable member
/// plan owns that state. `family_plan_indices` carries the snapshot-installed plan
/// index selected by the runtime for each object. Renderer code never guesses plan
/// identity from object membership.
#[derive(Clone, Copy, Debug)]
pub struct RetainedPlannedFamilyFrame<'a> {
    pub retained: &'a FrameState,
    pub family_animations: &'a [Option<FamilyAnimationState>],
    pub family_plan_indices: &'a [Option<u32>],
}

impl RetainedPlannedFamilyFrame<'_> {
    pub fn family_animation(&self, object_index: usize) -> Option<FamilyAnimationState> {
        self.family_animations.get(object_index).copied().flatten()
    }

    pub fn family_plan_index(&self, object_index: usize) -> Option<u32> {
        self.family_plan_indices
            .get(object_index)
            .copied()
            .flatten()
    }

    pub fn as_family_frame(&self) -> RetainedFamilyFrame<'_> {
        RetainedFamilyFrame {
            retained: self.retained,
            family_animations: self.family_animations,
        }
    }

    pub fn planned_family_leaf<'plan>(
        &self,
        plans: &'plan [RetainedFamilyAnimationPlan],
        object_index: usize,
    ) -> Result<Option<RetainedFamilyAnimationLeafFrame<'plan>>, RetainedPlannedFamilyFrameError>
    {
        if self.family_animations.len() != self.retained.objects.len()
            || self.family_plan_indices.len() != self.retained.objects.len()
        {
            return Err(RetainedPlannedFamilyFrameError::FrameShapeMismatch);
        }
        let Some(state) = self.family_animation(object_index) else {
            return Ok(None);
        };
        let object = self.retained.objects.get(object_index).ok_or(
            RetainedPlannedFamilyFrameError::InvalidObjectIndex(object_index),
        )?;
        let plan_index = self
            .family_plan_index(object_index)
            .ok_or(RetainedPlannedFamilyFrameError::MissingPlanIndex(object.id))?;
        let plan = plans.get(plan_index as usize).ok_or(
            RetainedPlannedFamilyFrameError::InvalidPlanIndex {
                object: object.id,
                plan_index,
                plan_count: plans.len(),
            },
        )?;
        if plan.leaf_for_object(object.id).is_none() {
            return Err(RetainedPlannedFamilyFrameError::PlanDoesNotOwnObject {
                object: object.id,
                plan_index,
            });
        }
        plan.leaf_frame_for_object(state, object.id)
            .map(Some)
            .map_err(|error| {
                RetainedPlannedFamilyFrameError::Plan(RetainedFamilyFramePlanError::Evaluation(
                    error,
                ))
            })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedPlannedFamilyFrameError {
    FrameShapeMismatch,
    InvalidObjectIndex(usize),
    MissingPlanIndex(noon_core::ObjectId),
    InvalidPlanIndex {
        object: noon_core::ObjectId,
        plan_index: u32,
        plan_count: usize,
    },
    PlanDoesNotOwnObject {
        object: noon_core::ObjectId,
        plan_index: u32,
    },
    Plan(RetainedFamilyFramePlanError),
}

impl std::fmt::Display for RetainedPlannedFamilyFrameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FrameShapeMismatch => formatter.write_str(
                "retained planned-family state shape does not match retained frame objects",
            ),
            Self::InvalidObjectIndex(index) => {
                write!(formatter, "invalid retained planned-family object index {index}")
            }
            Self::MissingPlanIndex(object) => write!(
                formatter,
                "active retained family state for object {} has no plan index",
                object.get()
            ),
            Self::InvalidPlanIndex {
                object,
                plan_index,
                plan_count,
            } => write!(
                formatter,
                "retained family object {} references plan index {plan_index}, but only {plan_count} plans are installed",
                object.get()
            ),
            Self::PlanDoesNotOwnObject { object, plan_index } => write!(
                formatter,
                "retained family plan {plan_index} does not own object {}",
                object.get()
            ),
            Self::Plan(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedPlannedFamilyFrameError {}

impl From<RetainedFamilyFramePlanError> for RetainedPlannedFamilyFrameError {
    fn from(value: RetainedFamilyFramePlanError) -> Self {
        Self::Plan(value)
    }
}
