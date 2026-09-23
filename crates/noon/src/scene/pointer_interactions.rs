use crate::{AuthoringError, Mobject, Scene};
use noon_core::{
    PointerIndicateOptions, PointerZoomOptions, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticObjectRole, SemanticPointerInteractions, SemanticPointerZoom, SemanticSignalValue,
    SemanticTransactionNodeRef,
};

impl Scene {
    /// Author root-wide click -> Indicate and/or wheel -> camera bindings before
    /// execution starts. Only the shared precise analytic-fill picker chooses
    /// click targets. Native input needs no Python callback after this declaration.
    /// Reconfiguration of an already-live binding is deliberately not supported.
    pub fn configure_pointer_interactions(
        &mut self,
        indicate: Option<PointerIndicateOptions>,
        zoom: Option<(&Mobject, PointerZoomOptions)>,
    ) -> Result<(), AuthoringError> {
        if self.execution.is_some() {
            return Err(AuthoringError::PointerInteractions(
                "configure pointer bindings before execution",
            ));
        }
        let mut store = self.store.borrow_mut();
        if store
            .node(self.root)
            .expect("scene root")
            .pointer_interactions()
            .enabled()
        {
            return Err(AuthoringError::PointerInteractions(
                "pointer bindings are already configured",
            ));
        }
        let mut transaction = SemanticMutationTransaction::new();
        let zoom = if let Some((camera, options)) = zoom {
            if !std::rc::Rc::ptr_eq(&self.store, camera.integration_store()) {
                return Err(AuthoringError::ForeignStore);
            }
            let state = store.semantic_object_state_checked(camera.node_id())?;
            if state.role() != SemanticObjectRole::Camera2D {
                return Err(AuthoringError::PointerInteractions(
                    "zoom target must be the Scene camera frame",
                ));
            }
            if state.signal_bindings().iter().any(|b| {
                matches!(
                    b.property(),
                    noon_core::SemanticObjectProperty::Translation
                        | noon_core::SemanticObjectProperty::Scale
                )
            }) {
                return Err(AuthoringError::PointerInteractions(
                    "zoom cannot replace another camera transform driver",
                ));
            }
            let center = transaction.create_node(SemanticNodeCreation::input_signal(
                SemanticSignalValue::Vec3(state.transform.translation),
            )?);
            let scale = transaction.create_node(SemanticNodeCreation::input_signal(
                SemanticSignalValue::Vec3(state.transform.scale),
            )?);
            transaction
                .scope_signal(self.root, center)
                .scope_signal(self.root, scale);
            Some(SemanticPointerZoom {
                camera: SemanticTransactionNodeRef::from(camera.node_id()),
                center_signal: center.into(),
                scale_signal: scale.into(),
                options,
            })
        } else {
            None
        };
        let bindings = SemanticPointerInteractions { indicate, zoom };
        bindings
            .validate()
            .map_err(AuthoringError::PointerInteractions)?;
        transaction.set_pointer_interactions(self.root, bindings);
        transaction.apply(&mut store)?;
        Ok(())
    }
}
