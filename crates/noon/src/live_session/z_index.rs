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
}
