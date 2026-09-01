use std::collections::HashSet;

use crate::{
    FamilyAnimationError, FamilyAnimationState, ObjectId, SemanticNodeId, SemanticStore,
    SemanticStoreError,
};

/// Global animation-member range owned by one semantic leaf/runtime object.
///
/// Heavy content identity stays in the retained resource. The plan only records the
/// stable semantic/runtime binding and the leaf's range within one globally ordered
/// Manim-visible animation-member sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyAnimationLeafSpan {
    pub semantic_leaf: SemanticNodeId,
    pub object: ObjectId,
    pub first_member: u32,
    pub member_count: u32,
}

impl FamilyAnimationLeafSpan {
    pub fn global_member_index(self, local_member: u32) -> Option<u32> {
        (local_member < self.member_count).then(|| self.first_member + local_member)
    }
}

/// Evaluated family-animation view for one semantic leaf.
///
/// The leaf keeps its local member indexing while every progress lookup is resolved
/// through the plan's global member count/index. Lag and member-order reversal therefore
/// continue across leaf/content boundaries instead of restarting for each object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FamilyAnimationLeafProgress {
    span: FamilyAnimationLeafSpan,
    total_member_count: u32,
    state: FamilyAnimationState,
}

impl FamilyAnimationLeafProgress {
    pub const fn span(self) -> FamilyAnimationLeafSpan {
        self.span
    }

    pub const fn total_member_count(self) -> u32 {
        self.total_member_count
    }

    pub fn member_progress(
        self,
        local_member: u32,
    ) -> Result<f32, FamilyAnimationMemberEvaluationError> {
        let global_member = self.span.global_member_index(local_member).ok_or(
            FamilyAnimationMemberEvaluationError::InvalidLocalMember {
                leaf: self.span.semantic_leaf,
                index: local_member,
                member_count: self.span.member_count,
            },
        )?;
        Ok(self
            .state
            .member_progress(global_member, self.total_member_count)?)
    }
}

/// Immutable binding between authoritative semantic leaves and one global member sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyAnimationMemberPlan {
    target: SemanticNodeId,
    leaves: Vec<FamilyAnimationLeafSpan>,
    total_member_count: u32,
}

impl FamilyAnimationMemberPlan {
    pub const fn target(&self) -> SemanticNodeId {
        self.target
    }

    pub fn leaves(&self) -> &[FamilyAnimationLeafSpan] {
        &self.leaves
    }

    pub const fn total_member_count(&self) -> u32 {
        self.total_member_count
    }

    pub fn span_for_leaf(&self, leaf: SemanticNodeId) -> Option<FamilyAnimationLeafSpan> {
        self.leaves
            .iter()
            .copied()
            .find(|span| span.semantic_leaf == leaf)
    }

