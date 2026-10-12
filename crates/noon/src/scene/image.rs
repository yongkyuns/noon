use super::*;
use crate::{image_authoring::publish_image_creation, ImageMobjectOptions};

impl Scene {
    /// Construct a detached immutable image, including after execution bootstrap.
    /// No execution row is allocated until Add/FadeIn admits the object.
    pub fn image(&mut self, options: ImageMobjectOptions) -> Result<Mobject, AuthoringError> {
        let mut store = self.store.borrow_mut();
        let node = if let Some(execution) = self.execution.as_mut() {
            execution.require_resource_creation_at_root(&store, self.root)?;
            publish_image_creation(&mut store, options, |store, transaction| {
                execution
                    .apply_semantic_transaction_at_root(store, self.root, transaction)
                    .map_err(AuthoringError::from)
            })?
        } else {
            publish_image_creation(&mut store, options, |store, transaction| {
                transaction.apply(store).map_err(AuthoringError::from)
            })?
        };
        drop(store);
        Mobject::from_node(Rc::clone(&self.store), node)
    }

    pub fn image_rgba8(
        &mut self,
        width: u32,
        height: u32,
        pixels: impl Into<std::sync::Arc<[u8]>>,
    ) -> Result<Mobject, AuthoringError> {
        self.image(ImageMobjectOptions::rgba8(width, height, pixels)?)
    }
}
