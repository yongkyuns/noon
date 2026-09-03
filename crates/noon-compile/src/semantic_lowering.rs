use std::collections::{HashMap, HashSet};

use noon_core::{
    ObjectId, SemanticMutationImpact, SemanticMutationTransactionResult, SemanticNodeId,
    SemanticObjectState, SemanticStore, SemanticStoreError,
};

/// Compiler-owned identity bridge from authoritative semantic nodes to the existing
/// object-key domain consumed by `CompiledScene` and runtime execution slots.
///
/// Semantic identity remains authoritative. The `ObjectId` values stored here are
/// derived compatibility keys only; they are not written back into `SemanticStore`
/// and must not become frontend/authoring identity. #959/A4 owns deletion of this
/// bridge once the compiled/runtime path accepts semantic identities directly.
///
/// The index deliberately does not allocate a second slot domain. A compatibility
/// key is a one-to-one encoding of the semantic node's generational identity, while
/// the existing compiler/runtime remains responsible for dense/stable execution
/// slots.
#[derive(Clone, Debug, Default)]
pub struct SemanticExecutionIndex {
    object_ids: HashMap<SemanticNodeId, ObjectId>,
}

impl SemanticExecutionIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.object_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.object_ids.is_empty()
    }

    /// Return the existing execution compatibility key for one indexed semantic
    /// object. Detached or never-lowered nodes are absent until an AddNode impact or
    /// scene lowering observes them.
    pub fn execution_object_id(&self, semantic_id: SemanticNodeId) -> Option<ObjectId> {
        self.object_ids.get(&semantic_id).copied()
    }

    /// Apply committed A1.5 mutation impacts to the identity index without scanning
    /// unrelated semantic nodes.
    ///
    /// Object creation installs only that newly allocated target object identity.
    /// Structural removal deletes exactly the identities reported by the semantic
    /// transaction's reverse-reference cleanup. Property/content/subscription and
    /// family-order impacts do not change identity and therefore require no index
    /// mutation.
    pub fn apply_transaction_result(
        &mut self,
        store: &SemanticStore,
        result: &SemanticMutationTransactionResult,
    ) {
        self.apply_impacts(store, result.impacts());
    }

    pub fn apply_impacts(&mut self, store: &SemanticStore, impacts: &[SemanticMutationImpact]) {
        for impact in impacts {
            match *impact {
                SemanticMutationImpact::NodeAdded { node } => {
                    if store
                        .node(node)
                        .and_then(|node| node.semantic_object_state())
                        .is_some()
                    {
                        self.ensure_object(node);
                    }
                }
                SemanticMutationImpact::NodeRemoved { node } => {
                    self.object_ids.remove(&node);
                }
                SemanticMutationImpact::SignalValue { .. }
                | SemanticMutationImpact::ObjectProperty { .. }
                | SemanticMutationImpact::ObjectContent { .. }
                | SemanticMutationImpact::Subscription { .. }
                | SemanticMutationImpact::FamilyMemberAdded { .. }
                | SemanticMutationImpact::FamilyMemberRemoved { .. }
                | SemanticMutationImpact::FamilyMemberReordered { .. }
                | SemanticMutationImpact::AnimationAdded { .. } => {}
            }
        }
    }

    /// Lower the current authoritative semantic scene to the first typed execution
    /// handoff without copying or lossy-converting authored object payloads.
    ///
    /// Top-level scene order and family depth-first order come from `SemanticStore`.
    /// Shared/aliased leaves are emitted once at their first visible occurrence.
    /// Geometry and text remain represented by `SemanticObjectState`, including
    /// versioned resource handles, so this boundary does not regress target semantic
    /// content into legacy `GeometryRef`/retained compiler payloads.
    ///
    /// Validation completes before the index is mutated, so a stale/migration-only
    /// visible leaf cannot leave a partially updated execution identity map.
    pub fn lower_scene<'a>(
        &mut self,
        store: &'a SemanticStore,
    ) -> Result<SemanticExecutionProjection<'a>, SemanticLoweringError> {
        let mut pending = Vec::new();
        let mut seen = HashSet::new();

        for root in store.scene_roots() {
            for semantic_id in store.ordered_leaf_nodes(root)? {
                if !seen.insert(semantic_id) {
                    continue;
                }
                let state = store
                    .node(semantic_id)
                    .and_then(|node| node.semantic_object_state())
                    .ok_or(SemanticLoweringError::MissingSemanticObjectState(
                        semantic_id,
                    ))?;
                pending.push((semantic_id, state));
            }
        }

        let objects = pending
            .into_iter()
            .map(|(semantic_id, state)| SemanticExecutionObject {
                semantic_id,
                execution_id: self.ensure_object(semantic_id),
                state,
            })
            .collect();

        Ok(SemanticExecutionProjection { objects })
    }

    fn ensure_object(&mut self, semantic_id: SemanticNodeId) -> ObjectId {
        *self
            .object_ids
            .entry(semantic_id)
            .or_insert_with(|| compatibility_object_id(semantic_id))
    }
}

