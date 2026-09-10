use super::{LiveSession, LiveSessionError};
use crate::LayoutAnchor;
use std::rc::Rc;

impl LiveSession<'_> {
    /// Publish priority through the same atomic scene/runtime mutation as authored edits.
    pub fn set_z_index(
        &mut self,
        source: &LayoutAnchor,
        value: f64,
        family: bool,
    ) -> Result<(), LiveSessionError> {
        if !Rc::ptr_eq(self.store, source.integration_store()) {
            return Err(crate::AuthoringError::ForeignStore.into());
        }
        self.session.require_published_store(&self.store.borrow())?;
        let transaction = source.z_index_transaction(value, family)?;
        self.apply(transaction).map(|_| ())
    }
}
