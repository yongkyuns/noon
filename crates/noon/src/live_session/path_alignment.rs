use super::*;
impl LiveSession<'_> {
    /// Align two persistent paths from one coherent current-state capture.
    pub fn align_points(
        &mut self,
        left: &Mobject,
        right: &Mobject,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(left)?;
        self.require_mobject(right)?;
        if left.node_id() == right.node_id() {
            return Ok(());
        }
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let a = self.capture_mobject_state(left)?;
        let b = self.capture_mobject_state(right)?;
        let mut store = self.store.borrow_mut();
        crate::path_alignment::prepare_alignment(&store, (left.node_id(), a), (right.node_id(), b))?
            .publish(&mut store, |store, transaction| {
                self.session
                    .apply_semantic_transaction_at_root(store, self.root, transaction)
                    .map(|_| ())
                    .map_err(LiveSessionError::from)
            })
    }
}