/// Borrowed typed handoff produced at the Semantic Scene -> execution boundary.
///
/// This is intentionally not another runtime scene/plan model: stable compiled and
/// runtime slots remain owned by `CompiledScene`/`ExecutionSlotTable`. The borrowed
/// semantic payload lets the next lowering slices feed those existing mechanisms
/// without first creating a dense retained-scene mirror.
#[derive(Debug)]
pub struct SemanticExecutionProjection<'a> {
    objects: Vec<SemanticExecutionObject<'a>>,
}

impl<'a> SemanticExecutionProjection<'a> {
    pub fn objects(&self) -> &[SemanticExecutionObject<'a>] {
        &self.objects
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SemanticExecutionObject<'a> {
    /// Authoritative scene-global semantic identity.
    pub semantic_id: SemanticNodeId,
    /// Temporary key accepted by the existing compiled/runtime object domain.
    pub execution_id: ObjectId,
    /// Authoritative mixed semantic payload; no legacy content conversion occurs.
    pub state: &'a SemanticObjectState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticLoweringError {
    Store(SemanticStoreError),
    /// A visible object leaf came from a migration-only legacy/state-less path
    /// instead of carrying target `SemanticObjectState` directly.
    MissingSemanticObjectState(SemanticNodeId),
}

impl From<SemanticStoreError> for SemanticLoweringError {
    fn from(value: SemanticStoreError) -> Self {
        Self::Store(value)
    }
}

impl std::fmt::Display for SemanticLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => error.fmt(formatter),
            Self::MissingSemanticObjectState(id) => write!(
                formatter,
                "semantic execution lowering requires target object state for visible node {}:{}",
                id.slot(),
                id.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticLoweringError {}

/// One-to-one compatibility encoding for the target semantic object domain.
///
/// `SemanticNodeId` already owns generation-safe identity. Packing its two u32
/// components into the legacy u64 wrapper avoids introducing an allocator or a
/// second lifetime while the existing compiler/runtime still accepts `ObjectId`.
fn compatibility_object_id(id: SemanticNodeId) -> ObjectId {
    let raw = (u64::from(id.generation()) << 32) | u64::from(id.slot());
    ObjectId::new(raw)
}

#[cfg(test)]
mod tests {
    use noon_core::{
        SemanticMutationImpact, SemanticMutationTransaction, SemanticNodeCreation,
        SemanticObjectContent, SemanticObjectProperty, SemanticObjectState, SemanticStore,
        StoredGeometry, TextResourceHandle, TextResourceId,
    };

    use super::*;

    fn circle(radius: f32) -> SemanticObjectState {
        SemanticObjectState::new(StoredGeometry::Circle { radius })
    }

    fn text(id: u64) -> SemanticObjectState {
        SemanticObjectState::new(TextResourceHandle {
            id: TextResourceId::new(id),
            version: 0,
        })
    }

    fn attach(store: &mut SemanticStore, state: SemanticObjectState) -> SemanticNodeId {
        let id = store.insert_semantic_object(state);
        store.attach_to_scene(id).unwrap();
        id
    }

    #[test]
    fn lower_scene_preserves_mixed_semantic_content_and_family_order() {
        let mut store = SemanticStore::new();
        let geometry = store.insert_semantic_object(circle(2.0));
        let text = store.insert_semantic_object(text(7));
        let family = store.insert_family();
        store.add_member(family, geometry).unwrap();
        store.add_member(family, text).unwrap();
        store.attach_to_scene(family).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let lowered = index.lower_scene(&store).unwrap();

        assert_eq!(
            lowered
                .objects()
                .iter()
                .map(|object| object.semantic_id)
                .collect::<Vec<_>>(),
            vec![geometry, text]
        );
        assert!(matches!(
            lowered.objects()[0].state.content,
            SemanticObjectContent::Geometry(StoredGeometry::Circle { radius: 2.0 })
        ));
        assert!(matches!(
            lowered.objects()[1].state.content,
            SemanticObjectContent::Text(_)
        ));
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn aliases_across_scene_roots_emit_one_execution_object() {
        let mut store = SemanticStore::new();
        let shared = store.insert_semantic_object(circle(1.0));
        let first = store.insert_family();
        let second = store.insert_family();
        store.add_member(first, shared).unwrap();
        store.add_member(second, shared).unwrap();
        store.attach_to_scene(first).unwrap();
        store.attach_to_scene(second).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let lowered = index.lower_scene(&store).unwrap();

        assert_eq!(lowered.len(), 1);
        assert_eq!(lowered.objects()[0].semantic_id, shared);
    }

    #[test]
    fn object_mutation_impacts_preserve_execution_identity() {
        let mut store = SemanticStore::new();
        let object = attach(&mut store, circle(1.0));
        let mut index = SemanticExecutionIndex::new();
        let before = index.lower_scene(&store).unwrap().objects()[0].execution_id;

        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_property(object, SemanticObjectProperty::RotationZ, 0.5_f64)
            .replace_content(object, StoredGeometry::Circle { radius: 3.0 });
        let result = transaction.apply(&mut store).unwrap();
        index.apply_transaction_result(&store, &result);

        let after = index.lower_scene(&store).unwrap().objects()[0].execution_id;
        assert_eq!(after, before);
        assert_eq!(index.execution_object_id(object), Some(before));
    }

    #[test]
    fn family_reorder_changes_projection_order_without_identity_churn() {
        let mut store = SemanticStore::new();
        let first = store.insert_semantic_object(circle(1.0));
        let second = store.insert_semantic_object(circle(2.0));
        let third = store.insert_semantic_object(circle(3.0));
        let family = store.insert_family();
        for member in [first, second, third] {
            store.add_member(family, member).unwrap();
        }
        store.attach_to_scene(family).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let initial = index
            .lower_scene(&store)
            .unwrap()
            .objects()
            .iter()
            .map(|object| (object.semantic_id, object.execution_id))
            .collect::<Vec<_>>();

        let mut transaction = SemanticMutationTransaction::new();
        transaction.reorder_member(family, third, Some(first));
        let result = transaction.apply(&mut store).unwrap();
        index.apply_transaction_result(&store, &result);

        let reordered = index
            .lower_scene(&store)
            .unwrap()
            .objects()
            .iter()
            .map(|object| (object.semantic_id, object.execution_id))
            .collect::<Vec<_>>();
        assert_eq!(
            reordered.iter().map(|entry| entry.0).collect::<Vec<_>>(),
            vec![third, first, second]
        );
        for (semantic_id, execution_id) in initial {
            assert_eq!(index.execution_object_id(semantic_id), Some(execution_id));
        }
    }

    #[test]
    fn node_added_and_removed_impacts_update_only_that_identity() {
        let mut store = SemanticStore::new();
        let mut index = SemanticExecutionIndex::new();

        let mut add = SemanticMutationTransaction::new();
        add.add_node(SemanticNodeCreation::object(circle(1.0)));
        let result = add.apply(&mut store).unwrap();
        let [SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
            panic!("expected one node-added impact");
        };
        index.apply_transaction_result(&store, &result);
        let old_id = index.execution_object_id(*node).unwrap();
        assert_eq!(index.len(), 1);

        let old_node = *node;
        let mut remove = SemanticMutationTransaction::new();
        remove.remove_node(old_node);
        let result = remove.apply(&mut store).unwrap();
        index.apply_transaction_result(&store, &result);
        assert_eq!(index.execution_object_id(old_node), None);
        assert!(index.is_empty());

        let replacement = store.insert_semantic_object(circle(2.0));
        assert_eq!(replacement.slot(), old_node.slot());
        assert_ne!(replacement.generation(), old_node.generation());
        store.attach_to_scene(replacement).unwrap();
        let new_id = index.lower_scene(&store).unwrap().objects()[0].execution_id;
        assert_ne!(new_id, old_id);
    }

    #[test]
    fn lowering_failure_does_not_partially_update_identity_index() {
        use noon_core::{GeometryRef, ObjectDefinition};

        let mut store = SemanticStore::new();
        attach(&mut store, circle(1.0));
        let legacy = store.insert_object(ObjectDefinition::new(
            ObjectId::new(99),
            GeometryRef::circle(1.0),
        ));
        store.attach_to_scene(legacy).unwrap();

        let mut index = SemanticExecutionIndex::new();
        assert_eq!(
            index.lower_scene(&store).unwrap_err(),
            SemanticLoweringError::MissingSemanticObjectState(legacy)
        );
        assert!(index.is_empty());
    }
}
