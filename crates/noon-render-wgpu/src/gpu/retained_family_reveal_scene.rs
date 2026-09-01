use noon_core::{
    FamilyAnimationState, ObjectId, RetainedFamilyAnimationEvaluationError,
    RetainedFamilyAnimationPlan,
};

use super::{
    retained_family_reveal_members, RetainedFamilyRevealError, RetainedFamilyRevealMembers,
};

/// One active family animation paired with its immutable prepared retained-content plan.
///
/// The plan owns semantic leaf/member binding while the state owns only this frame's
/// timing position. Keeping them separate lets render preparation borrow both without
/// rebuilding member metadata or moving family timing into the renderer.
#[derive(Clone, Copy, Debug)]
pub struct ActiveRetainedFamilyReveal<'a> {
    pub plan: &'a RetainedFamilyAnimationPlan,
    pub state: FamilyAnimationState,
}

impl<'a> ActiveRetainedFamilyReveal<'a> {
    pub const fn new(plan: &'a RetainedFamilyAnimationPlan, state: FamilyAnimationState) -> Self {
        Self { plan, state }
    }
}

/// Borrowed frame-level view of all concurrently active retained family reveals.
///
/// Lookup is allocation-free and deliberately object-addressable because
/// `RetainedFramePreparer` already walks retained objects in painter order. Semantic
/// family traversal and member flattening happened when each immutable plan was built.
/// If two active plans claim the same retained leaf, this view fails closed rather than
/// making renderer order decide which animation wins.
#[derive(Clone, Copy, Debug)]
pub struct RetainedFamilyRevealScene<'a> {
    active: &'a [ActiveRetainedFamilyReveal<'a>],
}

impl<'a> RetainedFamilyRevealScene<'a> {
    pub const fn new(active: &'a [ActiveRetainedFamilyReveal<'a>]) -> Self {
        Self { active }
    }

    pub const fn active(&self) -> &'a [ActiveRetainedFamilyReveal<'a>] {
        self.active
    }

    /// Resolve the already-prepared reveal members for one retained object.
    ///
    /// `None` means no active family animation owns this object. A returned iterator
    /// contains only realization commands; global lag/easing/reversal was evaluated by
    /// the shared family plan before this renderer boundary.
    pub fn members_for_object(
        &self,
        object: ObjectId,
    ) -> Result<Option<RetainedFamilyRevealMembers<'a>>, RetainedFamilyRevealSceneError> {
        let mut matched = None;
        for active in self.active {
            if active.plan.leaf_for_object(object).is_none() {
                continue;
            }
            if matched.is_some() {
                return Err(RetainedFamilyRevealSceneError::OverlappingObject {
                    object,
                });
            }
            matched = Some(*active);
        }

        let Some(active) = matched else {
            return Ok(None);
        };
        let frame = active
            .plan
            .leaf_frame_for_object(active.state, object)
            .map_err(RetainedFamilyRevealSceneError::Evaluation)?;
        Ok(Some(
            retained_family_reveal_members(frame).map_err(RetainedFamilyRevealSceneError::Reveal)?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetainedFamilyRevealSceneError {
    OverlappingObject { object: ObjectId },
    Evaluation(RetainedFamilyAnimationEvaluationError),
    Reveal(RetainedFamilyRevealError),
}

impl std::fmt::Display for RetainedFamilyRevealSceneError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OverlappingObject { object } => write!(
                formatter,
                "multiple active family animations target retained object {}",
                object.get()
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
    fn absent_object_has_no_family_realization() {
        let plan = geometry_plan(ObjectId::new(10));
        let active = [ActiveRetainedFamilyReveal::new(
            &plan,
            state(FamilyAnimationMode::Reveal, 0.5),
        )];
        let scene = RetainedFamilyRevealScene::new(&active);
        assert!(scene.members_for_object(ObjectId::new(99)).unwrap().is_none());
    }

    #[test]
    fn distinct_active_plans_resolve_by_retained_object() {
        let first = geometry_plan(ObjectId::new(10));
        let second = geometry_plan(ObjectId::new(20));
        let active = [
            ActiveRetainedFamilyReveal::new(
                &first,
                state(FamilyAnimationMode::Reveal, 0.25),
            ),
            ActiveRetainedFamilyReveal::new(
                &second,
                state(FamilyAnimationMode::Reveal, 0.75),
            ),
        ];
        let scene = RetainedFamilyRevealScene::new(&active);

        let first_members = scene
            .members_for_object(ObjectId::new(10))
            .unwrap()
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let second_members = scene
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
    fn overlapping_active_plans_fail_closed_before_realization() {
        let first = geometry_plan(ObjectId::new(10));
        let second = geometry_plan(ObjectId::new(10));
        let active = [
            ActiveRetainedFamilyReveal::new(
                &first,
                state(FamilyAnimationMode::Reveal, 0.25),
            ),
            ActiveRetainedFamilyReveal::new(
                &second,
                state(FamilyAnimationMode::Reveal, 0.75),
            ),
        ];
        let scene = RetainedFamilyRevealScene::new(&active);
        assert_eq!(
            scene.members_for_object(ObjectId::new(10)).unwrap_err(),
            RetainedFamilyRevealSceneError::OverlappingObject {
                object: ObjectId::new(10),
            }
        );
    }

    #[test]
    fn unsupported_operation_is_not_silently_treated_as_reveal() {
        let plan = geometry_plan(ObjectId::new(10));
        let active = [ActiveRetainedFamilyReveal::new(
            &plan,
            state(FamilyAnimationMode::DrawBorderThenFill, 0.5),
        )];
        let scene = RetainedFamilyRevealScene::new(&active);
        assert!(matches!(
            scene.members_for_object(ObjectId::new(10)),
            Err(RetainedFamilyRevealSceneError::Reveal(
                RetainedFamilyRevealError::UnsupportedMode(
                    FamilyAnimationMode::DrawBorderThenFill
                )
            ))
        ));
    }
}