    pub fn leaf_progress(
        &self,
        state: FamilyAnimationState,
        leaf: SemanticNodeId,
    ) -> Result<FamilyAnimationLeafProgress, FamilyAnimationMemberEvaluationError> {
        state.validate()?;
        let span = self
            .span_for_leaf(leaf)
            .ok_or(FamilyAnimationMemberEvaluationError::UnknownLeaf(leaf))?;
        Ok(FamilyAnimationLeafProgress {
            span,
            total_member_count: self.total_member_count,
            state,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FamilyAnimationMemberEvaluationError {
    UnknownLeaf(SemanticNodeId),
    InvalidLocalMember {
        leaf: SemanticNodeId,
        index: u32,
        member_count: u32,
    },
    Animation(FamilyAnimationError),
}

impl std::fmt::Display for FamilyAnimationMemberEvaluationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownLeaf(leaf) => {
                write!(formatter, "family animation plan has no semantic leaf {leaf:?}")
            }
            Self::InvalidLocalMember {
                leaf,
                index,
                member_count,
            } => write!(
                formatter,
                "family animation local member {index} is outside leaf {leaf:?} member count {member_count}"
            ),
            Self::Animation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for FamilyAnimationMemberEvaluationError {}

impl From<FamilyAnimationError> for FamilyAnimationMemberEvaluationError {
    fn from(value: FamilyAnimationError) -> Self {
        Self::Animation(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FamilyAnimationMemberPlanError {
    Semantic(SemanticStoreError),
    DuplicateSemanticLeaf(SemanticNodeId),
    UnexpectedLeaf {
        index: usize,
        expected: SemanticNodeId,
        actual: SemanticNodeId,
    },
    DuplicateObject(ObjectId),
    TooManyLeaves {
        expected: usize,
    },
    Incomplete {
        accepted: usize,
        expected: usize,
    },
    MemberCountOverflow,
}

impl std::fmt::Display for FamilyAnimationMemberPlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Semantic(error) => error.fmt(formatter),
            Self::DuplicateSemanticLeaf(leaf) => write!(
                formatter,
                "family animation member plan repeats semantic leaf {}:{}",
                leaf.slot(),
                leaf.generation()
            ),
            Self::UnexpectedLeaf {
                index,
                expected,
                actual,
            } => write!(
                formatter,
                "family animation leaf mismatch at index {index}: expected {expected:?}, got {actual:?}"
            ),
            Self::DuplicateObject(object) => write!(
                formatter,
                "family animation member plan maps retained object {} more than once",
                object.get()
            ),
            Self::TooManyLeaves { expected } => write!(
                formatter,
                "family animation member plan received more than {expected} semantic leaves"
            ),
            Self::Incomplete { accepted, expected } => write!(
                formatter,
                "family animation member plan is incomplete: accepted {accepted} of {expected} semantic leaves"
            ),
            Self::MemberCountOverflow => formatter.write_str(
                "family animation global member count exceeds the retained u32 member range",
            ),
        }
    }
}

impl std::error::Error for FamilyAnimationMemberPlanError {}

impl From<SemanticStoreError> for FamilyAnimationMemberPlanError {
    fn from(value: SemanticStoreError) -> Self {
        Self::Semantic(value)
    }
}

/// Transactional builder for a global animation-member plan.
///
/// Shared semantic state snapshots leaf identity/order. The caller supplies only the
/// runtime `ObjectId` and content-local member cardinality for each expected leaf.
/// This keeps content/resource inspection outside the family scheduler while making
/// it impossible for a frontend adapter to silently reorder semantic family members.
#[derive(Clone, Debug)]
pub struct FamilyAnimationMemberPlanBuilder {
    target: SemanticNodeId,
    expected_leaves: Vec<SemanticNodeId>,
    next_leaf: usize,
    next_member: u32,
    used_objects: HashSet<ObjectId>,
    spans: Vec<FamilyAnimationLeafSpan>,
}

impl FamilyAnimationMemberPlanBuilder {
    pub fn begin(
        store: &SemanticStore,
        target: SemanticNodeId,
    ) -> Result<Self, FamilyAnimationMemberPlanError> {
        Self::begin_ordered(target, store.ordered_leaf_nodes(target)?)
    }

    /// Begin from an already-snapshotted authoritative semantic leaf sequence.
    ///
    /// This is the execution-side counterpart to a serialized family-animation request:
    /// semantic identity/order survive the boundary, while content-local member counts
    /// are still resolved only from execution-local retained resources.
    pub fn begin_ordered(
        target: SemanticNodeId,
        expected_leaves: Vec<SemanticNodeId>,
    ) -> Result<Self, FamilyAnimationMemberPlanError> {
        let mut seen = HashSet::with_capacity(expected_leaves.len());
        for leaf in &expected_leaves {
            if !seen.insert(*leaf) {
                return Err(FamilyAnimationMemberPlanError::DuplicateSemanticLeaf(*leaf));
            }
        }
        Ok(Self {
            target,
            spans: Vec::with_capacity(expected_leaves.len()),
            expected_leaves,
            next_leaf: 0,
            next_member: 0,
            used_objects: HashSet::new(),
        })
    }

    pub fn expected_leaf_count(&self) -> usize {
        self.expected_leaves.len()
    }

    pub fn accept_leaf(
        &mut self,
        semantic_leaf: SemanticNodeId,
        object: ObjectId,
        local_member_count: u32,
    ) -> Result<(), FamilyAnimationMemberPlanError> {
        let Some(expected) = self.expected_leaves.get(self.next_leaf).copied() else {
            return Err(FamilyAnimationMemberPlanError::TooManyLeaves {
                expected: self.expected_leaves.len(),
            });
        };
        if semantic_leaf != expected {
            return Err(FamilyAnimationMemberPlanError::UnexpectedLeaf {
                index: self.next_leaf,
                expected,
                actual: semantic_leaf,
            });
        }
        if self.used_objects.contains(&object) {
            return Err(FamilyAnimationMemberPlanError::DuplicateObject(object));
        }

        let next_member = self
            .next_member
            .checked_add(local_member_count)
            .ok_or(FamilyAnimationMemberPlanError::MemberCountOverflow)?;
        self.spans.push(FamilyAnimationLeafSpan {
            semantic_leaf,
            object,
            first_member: self.next_member,
            member_count: local_member_count,
        });
        self.used_objects.insert(object);
        self.next_member = next_member;
        self.next_leaf += 1;
        Ok(())
    }

    pub fn finish(self) -> Result<FamilyAnimationMemberPlan, FamilyAnimationMemberPlanError> {
        if self.next_leaf != self.expected_leaves.len() {
            return Err(FamilyAnimationMemberPlanError::Incomplete {
                accepted: self.next_leaf,
                expected: self.expected_leaves.len(),
            });
        }
        Ok(FamilyAnimationMemberPlan {
            target: self.target,
            leaves: self.spans,
            total_member_count: self.next_member,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{FamilyAnimationMode, RateFunction, SemanticStore};

    use super::*;

    fn nested_family() -> (
        SemanticStore,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
    ) {
        let mut store = SemanticStore::new();
        let text = store.insert_authoring_object();
        let circle = store.insert_authoring_object();
        let empty = store.insert_authoring_object();
        let nested = store.insert_family();
        let root = store.insert_family();
        store.add_member(nested, circle).unwrap();
        store.add_member(nested, empty).unwrap();
        store.add_member(root, text).unwrap();
        store.add_member(root, nested).unwrap();
        (store, root, text, circle, empty)
    }

    fn mixed_plan() -> (
        FamilyAnimationMemberPlan,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
    ) {
        let (store, root, text, circle, empty) = nested_family();
        let mut builder = FamilyAnimationMemberPlanBuilder::begin(&store, root).unwrap();
        builder.accept_leaf(text, ObjectId::new(10), 2).unwrap();
        builder.accept_leaf(circle, ObjectId::new(11), 1).unwrap();
        builder.accept_leaf(empty, ObjectId::new(12), 0).unwrap();
        (builder.finish().unwrap(), text, circle, empty)
    }

    fn animation_state(reverse_member_order: bool) -> FamilyAnimationState {
        FamilyAnimationState {
            mode: FamilyAnimationMode::Reveal,
            overall_progress: 0.5,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order,
        }
    }

    #[test]
    fn mixed_leaf_cardinalities_form_one_global_member_sequence() {
        let (plan, text, _, _) = mixed_plan();

        assert_eq!(plan.total_member_count(), 3);
        assert_eq!(plan.leaves()[0].global_member_index(0), Some(0));
        assert_eq!(plan.leaves()[0].global_member_index(1), Some(1));
        assert_eq!(plan.leaves()[0].global_member_index(2), None);
        assert_eq!(plan.leaves()[1].global_member_index(0), Some(2));
        assert_eq!(plan.span_for_leaf(text), Some(plan.leaves()[0]));
    }

    #[test]
    fn lag_progress_continues_across_leaf_boundaries() {
        let (plan, text, circle, _) = mixed_plan();
        let text_progress = plan.leaf_progress(animation_state(false), text).unwrap();
        let circle_progress = plan.leaf_progress(animation_state(false), circle).unwrap();

        assert_eq!(text_progress.total_member_count(), 3);
        assert_eq!(text_progress.member_progress(0).unwrap(), 1.0);
        assert_eq!(text_progress.member_progress(1).unwrap(), 0.5);
        assert_eq!(circle_progress.member_progress(0).unwrap(), 0.0);
    }

    #[test]
    fn member_order_reversal_is_global_not_leaf_local() {
        let (plan, text, circle, _) = mixed_plan();
        let normal_text = plan.leaf_progress(animation_state(false), text).unwrap();
        let normal_circle = plan.leaf_progress(animation_state(false), circle).unwrap();
        let reversed_text = plan.leaf_progress(animation_state(true), text).unwrap();
        let reversed_circle = plan.leaf_progress(animation_state(true), circle).unwrap();

        assert_eq!(
            reversed_text.member_progress(0).unwrap(),
            normal_circle.member_progress(0).unwrap()
        );
        assert_eq!(
            reversed_text.member_progress(1).unwrap(),
            normal_text.member_progress(1).unwrap()
        );
        assert_eq!(
            reversed_circle.member_progress(0).unwrap(),
            normal_text.member_progress(0).unwrap()
        );
    }

    #[test]
    fn invalid_leaf_local_member_and_state_fail_closed() {
        let (plan, text, _, empty) = mixed_plan();
        let empty_progress = plan.leaf_progress(animation_state(false), empty).unwrap();
        assert_eq!(
            empty_progress.member_progress(0),
            Err(FamilyAnimationMemberEvaluationError::InvalidLocalMember {
                leaf: empty,
                index: 0,
                member_count: 0,
            })
        );

        let unknown = SemanticNodeId::new(u32::MAX, 0);
        assert_eq!(
            plan.leaf_progress(animation_state(false), unknown),
            Err(FamilyAnimationMemberEvaluationError::UnknownLeaf(unknown))
        );

        let mut invalid = animation_state(false);
        invalid.overall_progress = 1.5;
        assert_eq!(
            plan.leaf_progress(invalid, text),
            Err(FamilyAnimationMemberEvaluationError::Animation(
                FamilyAnimationError::InvalidOverallProgress(1.5)
            ))
        );
    }

    #[test]
    fn leaf_target_builds_a_single_span_without_family_special_case() {
        let mut store = SemanticStore::new();
        let leaf = store.insert_authoring_object();
        let mut builder = FamilyAnimationMemberPlanBuilder::begin(&store, leaf).unwrap();
        builder.accept_leaf(leaf, ObjectId::new(7), 4).unwrap();
        let plan = builder.finish().unwrap();
        assert_eq!(plan.target(), leaf);
        assert_eq!(plan.total_member_count(), 4);
        assert_eq!(plan.leaves().len(), 1);
    }

    #[test]
    fn ordered_constructor_preserves_semantic_identity_and_rejects_duplicates() {
        let target = SemanticNodeId::new(99, 7);
        let first = SemanticNodeId::new(41, 3);
        let second = SemanticNodeId::new(12, 8);
        let mut builder =
            FamilyAnimationMemberPlanBuilder::begin_ordered(target, vec![first, second]).unwrap();
        builder.accept_leaf(first, ObjectId::new(10), 2).unwrap();
        builder.accept_leaf(second, ObjectId::new(11), 1).unwrap();
        let plan = builder.finish().unwrap();
        assert_eq!(plan.target(), target);
        assert_eq!(plan.leaves()[0].semantic_leaf, first);
        assert_eq!(plan.leaves()[1].semantic_leaf, second);

        assert_eq!(
            FamilyAnimationMemberPlanBuilder::begin_ordered(target, vec![first, first])
                .unwrap_err(),
            FamilyAnimationMemberPlanError::DuplicateSemanticLeaf(first)
        );
    }

    #[test]
    fn frontend_cannot_reorder_semantic_leaves() {
        let (store, root, text, circle, _) = nested_family();
        let mut builder = FamilyAnimationMemberPlanBuilder::begin(&store, root).unwrap();
        let error = builder
            .accept_leaf(circle, ObjectId::new(11), 1)
            .unwrap_err();
        assert_eq!(
            error,
            FamilyAnimationMemberPlanError::UnexpectedLeaf {
                index: 0,
                expected: text,
                actual: circle,
            }
        );

        builder.accept_leaf(text, ObjectId::new(10), 2).unwrap();
    }

    #[test]
    fn duplicate_runtime_object_binding_is_rejected_without_consuming_leaf() {
        let (store, root, text, circle, empty) = nested_family();
        let mut builder = FamilyAnimationMemberPlanBuilder::begin(&store, root).unwrap();
        builder.accept_leaf(text, ObjectId::new(10), 2).unwrap();
        assert_eq!(
            builder.accept_leaf(circle, ObjectId::new(10), 1),
            Err(FamilyAnimationMemberPlanError::DuplicateObject(
                ObjectId::new(10)
            ))
        );
        builder.accept_leaf(circle, ObjectId::new(11), 1).unwrap();
        builder.accept_leaf(empty, ObjectId::new(12), 0).unwrap();
        assert_eq!(builder.finish().unwrap().total_member_count(), 3);
    }

    #[test]
    fn incomplete_and_extra_leaf_submission_fail_closed() {
        let (store, root, text, circle, empty) = nested_family();
        let mut incomplete = FamilyAnimationMemberPlanBuilder::begin(&store, root).unwrap();
        incomplete.accept_leaf(text, ObjectId::new(10), 2).unwrap();
        assert_eq!(
            incomplete.finish().unwrap_err(),
            FamilyAnimationMemberPlanError::Incomplete {
                accepted: 1,
                expected: 3,
            }
        );

        let mut complete = FamilyAnimationMemberPlanBuilder::begin(&store, root).unwrap();
        complete.accept_leaf(text, ObjectId::new(10), 2).unwrap();
        complete.accept_leaf(circle, ObjectId::new(11), 1).unwrap();
        complete.accept_leaf(empty, ObjectId::new(12), 0).unwrap();
        assert_eq!(
            complete
                .accept_leaf(empty, ObjectId::new(13), 1)
                .unwrap_err(),
            FamilyAnimationMemberPlanError::TooManyLeaves { expected: 3 }
        );
    }

    #[test]
    fn global_member_count_overflow_is_rejected_without_advancing_builder() {
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let root = store.insert_family();
        store.add_member(root, first).unwrap();
        store.add_member(root, second).unwrap();

        let mut builder = FamilyAnimationMemberPlanBuilder::begin(&store, root).unwrap();
        builder
            .accept_leaf(first, ObjectId::new(1), u32::MAX)
            .unwrap();
        assert_eq!(
            builder
                .accept_leaf(second, ObjectId::new(2), 1)
                .unwrap_err(),
            FamilyAnimationMemberPlanError::MemberCountOverflow
        );
        // A failed count must not consume the semantic leaf.
        builder.accept_leaf(second, ObjectId::new(2), 0).unwrap();
        assert_eq!(builder.finish().unwrap().total_member_count(), u32::MAX);
    }
}
