//! Inset queries and declarations borrow the one live semantic/runtime context.
use super::*;

impl SemanticExecutionPlayer {
    pub(crate) fn live_zoom_factor(
        &mut self,
        view: &noon::ZoomedView,
    ) -> Result<f64, AuthoringFailure> {
        self.with_live_session(|live| live.zoom_factor(view))
    }

    pub(crate) fn live_apply_semantic_transaction(
        &mut self,
        transaction: noon_core::SemanticMutationTransaction,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.apply(transaction).map(|_| ()))
    }
}
