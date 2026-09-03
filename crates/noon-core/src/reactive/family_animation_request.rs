use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    FamilyAnimationError, FamilyAnimationSpec, ObjectId, SemanticNodeId, SemanticStore,
    SemanticStoreError,
};

/// One authoritative semantic-leaf to runtime-object binding carried across authoring.
///
/// The binding contains only stable semantic/runtime identity. Content-local members
/// such as shaped glyphs are deliberately resolved after retained scene materialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyAnimationLeafBinding {
    pub semantic_leaf: SemanticNodeId,
    pub object: ObjectId,
}

impl FamilyAnimationLeafBinding {
    pub const fn new(semantic_leaf: SemanticNodeId, object: ObjectId) -> Self {
        Self {
            semantic_leaf,
            object,
        }
    }
}

/// Glyph/resource-free request for one semantic-family animation.
///
/// `bindings` are already in authoritative [`SemanticStore`] leaf order. Frontends may
/// discover concrete runtime objects in any order; [`Self::from_semantic_bindings`]
/// restores semantic order in Rust before the request crosses an authoring boundary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyAnimationRequest {
    target: SemanticNodeId,
    bindings: Vec<FamilyAnimationLeafBinding>,
    spec: FamilyAnimationSpec,
}

impl FamilyAnimationRequest {
    pub fn new(
        target: SemanticNodeId,
        bindings: Vec<FamilyAnimationLeafBinding>,
        spec: FamilyAnimationSpec,
    ) -> Result<Self, FamilyAnimationRequestError> {
        let request = Self {
            target,
            bindings,
            spec,
        };
        request.validate()?;
        Ok(request)
    }

    /// Snapshot authoritative semantic order while accepting bindings in arbitrary
    /// materialization order.
    pub fn from_semantic_bindings(
        store: &SemanticStore,
        target: SemanticNodeId,
        spec: FamilyAnimationSpec,
        bindings: impl IntoIterator<Item = FamilyAnimationLeafBinding>,
    ) -> Result<Self, FamilyAnimationRequestError> {
        spec.validate()?;
        let expected = store.ordered_leaf_nodes(target)?;
        let expected_set = expected.iter().copied().collect::<HashSet<_>>();
        let mut by_leaf = HashMap::with_capacity(expected.len());
        let mut used_objects = HashSet::with_capacity(expected.len());

        for binding in bindings {
            if !expected_set.contains(&binding.semantic_leaf) {
                return Err(FamilyAnimationRequestError::UnexpectedLeaf(
                    binding.semantic_leaf,
                ));
            }
            if by_leaf.contains_key(&binding.semantic_leaf) {
                return Err(FamilyAnimationRequestError::DuplicateLeaf(
                    binding.semantic_leaf,
                ));
            }
            if !used_objects.insert(binding.object) {
                return Err(FamilyAnimationRequestError::DuplicateObject(binding.object));
            }
            by_leaf.insert(binding.semantic_leaf, binding);
        }

        let mut ordered = Vec::with_capacity(expected.len());
        for semantic_leaf in expected {
            let binding = by_leaf
                .remove(&semantic_leaf)
                .ok_or(FamilyAnimationRequestError::MissingLeaf(semantic_leaf))?;
            ordered.push(binding);
        }
        debug_assert!(by_leaf.is_empty());
        Self::new(target, ordered, spec)
    }

    pub const fn target(&self) -> SemanticNodeId {
        self.target
    }

    pub fn bindings(&self) -> &[FamilyAnimationLeafBinding] {
        &self.bindings
    }

    pub const fn spec(&self) -> FamilyAnimationSpec {
        self.spec
    }

