use std::{cell::RefCell, rc::Rc};

use noon::{
    LoweredRetainedFamilyAnimation, RetainedFamilyAnimationLoweringError,
    RetainedFamilyAnimationLoweringSession, RetainedScene,
};
use noon_core::{FamilyAnimationSpec, ObjectId, SemanticNodeId, SemanticStore};

/// Shared semantic authoring owner used to scope otherwise-local semantic node IDs.
///
/// `SemanticNodeId` is intentionally store-local. Frontends must therefore retain the
/// originating store together with a node ID whenever identity crosses object/resource
/// specific authoring handles. Pointer identity stays entirely inside Rust and never
/// becomes part of the Python/JavaScript or renderer wire contract.
#[derive(Clone)]
pub struct AuthoringSemanticStore {
    inner: Rc<RefCell<SemanticStore>>,
}

impl Default for AuthoringSemanticStore {
    fn default() -> Self {
        Self {
            inner: Rc::new(RefCell::new(SemanticStore::new())),
        }
    }
}

impl AuthoringSemanticStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_object(&self) -> AuthoringSemanticIdentity {
        let id = self.inner.borrow_mut().insert_authoring_object();
        AuthoringSemanticIdentity::new(self.clone(), id)
    }

    pub fn insert_family(&self) -> AuthoringSemanticIdentity {
        let id = self.inner.borrow_mut().insert_family();
        AuthoringSemanticIdentity::new(self.clone(), id)
    }

    pub fn add_member(
        &self,
        family: &AuthoringSemanticIdentity,
        member: &AuthoringSemanticIdentity,
    ) -> Result<(), AuthoringSemanticError> {
        self.require_identity(family)?;
        self.require_identity(member)?;
        self.inner
            .borrow_mut()
            .add_member(family.node, member.node)
            .map_err(|error| AuthoringSemanticError::Semantic(error.to_string()))
    }

    fn require_identity(
        &self,
        identity: &AuthoringSemanticIdentity,
    ) -> Result<(), AuthoringSemanticError> {
        if self.same_store(&identity.store) {
            Ok(())
        } else {
            Err(AuthoringSemanticError::StoreMismatch)
        }
    }

    fn same_store(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

/// Store-scoped semantic identity for one live authoring node.
///
/// Equality deliberately includes the originating store rather than comparing the
/// numeric `SemanticNodeId` alone. This prevents independent authoring stores that
/// happen to allocate the same slot/generation from aliasing each other.
#[derive(Clone)]
pub struct AuthoringSemanticIdentity {
    store: AuthoringSemanticStore,
    node: SemanticNodeId,
}

impl AuthoringSemanticIdentity {
    fn new(store: AuthoringSemanticStore, node: SemanticNodeId) -> Self {
        Self { store, node }
    }

    pub const fn node_id(&self) -> SemanticNodeId {
        self.node
    }

    pub fn same_store(&self, other: &Self) -> bool {
        self.store.same_store(&other.store)
    }

    pub fn matches(&self, other: &Self) -> bool {
        self.node == other.node && self.same_store(other)
    }
}

impl std::fmt::Debug for AuthoringSemanticIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthoringSemanticIdentity")
            .field("node", &self.node)
            .finish_non_exhaustive()
    }
}

/// Rust-owned bridge from live authoring identity to final retained object identity.
///
/// Frontends bind one materialized leaf at a time. They never supply an ordered family
/// member list: authoritative ordering is snapshotted from the semantic store when the
/// session begins and remains owned by `RetainedFamilyAnimationLoweringSession`.
pub struct RetainedFamilyAuthoringLoweringSession {
    store: AuthoringSemanticStore,
    lowering: RetainedFamilyAnimationLoweringSession,
}

impl RetainedFamilyAuthoringLoweringSession {
    pub fn begin(
        target: &AuthoringSemanticIdentity,
        spec: FamilyAnimationSpec,
    ) -> Result<Self, AuthoringSemanticError> {
        let lowering = {
            let store = target.store.inner.borrow();
            RetainedFamilyAnimationLoweringSession::begin(&store, target.node, spec)?
        };
        Ok(Self {
            store: target.store.clone(),
            lowering,
        })
    }

    pub fn expected_leaf_count(&self) -> usize {
        self.lowering.expected_leaf_count()
    }

    pub fn binding_count(&self) -> usize {
        self.lowering.binding_count()
    }

