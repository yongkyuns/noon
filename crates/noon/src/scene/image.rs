use super::*;
use crate::{image_authoring::publish_image_creation, ImageMobjectOptions};

impl Scene {
    /// Construct a detached immutable image, including after execution bootstrap.
    /// No execution row is allocated until Add/FadeIn admits the object.
    pub fn image(&mut self, options: ImageMobjectOptions) -> Result<Mobject, AuthoringError> {
        self.require_image_resource_admission()?;
        let node = self.with_semantic_publication(|store, publish| {
            publish_image_creation(store, options, publish)
        })?;
        Mobject::from_node(Rc::clone(&self.store), node)
    }

    /// Fill an object with immutable pixels and publish its style in the same transaction.
    pub fn set_fill_image(
        &mut self,
        object: &Mobject,
        options: crate::ImagePaintOptions,
        opacity: Option<f64>,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.require_object(object)?;
        let state = object.state()?;
        self.require_image_resource_admission()?;
        self.with_semantic_publication(|store, publish| {
            crate::image_authoring::publish_image_fill(
                store,
                object.node_id(),
                state,
                options,
                opacity,
                publish,
            )
        })
    }

    /// Edit an existing image paint's UV mapping without importing its pixels again.
    pub fn set_fill_image_mapping(
        &mut self,
        object: &Mobject,
        mapping: noon_core::ImageMapping,
        opacity: Option<f64>,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.require_object(object)?;
        let state = crate::image_authoring::prepare_image_fill_mapping(object, mapping, opacity)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_style(object.node_id(), state.style);
        self.apply_semantic_transaction(transaction)
    }

    /// Change the image's sampling mode while keeping its pixel resource and identity.
    pub fn set_image_sampling(
        &mut self,
        object: &Mobject,
        sampling: noon_core::RasterImageSampling,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        let content =
            crate::image_authoring::image_content_with_sampling(&object.state()?, sampling)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_content(object.node_id(), content);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    fn require_image_resource_admission(&self) -> Result<(), AuthoringError> {
        if let Some(execution) = self.running_execution() {
            execution.require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        }
        Ok(())
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