    /// Revalidate after serialization or any frontend-owned storage boundary.
    pub fn validate(&self) -> Result<(), FamilyAnimationRequestError> {
        self.spec.validate()?;
        let mut leaves = HashSet::with_capacity(self.bindings.len());
        let mut objects = HashSet::with_capacity(self.bindings.len());
        for binding in &self.bindings {
            if !leaves.insert(binding.semantic_leaf) {
                return Err(FamilyAnimationRequestError::DuplicateLeaf(
                    binding.semantic_leaf,
                ));
            }
            if !objects.insert(binding.object) {
                return Err(FamilyAnimationRequestError::DuplicateObject(binding.object));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FamilyAnimationRequestError {
    Animation(FamilyAnimationError),
    Semantic(SemanticStoreError),
    UnexpectedLeaf(SemanticNodeId),
    DuplicateLeaf(SemanticNodeId),
    DuplicateObject(ObjectId),
    MissingLeaf(SemanticNodeId),
}

impl std::fmt::Display for FamilyAnimationRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Animation(error) => error.fmt(formatter),
            Self::Semantic(error) => error.fmt(formatter),
            Self::UnexpectedLeaf(leaf) => write!(
                formatter,
                "semantic leaf {}:{} is not part of the family animation target",
                leaf.slot(),
                leaf.generation()
            ),
            Self::DuplicateLeaf(leaf) => write!(
                formatter,
                "semantic leaf {}:{} is bound more than once in the family animation request",
                leaf.slot(),
                leaf.generation()
            ),
            Self::DuplicateObject(object) => write!(
                formatter,
                "runtime object {} is bound to more than one semantic family leaf",
                object.get()
            ),
            Self::MissingLeaf(leaf) => write!(
                formatter,
                "semantic leaf {}:{} has no runtime object binding",
                leaf.slot(),
                leaf.generation()
            ),
        }
    }
}

impl std::error::Error for FamilyAnimationRequestError {}

impl From<FamilyAnimationError> for FamilyAnimationRequestError {
    fn from(value: FamilyAnimationError) -> Self {
        Self::Animation(value)
    }
}

impl From<SemanticStoreError> for FamilyAnimationRequestError {
    fn from(value: SemanticStoreError) -> Self {
        Self::Semantic(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::{FamilyAnimationMode, RateFunction};

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

    fn mixed_semantics() -> (
        SemanticStore,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
    ) {
        let mut store = SemanticStore::new();
        let text = store.insert_authoring_object();
        let circle = store.insert_authoring_object();
        let family = store.insert_family();
        store.add_member(family, text).unwrap();
        store.add_member(family, circle).unwrap();
        (store, family, text, circle)
    }

    #[test]
    fn arbitrary_binding_order_is_restored_to_authoritative_semantic_order() {
        let (store, family, text, circle) = mixed_semantics();
        let request = FamilyAnimationRequest::from_semantic_bindings(
            &store,
            family,
            spec(),
            [
                FamilyAnimationLeafBinding::new(circle, ObjectId::new(11)),
                FamilyAnimationLeafBinding::new(text, ObjectId::new(10)),
            ],
        )
        .unwrap();

        assert_eq!(request.target(), family);
        assert_eq!(request.bindings()[0].semantic_leaf, text);
        assert_eq!(request.bindings()[0].object, ObjectId::new(10));
        assert_eq!(request.bindings()[1].semantic_leaf, circle);
        assert_eq!(request.bindings()[1].object, ObjectId::new(11));
    }

    #[test]
    fn missing_unexpected_and_duplicate_bindings_fail_closed() {
        let (mut store, family, text, circle) = mixed_semantics();
        let outsider = store.insert_authoring_object();

        assert!(matches!(
            FamilyAnimationRequest::from_semantic_bindings(
                &store,
                family,
                spec(),
                [FamilyAnimationLeafBinding::new(text, ObjectId::new(10))],
            ),
            Err(FamilyAnimationRequestError::MissingLeaf(leaf)) if leaf == circle
        ));
        assert!(matches!(
            FamilyAnimationRequest::from_semantic_bindings(
                &store,
                family,
                spec(),
                [
                    FamilyAnimationLeafBinding::new(text, ObjectId::new(10)),
                    FamilyAnimationLeafBinding::new(outsider, ObjectId::new(11)),
                ],
            ),
            Err(FamilyAnimationRequestError::UnexpectedLeaf(leaf)) if leaf == outsider
        ));
        assert!(matches!(
            FamilyAnimationRequest::new(
                family,
                vec![
                    FamilyAnimationLeafBinding::new(text, ObjectId::new(10)),
                    FamilyAnimationLeafBinding::new(text, ObjectId::new(11)),
                ],
                spec(),
            ),
            Err(FamilyAnimationRequestError::DuplicateLeaf(leaf)) if leaf == text
        ));
        assert!(matches!(
            FamilyAnimationRequest::new(
                family,
                vec![
                    FamilyAnimationLeafBinding::new(text, ObjectId::new(10)),
                    FamilyAnimationLeafBinding::new(circle, ObjectId::new(10)),
                ],
                spec(),
            ),
            Err(FamilyAnimationRequestError::DuplicateObject(object))
                if object == ObjectId::new(10)
        ));
    }
}