    pub fn bind_leaf(
        &mut self,
        leaf: &AuthoringSemanticIdentity,
        object: ObjectId,
    ) -> Result<(), AuthoringSemanticError> {
        self.store.require_identity(leaf)?;
        self.lowering.bind_leaf(leaf.node, object)?;
        Ok(())
    }

    pub fn finish(
        self,
        scene: &RetainedScene,
    ) -> Result<LoweredRetainedFamilyAnimation, AuthoringSemanticError> {
        self.lowering.finish(scene).map_err(Into::into)
    }
}

#[derive(Debug)]
pub enum AuthoringSemanticError {
    StoreMismatch,
    Semantic(String),
    Lowering(RetainedFamilyAnimationLoweringError),
}

impl std::fmt::Display for AuthoringSemanticError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StoreMismatch => write!(
                formatter,
                "authoring semantic identities belong to different stores"
            ),
            Self::Semantic(error) => formatter.write_str(error),
            Self::Lowering(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for AuthoringSemanticError {}

impl From<RetainedFamilyAnimationLoweringError> for AuthoringSemanticError {
    fn from(value: RetainedFamilyAnimationLoweringError) -> Self {
        Self::Lowering(value)
    }
}

#[cfg(test)]
mod tests {
    use noon::{RetainedScene, Text};
    use noon_core::{FamilyAnimationMode, GeometryRef, ObjectId, RateFunction, SceneDefinition};

    use super::*;

    fn reveal_spec() -> FamilyAnimationSpec {
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

    #[test]
    fn semantic_identity_is_scoped_by_store_even_when_node_ids_collide() {
        let first_store = AuthoringSemanticStore::new();
        let second_store = AuthoringSemanticStore::new();
        let first = first_store.insert_object();
        let second = second_store.insert_object();

        assert_eq!(first.node_id(), second.node_id());
        assert!(!first.same_store(&second));
        assert!(!first.matches(&second));

        let family = first_store.insert_family();
        assert!(matches!(
            first_store.add_member(&family, &second),
            Err(AuthoringSemanticError::StoreMismatch)
        ));
    }

    #[test]
    fn lowering_binds_live_identities_without_materialization_order_becoming_family_order() {
        let semantics = AuthoringSemanticStore::new();
        let text_leaf = semantics.insert_object();
        let circle_leaf = semantics.insert_object();
        let family = semantics.insert_family();
        semantics.add_member(&family, &text_leaf).unwrap();
        semantics.add_member(&family, &circle_leaf).unwrap();

        let mut legacy = SceneDefinition::new();
        let circle_id = legacy.add(GeometryRef::circle(1.0));
        let text_id = ObjectId::new(1_u64 << 52);
        let mut scene = RetainedScene::from_legacy(&legacy).unwrap();
        scene
            .insert_native_text_at(0, text_id, Text::new("AB"))
            .unwrap();

        let mut lowering =
            RetainedFamilyAuthoringLoweringSession::begin(&family, reveal_spec()).unwrap();
        assert_eq!(lowering.expected_leaf_count(), 2);

        // Materialization order is deliberately opposite semantic family order.
        lowering.bind_leaf(&circle_leaf, circle_id).unwrap();
        lowering.bind_leaf(&text_leaf, text_id).unwrap();
        let lowered = lowering.finish(&scene).unwrap();

        assert_eq!(lowered.plan().leaves().len(), 2);
        assert_eq!(lowered.plan().leaves()[0].span().object, text_id);
        assert_eq!(lowered.plan().leaves()[0].span().member_count, 2);
        assert_eq!(lowered.plan().leaves()[1].span().object, circle_id);
        assert_eq!(lowered.plan().leaves()[1].span().member_count, 1);
    }

    #[test]
    fn lowering_rejects_a_live_leaf_from_another_store_before_raw_id_binding() {
        let semantics = AuthoringSemanticStore::new();
        let leaf = semantics.insert_object();
        let family = semantics.insert_family();
        semantics.add_member(&family, &leaf).unwrap();

        let other = AuthoringSemanticStore::new().insert_object();
        assert_eq!(leaf.node_id(), other.node_id());

        let mut lowering =
            RetainedFamilyAuthoringLoweringSession::begin(&family, reveal_spec()).unwrap();
        assert!(matches!(
            lowering.bind_leaf(&other, ObjectId::new(9)),
            Err(AuthoringSemanticError::StoreMismatch)
        ));
        assert_eq!(lowering.binding_count(), 0);
    }
}
