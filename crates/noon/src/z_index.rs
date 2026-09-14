//! Shared authored painter priority; the renderer consumes only derived order.
use crate::{AuthoringError, ExecutionSession, LayoutAnchor, Mobject, MobjectFamily, Scene};
use noon_core::{
    SemanticMutationTransaction, SemanticNodeId, SemanticStore, SemanticStoreIdentity,
};
use std::{cell::RefCell, rc::Rc};

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
        let store = self.integration_store().borrow().identity();
        self.z_index_transaction(store, value, family)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn z_index_transaction(
        &self,
        store: SemanticStoreIdentity,
        value: f64,
        family: bool,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        if self.integration_store().borrow().identity() != store {
            return Err(AuthoringError::ForeignStore);
        }
        let id = self.resolve()?;
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

/// Observe one anchor's effective priority from an existing coherent execution.
///
/// Reachable semantic leaves use Runtime's current priority. Detached objects and
/// non-rendered family roots retain their authored priority.
pub fn effective_z_index(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    source: &LayoutAnchor,
) -> Result<f64, AuthoringError> {
    if !Rc::ptr_eq(store, source.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    let node = source.resolve()?;
    let store_ref = store.borrow();
    execution
        .require_published_store(&store_ref)
        .map_err(AuthoringError::from)?;
    if execution.semantic_object_is_reachable(node) {
        return execution
            .effective_semantic_object(&store_ref, node)
            .map(|observed| observed.object.z_index)
            .map_err(AuthoringError::from);
    }
    drop(store_ref);
    source.z_index()
}

/// Publish one z-index edit through an already-lowered execution component.
///
/// This is migration plumbing for legacy standalone-session callers. Durable
/// application control remains Scene-owned; the neutral operation keeps only the
/// shared transaction preparation and coherent running-publication mechanics.
pub fn publish_z_index(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    source: &LayoutAnchor,
    value: f64,
    family: bool,
) -> Result<(), AuthoringError> {
    let transaction = source.z_index_transaction(store.borrow().identity(), value, family)?;
    Scene::publish_running_transaction(store, root, execution, transaction)
        .map(|_| ())
        .map_err(AuthoringError::from)
}

impl Scene {
    /// Set authored priority through this Scene's persistent mutation path.
    ///
    /// Cold Scenes update authored state directly. Running Scenes publish the same
    /// family-aware transaction atomically through the Scene-owned execution component.
    pub fn set_z_index(
        &mut self,
        source: &LayoutAnchor,
        value: f64,
        family: bool,
    ) -> Result<(), AuthoringError> {
        let store = self.integration_store().borrow().identity();
        let transaction = source.z_index_transaction(store, value, family)?;
        self.apply_semantic_transaction(transaction).map(|_| ())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_z_index_publishes_running_edit_and_rejects_foreign_store() {
        let mut scene = Scene::new();
        let object = scene.square(1.0).unwrap();
        scene.add(&object).unwrap();
        let detached = scene.square(1.0).unwrap();
        detached.set_z_index(3.0).unwrap();
        let anchor = LayoutAnchor::from(&object);
        assert!(matches!(
            scene.effective_z_index(&anchor),
            Err(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable
            ))
        ));

        let execution = scene.execution_session().unwrap();
        scene.install_execution(execution);
        let before = scene.revision();
        scene.set_z_index(&anchor, 7.0, true).unwrap();
        assert_eq!(scene.revision().get(), before.get() + 1);
        assert_eq!(object.z_index().unwrap(), 7.0);
        assert_eq!(scene.effective_z_index(&anchor).unwrap(), 7.0);
        assert_eq!(
            scene
                .effective_z_index(&LayoutAnchor::from(&detached))
                .unwrap(),
            3.0
        );

        let foreign_scene = Scene::new();
        let foreign = foreign_scene.square(1.0).unwrap();
        assert!(matches!(
            scene.set_z_index(&LayoutAnchor::from(&foreign), 1.0, false),
            Err(AuthoringError::ForeignStore)
        ));
        assert!(matches!(
            scene.effective_z_index(&LayoutAnchor::from(&foreign)),
            Err(AuthoringError::ForeignStore)
        ));
    }
}
