use std::collections::HashMap;

use noon_core::{
    FamilyAnimationState, ObjectId, RetainedFamilyAnimationEvaluationError,
    RetainedFamilyAnimationPlan,
};

use super::{
    retained_family_reveal_members, RetainedFamilyRevealError, RetainedFamilyRevealMembers,
};

/// Prepared scene-level binding for a set of concurrently active family animations.
///
/// This index is rebuilt only when the active animation set changes, not every frame.
/// It maps each retained leaf to exactly one prepared family plan so the mixed retained
/// renderer can resolve family realization in O(1) while it already walks objects in
/// painter order. Family timing state intentionally stays out of this immutable index.
#[derive(Clone, Debug)]
pub struct RetainedFamilyRevealScenePlan<'a> {
    plans: Vec<&'a RetainedFamilyAnimationPlan>,
    plan_by_object: HashMap<ObjectId, usize>,
}

impl<'a> RetainedFamilyRevealScenePlan<'a> {
    pub fn new(
        plans: impl IntoIterator<Item = &'a RetainedFamilyAnimationPlan>,
    ) -> Result<Self, RetainedFamilyRevealSceneError> {
        let mut retained_plans = Vec::new();
        let mut plan_by_object = HashMap::new();
        for plan in plans {
            let plan_index = retained_plans.len();
            for leaf in plan.leaves() {
                let object = leaf.span().object;
                if let Some(&first_animation) = plan_by_object.get(&object) {
                    return Err(RetainedFamilyRevealSceneError::OverlappingObject {
                        object,
                        first_animation,
                        second_animation: plan_index,
                    });
                }
                plan_by_object.insert(object, plan_index);
            }
            retained_plans.push(plan);
        }
        Ok(Self {
            plans: retained_plans,
            plan_by_object,
        })
    }

    pub fn animation_count(&self) -> usize {
        self.plans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plans.is_empty()
    }

    /// Borrow this immutable binding with the timing states for one frame.
    ///
    /// The state slice is parallel to the plan order supplied to [`Self::new`]. No map
    /// or member metadata is rebuilt as progress advances.
    pub fn frame<'plan, 'state>(
        &'plan self,
        states: &'state [FamilyAnimationState],
    ) -> Result<RetainedFamilyRevealSceneFrame<'plan, 'state, 'a>, RetainedFamilyRevealSceneError>
    {
        if states.len() != self.plans.len() {
            return Err(RetainedFamilyRevealSceneError::StateCountMismatch {
                expected: self.plans.len(),
                actual: states.len(),
            });
        }
        Ok(RetainedFamilyRevealSceneFrame { plan: self, states })
    }
}

/// Allocation-free frame view over an immutable active-family scene plan.
#[derive(Clone, Copy, Debug)]
pub struct RetainedFamilyRevealSceneFrame<'plan, 'state, 'content> {
    plan: &'plan RetainedFamilyRevealScenePlan<'content>,
    states: &'state [FamilyAnimationState],
}

