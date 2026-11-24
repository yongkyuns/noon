//! Identity for interaction with the selected execution projection.
//!
//! ExecutionSession is constructed only through canonical semantic lowering. Its
//! retained object keys already encode the original semantic slot and generation;
//! no second identity map or history is needed to recover a deleted past target.

use noon_compile::{semantic_execution_node_id, semantic_execution_object_id};
use noon_core::{ObjectId, SemanticNodeId};

use crate::ExecutionSession;

impl ExecutionSession {
    /// Resolve only a member of the currently evaluated execution revision.
    ///
    /// A sealed history may contain an object deleted from the current semantic
    /// store. Decoding its canonical key is valid here because the session owns
    /// its originating execution projection. Raw runtime keys are not admitted.
    /// This is not a presentation receipt or permission to mutate a past object.
    pub(in crate::execution_session) fn interaction_semantic_object_id(
        &self,
        object: ObjectId,
    ) -> Option<SemanticNodeId> {
        if !self.runtime.contains_object(object) {
            return None;
        }
        if self.runtime.replay_is_sealed() {
            Some(semantic_execution_node_id(object))
        } else {
            self.execution_index.semantic_object_id(object)
        }
    }

    /// The inverse interaction lookup uses the same selected membership proof.
    ///
    /// Keep ordinary authoring identity and reachability queries unchanged: an
    /// earlier displayed identity does not restore a deleted source declaration.
    /// Future rows and another generation of the same slot cannot satisfy this
    /// lookup until their own creation revision is selected.
    pub(in crate::execution_session) fn interaction_execution_object_id(
        &self,
        node: SemanticNodeId,
    ) -> Option<ObjectId> {
        let object = if self.runtime.replay_is_sealed() {
            semantic_execution_object_id(node)
        } else {
            self.execution_index.execution_object_id(node)?
        };
        self.runtime.contains_object(object).then_some(object)
    }
}

#[cfg(test)]
mod tests;
