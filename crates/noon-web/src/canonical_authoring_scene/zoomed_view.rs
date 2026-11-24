//! Inset authoring delegates to the current scene or its existing live player.
use super::*;

impl CanonicalAuthoringScene {
    /// Create the two detached ordinary objects for one shared retained inset.
    pub fn create_zoomed_view(
        &mut self,
        camera_frame_id: ObjectId,
        display_id: ObjectId,
        options: noon::ZoomedSceneOptions,
    ) -> Result<noon::ZoomedView, AuthoringFailure> {
        if camera_frame_id == display_id
            || self.bindings.contains_key(&camera_frame_id)
            || self.bindings.contains_key(&display_id)
        {
            return Err("zoomed-view wrapper identity is already bound".into());
        }
        #[cfg(any(target_arch = "wasm32", test))]
        if !matches!(self.player_ownership, PlayerOwnership::Unstarted) || self.scene.time() != 0.0
        {
            return Err("zoomed-view declaration must precede execution".into());
        }
        let view = self
            .scene
            .zoomed_view(options)
            .map_err(AuthoringFailure::from)?;
        for (id, object) in [
            (camera_frame_id, view.camera_frame()),
            (display_id, view.display()),
        ] {
            let node = object.node_id();
            self.bindings.insert(id, node);
            self.identities.insert(node, id);
        }
        Ok(view)
    }

    /// Query the current inset ratio through the owning Scene.
    pub fn zoom_factor(&mut self, view: &noon::ZoomedView) -> Result<f64, AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(
            view.display().integration_store(),
            self.scene.integration_store(),
        ) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        #[cfg(any(target_arch = "wasm32", test))]
        if !self.player_ownership.is_unstarted() {
            return self
                .active_live_player()?
                .scene()
                .zoom_factor(view)
                .map_err(|error| AuthoringFailure::from_scene_mutation(error, true));
        }
        self.scene.zoom_factor(view).map_err(Into::into)
    }

    /// Activate an inset and its foreground membership in one Scene publication.
    pub fn activate_zooming(&mut self, view: &noon::ZoomedView) -> Result<(), AuthoringFailure> {
        #[cfg(any(target_arch = "wasm32", test))]
        {
            if !self.player_ownership.is_unstarted() {
                return self
                    .active_live_player()?
                    .scene_mut()
                    .activate_zooming(view)
                    .map_err(|error| AuthoringFailure::from_scene_mutation(error, true));
            }
            if self.scene.time() != 0.0 {
                return Err("zoom activation cannot follow pre-execution canonical timing".into());
            }
        }
        self.scene.activate_zooming(view).map_err(Into::into)
    }
}
