use super::{LiveSession, LiveSessionError};
use crate::LayoutAnchor;
use std::rc::Rc;

impl LiveSession<'_> {
    /// Read a reachable leaf from the latest coherent runtime publication.
    /// Detached objects and non-rendered family roots retain authored priority.
    pub fn z_index(&self, source: &LayoutAnchor) -> Result<f64, LiveSessionError> {
        if !Rc::ptr_eq(self.store, source.integration_store()) {
            return Err(crate::AuthoringError::ForeignStore.into());
        }
        let node = source.resolve()?;
        let store = self.store.borrow();
        self.session.require_published_store(&store)?;
        if self.session.semantic_object_is_reachable(node) {
            return Ok(self
                .session
                .effective_semantic_object(&store, node)?
                .object
                .z_index);
        }
        source.z_index().map_err(Into::into)
    }

    /// Publish priority through the same atomic scene/runtime mutation as authored edits.
    pub fn set_z_index(
        &mut self,
        source: &LayoutAnchor,
        value: f64,
        family: bool,
    ) -> Result<(), LiveSessionError> {
        let transaction =
            source.z_index_transaction(self.store.borrow().identity(), value, family)?;
        self.session.require_published_store(&self.store.borrow())?;
        self.apply(transaction).map(|_| ())
    }
}
