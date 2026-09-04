use noon_core::{ObjectId, SemanticNodeId};

use super::SemanticExecutionReachabilityUpdate;

/// Derive the temporary execution compatibility key for one authoritative semantic
/// identity.
///
/// This deliberately allocates no second identity domain. The packed key is stable
/// for the lifetime of one generational semantic identity and changes when a semantic
/// slot is reused with a new generation.
pub const fn semantic_execution_object_id(id: SemanticNodeId) -> ObjectId {
    let raw = ((id.generation() as u64) << 32) | id.slot() as u64;
    ObjectId::new(raw)
}

impl SemanticExecutionReachabilityUpdate {
    /// Execution-key view of objects whose net rooted membership changed 0 -> 1.
    ///
    /// The key is derived from the semantic identity carried by this committed
    /// reachability result rather than looked up through `SemanticExecutionIndex`.
    /// Callers therefore do not depend on whether a later `NodeRemoved` impact has
    /// already retired that index entry.
    pub fn entered_execution_objects(&self) -> impl ExactSizeIterator<Item = ObjectId> + '_ {
        self.entered_objects()
            .iter()
            .copied()
            .map(semantic_execution_object_id)
    }

    /// Execution-key view of objects whose net rooted membership changed 1 -> 0.
    pub fn exited_execution_objects(&self) -> impl ExactSizeIterator<Item = ObjectId> + '_ {
        self.exited_objects()
            .iter()
            .copied()
            .map(semantic_execution_object_id)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        SemanticMutationTransaction, SemanticObjectState, SemanticStore, StoredGeometry,
    };

    use super::*;
    use crate::{SemanticExecutionIndex, SemanticExecutionReachability};

    fn circle(radius: f32) -> SemanticObjectState {
        SemanticObjectState::new(StoredGeometry::Circle { radius })
    }

    #[test]
    fn exited_execution_key_survives_identity_index_retirement_ordering() {
        let mut store = SemanticStore::new();
        let object = store.insert_semantic_object(circle(1.0));
        store.attach_to_scene(object).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let execution_id = index.lower_scene(&store).unwrap().objects()[0].execution_id;
        let mut reachability = SemanticExecutionReachability::from_store(&store).unwrap();

        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_node(object);
        let result = transaction.apply(&mut store).unwrap();

        // Deliberately consume the deletion in the mutable identity index first. The
        // reachability result must still carry enough semantic identity to retire the
        // corresponding execution slot afterward.
        index.apply_transaction_result(&store, &result);
        assert_eq!(index.execution_object_id(object), None);

        let update = reachability
            .apply_transaction_result(&store, &result)
            .unwrap();
        assert_eq!(
            update.exited_execution_objects().collect::<Vec<_>>(),
            vec![execution_id]
        );
        assert!(update.entered_execution_objects().next().is_none());
    }

    #[test]
    fn generation_reuse_changes_execution_compatibility_key() {
        let old = SemanticNodeId::new(7, 3);
        let replacement = SemanticNodeId::new(7, 4);
        assert_ne!(
            semantic_execution_object_id(old),
            semantic_execution_object_id(replacement)
        );
    }
}
