//! Shared authored painter priority; the renderer consumes only derived order.
use crate::{AuthoringError, LayoutAnchor, Mobject, MobjectFamily};
use noon_core::SemanticMutationTransaction;

impl LayoutAnchor {
    /// Read the selected root's own priority, including non-rendered family roots.
    pub fn z_index(&self) -> Result<f64, AuthoringError> {
        let id = self.resolve()?;
        self.integration_store()
            .borrow()
            .node(id)
            .and_then(|node| node.presentation())
            .map(|presentation| presentation.z_index)
            .ok_or_else(|| {
                noon_core::SemanticSceneOperationError::NotSemanticAuthoringNode(id).into()
            })
    }

    /// Set priority on the selected root and optionally all unique descendants.
    pub fn set_z_index(&self, value: f64, family: bool) -> Result<(), AuthoringError> {
        self.z_index_transaction(value, family)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn z_index_transaction(
        &self,
        value: f64,
        family: bool,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        let id = self.resolve()?;
        self.z_index()?;
        let mut transaction = SemanticMutationTransaction::new();
        if family {
            for node in self
                .integration_store()
                .borrow()
                .ordered_authoring_nodes(id)?
            {
                transaction.set_z_index(node, value);
            }
        } else {
            transaction.set_z_index(id, value);
        }
        Ok(transaction)
    }
}

impl Mobject {
    pub fn z_index(&self) -> Result<f64, AuthoringError> {
        LayoutAnchor::from(self).z_index()
    }
    pub fn set_z_index(&self, value: f64) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).set_z_index(value, true)
    }
}

impl MobjectFamily {
    pub fn z_index(&self) -> Result<f64, AuthoringError> {
        LayoutAnchor::from(self).z_index()
    }
    pub fn set_z_index(&self, value: f64, family: bool) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).set_z_index(value, family)
    }
}