impl<'content> RetainedFamilyRevealSceneFrame<'_, '_, 'content> {
    /// Resolve already-prepared realization commands for one retained object.
    ///
    /// `None` means the object is outside every active family animation. A returned
    /// iterator performs only leaf-local realization; global lag/easing/reversal was
    /// evaluated by the shared family plan before this renderer boundary.
    pub fn members_for_object(
        &self,
        object: ObjectId,
    ) -> Result<Option<RetainedFamilyRevealMembers<'content>>, RetainedFamilyRevealSceneError> {
        let Some(&plan_index) = self.plan.plan_by_object.get(&object) else {
            return Ok(None);
        };
        let plan = self.plan.plans[plan_index];
        let state = self.states[plan_index];
        let frame = plan
            .leaf_frame_for_object(state, object)
            .map_err(RetainedFamilyRevealSceneError::Evaluation)?;
        Ok(Some(
            retained_family_reveal_members(frame).map_err(RetainedFamilyRevealSceneError::Reveal)?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetainedFamilyRevealSceneError {
    OverlappingObject {
        object: ObjectId,
        first_animation: usize,
        second_animation: usize,
    },
    StateCountMismatch {
        expected: usize,
        actual: usize,
    },
    Evaluation(RetainedFamilyAnimationEvaluationError),
    Reveal(RetainedFamilyRevealError),
}

impl std::fmt::Display for RetainedFamilyRevealSceneError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OverlappingObject {
                object,
                first_animation,
                second_animation,
            } => write!(
                formatter,
                "family animations {first_animation} and {second_animation} both target retained object {}",
                object.get()
            ),
            Self::StateCountMismatch { expected, actual } => write!(
                formatter,
                "retained family reveal scene expects {expected} animation states, got {actual}"
            ),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::Reveal(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedFamilyRevealSceneError {}

#[cfg(test)]
mod tests {
    use noon_core::{
        FamilyAnimationMode, GeometryRef, RateFunction, RetainedFamilyAnimationPlanBuilder,
        RetainedObjectDefinition, SemanticStore, TextResourceArena,
    };

    use super::*;
    use crate::RetainedFamilyRevealMember;

    fn state(mode: FamilyAnimationMode, progress: f64) -> FamilyAnimationState {
        FamilyAnimationState {
            mode,
            overall_progress: progress,
            lag_ratio: 0.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        }
    }

    fn geometry_plan(object: ObjectId) -> RetainedFamilyAnimationPlan {
        let mut store = SemanticStore::new();
        let leaf = store.insert_authoring_object();
        let family = store.insert_family();
        store.add_member(family, leaf).unwrap();

        let object = RetainedObjectDefinition::geometry(object, GeometryRef::circle(1.0));
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&store, family).unwrap();
        builder
            .accept_leaf(leaf, &object, &TextResourceArena::new())
            .unwrap();
        builder.finish().unwrap()
    }

    #[test]
    fn frame_lookup_is_object_addressable_across_distinct_plans() {
        let first = geometry_plan(ObjectId::new(10));
        let second = geometry_plan(ObjectId::new(20));
        let scene = RetainedFamilyRevealScenePlan::new([&first, &second]).unwrap();
        let states = [
            state(FamilyAnimationMode::Reveal, 0.25),
            state(FamilyAnimationMode::Reveal, 0.75),
        ];
        let frame = scene.frame(&states).unwrap();

        assert!(frame.members_for_object(ObjectId::new(99)).unwrap().is_none());
        let first_members = frame
            .members_for_object(ObjectId::new(10))
            .unwrap()
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let second_members = frame
            .members_for_object(ObjectId::new(20))
            .unwrap()
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            first_members,
            vec![RetainedFamilyRevealMember::Geometry {
                object: ObjectId::new(10),
                reveal: 0.25,
            }]
        );
        assert_eq!(
            second_members,
            vec![RetainedFamilyRevealMember::Geometry {
                object: ObjectId::new(20),
                reveal: 0.75,
            }]
        );
    }

    #[test]
    fn overlapping_plan_ownership_fails_when_active_set_is_built() {
        let first = geometry_plan(ObjectId::new(10));
        let second = geometry_plan(ObjectId::new(10));
        assert_eq!(
            RetainedFamilyRevealScenePlan::new([&first, &second]).unwrap_err(),
            RetainedFamilyRevealSceneError::OverlappingObject {
                object: ObjectId::new(10),
                first_animation: 0,
                second_animation: 1,
            }
        );
    }

    #[test]
    fn frame_requires_one_state_per_prepared_plan() {
        let first = geometry_plan(ObjectId::new(10));
        let second = geometry_plan(ObjectId::new(20));
        let scene = RetainedFamilyRevealScenePlan::new([&first, &second]).unwrap();
        assert_eq!(
            scene
                .frame(&[state(FamilyAnimationMode::Reveal, 0.5)])
                .unwrap_err(),
            RetainedFamilyRevealSceneError::StateCountMismatch {
                expected: 2,
                actual: 1,
            }
        );
    }

    #[test]
    fn unsupported_operation_is_not_silently_treated_as_reveal() {
        let plan = geometry_plan(ObjectId::new(10));
        let scene = RetainedFamilyRevealScenePlan::new([&plan]).unwrap();
        let states = [state(FamilyAnimationMode::DrawBorderThenFill, 0.5)];
        let frame = scene.frame(&states).unwrap();
        assert!(matches!(
            frame.members_for_object(ObjectId::new(10)),
            Err(RetainedFamilyRevealSceneError::Reveal(
                RetainedFamilyRevealError::UnsupportedMode(
                    FamilyAnimationMode::DrawBorderThenFill
                )
            ))
        ));
    }
}
