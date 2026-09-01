use std::collections::{HashMap, HashSet};

use noon_core::{
    FamilyAnimationError, FamilyAnimationMemberPlanError, FamilyAnimationSpec, ObjectId,
    RetainedFamilyAnimationMemberPlanError, RetainedFamilyAnimationPlan,
    RetainedFamilyAnimationPlanBuilder, SemanticNodeId, SemanticStore,
};

use crate::{RetainedMobject, RetainedScene};

/// Fully lowered retained-family animation ready for runtime execution.
///
/// Semantic family identity and ordering have already been resolved into the immutable
/// retained plan. Frontends never need to receive or reconstruct an ordered `ObjectId`
/// sequence; they only provide individual semantic-to-retained bindings while objects
/// are materialized.
#[derive(Clone, Debug, PartialEq)]
pub struct LoweredRetainedFamilyAnimation {
    plan: RetainedFamilyAnimationPlan,
    spec: FamilyAnimationSpec,
}

impl LoweredRetainedFamilyAnimation {
    pub const fn plan(&self) -> &RetainedFamilyAnimationPlan {
        &self.plan
    }

    pub const fn spec(&self) -> FamilyAnimationSpec {
        self.spec
    }

    pub fn into_parts(self) -> (RetainedFamilyAnimationPlan, FamilyAnimationSpec) {
        (self.plan, self.spec)
    }
}

/// Transactional Rust-owned bridge from semantic authoring handles to retained objects.
///
/// The session snapshots authoritative leaf order from [`SemanticStore`] at `begin`.
/// Callers may bind leaves in any materialization order. `finish` resolves each bound
/// `ObjectId` against the actual [`RetainedScene`] and replays bindings into
/// [`RetainedFamilyAnimationPlanBuilder`] strictly in the semantic snapshot order.
/// This keeps family ordering and Text member resolution out of Python/JavaScript.
#[derive(Clone, Debug)]
pub struct RetainedFamilyAnimationLoweringSession {
    builder: RetainedFamilyAnimationPlanBuilder,
    expected_leaves: Vec<SemanticNodeId>,
    expected_leaf_set: HashSet<SemanticNodeId>,
    bindings: HashMap<SemanticNodeId, ObjectId>,
    used_objects: HashSet<ObjectId>,
    spec: FamilyAnimationSpec,
}

impl RetainedFamilyAnimationLoweringSession {
    pub fn begin(
        store: &SemanticStore,
        target: SemanticNodeId,
        spec: FamilyAnimationSpec,
    ) -> Result<Self, RetainedFamilyAnimationLoweringError> {
        spec.validate()?;
        let builder = RetainedFamilyAnimationPlanBuilder::begin(store, target)?;
        let expected_leaves = store
            .ordered_leaf_nodes(target)
            .map_err(FamilyAnimationMemberPlanError::from)
            .map_err(RetainedFamilyAnimationMemberPlanError::from)?;
        debug_assert_eq!(builder.expected_leaf_count(), expected_leaves.len());
        let expected_leaf_set = expected_leaves.iter().copied().collect();
        Ok(Self {
            builder,
            expected_leaves,
            expected_leaf_set,
            bindings: HashMap::new(),
            used_objects: HashSet::new(),
            spec,
        })
    }

    pub fn expected_leaf_count(&self) -> usize {
        self.expected_leaves.len()
    }

    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Bind one semantic leaf to its final retained object identity.
    ///
    /// Binding order is intentionally irrelevant. The authoritative order is restored
    /// only in [`Self::finish`].
    pub fn bind_leaf(
        &mut self,
        semantic_leaf: SemanticNodeId,
        object: ObjectId,
    ) -> Result<(), RetainedFamilyAnimationLoweringError> {
        if !self.expected_leaf_set.contains(&semantic_leaf) {
            return Err(RetainedFamilyAnimationLoweringError::UnexpectedLeaf(
                semantic_leaf,
            ));
        }
        if self.bindings.contains_key(&semantic_leaf) {
            return Err(RetainedFamilyAnimationLoweringError::DuplicateLeaf(
                semantic_leaf,
            ));
        }
        if self.used_objects.contains(&object) {
            return Err(RetainedFamilyAnimationLoweringError::DuplicateObject(
                object,
            ));
        }
        self.bindings.insert(semantic_leaf, object);
        self.used_objects.insert(object);
        Ok(())
    }

    pub fn bind_mobject(
        &mut self,
        semantic_leaf: SemanticNodeId,
        object: RetainedMobject,
    ) -> Result<(), RetainedFamilyAnimationLoweringError> {
        self.bind_leaf(semantic_leaf, object.id())
    }

    pub fn finish(
        self,
        scene: &RetainedScene,
    ) -> Result<LoweredRetainedFamilyAnimation, RetainedFamilyAnimationLoweringError> {
        let Self {
            mut builder,
            expected_leaves,
            mut bindings,
            spec,
            ..
        } = self;

        for semantic_leaf in expected_leaves {
            let object_id = bindings.remove(&semantic_leaf).ok_or(
                RetainedFamilyAnimationLoweringError::MissingLeaf(semantic_leaf),
            )?;
            let object = scene
                .objects()
                .iter()
                .find(|object| object.id == object_id)
                .ok_or(RetainedFamilyAnimationLoweringError::MissingObject(
                    object_id,
                ))?;
            builder.accept_leaf(semantic_leaf, object, scene.texts())?;
        }
        debug_assert!(bindings.is_empty());

        Ok(LoweredRetainedFamilyAnimation {
            plan: builder.finish()?,
            spec,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyAnimationLoweringError {
    Animation(FamilyAnimationError),
    Plan(RetainedFamilyAnimationMemberPlanError),
    UnexpectedLeaf(SemanticNodeId),
    DuplicateLeaf(SemanticNodeId),
    DuplicateObject(ObjectId),
    MissingLeaf(SemanticNodeId),
    MissingObject(ObjectId),
}

impl std::fmt::Display for RetainedFamilyAnimationLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Animation(error) => error.fmt(formatter),
            Self::Plan(error) => error.fmt(formatter),
            Self::UnexpectedLeaf(leaf) => write!(
                formatter,
                "semantic leaf {}:{} is not part of the retained family lowering target",
                leaf.slot(),
                leaf.generation()
            ),
            Self::DuplicateLeaf(leaf) => write!(
                formatter,
                "semantic leaf {}:{} was bound to retained content more than once",
                leaf.slot(),
                leaf.generation()
            ),
            Self::DuplicateObject(object) => write!(
                formatter,
                "retained object {} was bound to more than one semantic family leaf",
                object.get()
            ),
            Self::MissingLeaf(leaf) => write!(
                formatter,
                "semantic leaf {}:{} has no retained object binding",
                leaf.slot(),
                leaf.generation()
            ),
            Self::MissingObject(object) => write!(
                formatter,
                "retained family binding references missing object {}",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyAnimationLoweringError {}

impl From<FamilyAnimationError> for RetainedFamilyAnimationLoweringError {
    fn from(value: FamilyAnimationError) -> Self {
        Self::Animation(value)
    }
}

impl From<RetainedFamilyAnimationMemberPlanError> for RetainedFamilyAnimationLoweringError {
    fn from(value: RetainedFamilyAnimationMemberPlanError) -> Self {
        Self::Plan(value)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        FamilyAnimationMode, GeometryRef, RateFunction, SceneDefinition, SemanticStore,
    };

    use crate::Text;

    use super::*;

    fn spec() -> FamilyAnimationSpec {
        FamilyAnimationSpec::new(
            FamilyAnimationMode::Reveal,
            1.0,
            2.0,
            1.0,
            RateFunction::Linear,
            false,
            false,
        )
        .unwrap()
    }

    fn mixed_scene() -> (RetainedScene, ObjectId, ObjectId) {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(1.0));
        let text = ObjectId::new(1_u64 << 52);
        let mut scene = RetainedScene::from_legacy(&legacy).unwrap();
        scene
            .insert_native_text_at(0, text, Text::new("AB"))
            .unwrap();
        (scene, text, circle)
    }

    fn mixed_semantics() -> (
        SemanticStore,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
    ) {
        let mut semantics = SemanticStore::new();
        let text = semantics.insert_authoring_object();
        let circle = semantics.insert_authoring_object();
        let family = semantics.insert_family();
        semantics.add_member(family, text).unwrap();
        semantics.add_member(family, circle).unwrap();
        (semantics, family, text, circle)
    }

    #[test]
    fn out_of_order_bindings_lower_in_authoritative_semantic_order() {
        let (scene, text_object, circle_object) = mixed_scene();
        let (semantics, family, text_leaf, circle_leaf) = mixed_semantics();
        let expected_spec = spec();
        let mut session =
            RetainedFamilyAnimationLoweringSession::begin(&semantics, family, expected_spec)
                .unwrap();

        // Materialization order is deliberately opposite semantic family order.
        session.bind_leaf(circle_leaf, circle_object).unwrap();
        session.bind_leaf(text_leaf, text_object).unwrap();
        let lowered = session.finish(&scene).unwrap();

        assert_eq!(lowered.spec(), expected_spec);
        assert_eq!(lowered.plan().member_plan().target(), family);
        assert_eq!(lowered.plan().member_plan().total_member_count(), 3);
        assert_eq!(lowered.plan().leaves().len(), 2);
        assert_eq!(lowered.plan().leaves()[0].span().semantic_leaf, text_leaf);
        assert_eq!(lowered.plan().leaves()[0].span().object, text_object);
        assert_eq!(lowered.plan().leaves()[0].span().member_count, 2);
        assert_eq!(lowered.plan().leaves()[1].span().semantic_leaf, circle_leaf);
        assert_eq!(lowered.plan().leaves()[1].span().object, circle_object);
        assert_eq!(lowered.plan().leaves()[1].span().member_count, 1);
    }

    #[test]
    fn missing_leaf_fails_before_plan_commit() {
        let (scene, text_object, _) = mixed_scene();
        let (semantics, family, text_leaf, circle_leaf) = mixed_semantics();
        let mut session =
            RetainedFamilyAnimationLoweringSession::begin(&semantics, family, spec()).unwrap();
        session.bind_leaf(text_leaf, text_object).unwrap();

        assert!(matches!(
            session.finish(&scene),
            Err(RetainedFamilyAnimationLoweringError::MissingLeaf(leaf)) if leaf == circle_leaf
        ));
    }

    #[test]
    fn unexpected_and_duplicate_bindings_fail_closed() {
        let (_, text_object, circle_object) = mixed_scene();
        let (mut semantics, family, text_leaf, circle_leaf) = mixed_semantics();
        let outsider = semantics.insert_authoring_object();
        let mut session =
            RetainedFamilyAnimationLoweringSession::begin(&semantics, family, spec()).unwrap();

        assert!(matches!(
            session.bind_leaf(outsider, text_object),
            Err(RetainedFamilyAnimationLoweringError::UnexpectedLeaf(leaf)) if leaf == outsider
        ));
        session.bind_leaf(text_leaf, text_object).unwrap();
        assert!(matches!(
            session.bind_leaf(text_leaf, circle_object),
            Err(RetainedFamilyAnimationLoweringError::DuplicateLeaf(leaf)) if leaf == text_leaf
        ));
        assert!(matches!(
            session.bind_leaf(circle_leaf, text_object),
            Err(RetainedFamilyAnimationLoweringError::DuplicateObject(object))
                if object == text_object
        ));
    }

    #[test]
    fn missing_retained_object_is_reported_by_scene_resolution() {
        let (scene, text_object, _) = mixed_scene();
        let (semantics, family, text_leaf, circle_leaf) = mixed_semantics();
        let missing = ObjectId::new((1_u64 << 52) + 99);
        let mut session =
            RetainedFamilyAnimationLoweringSession::begin(&semantics, family, spec()).unwrap();
        session.bind_leaf(text_leaf, text_object).unwrap();
        session.bind_leaf(circle_leaf, missing).unwrap();

        assert!(matches!(
            session.finish(&scene),
            Err(RetainedFamilyAnimationLoweringError::MissingObject(object)) if object == missing
        ));
    }
}
