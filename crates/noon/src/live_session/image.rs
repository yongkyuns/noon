use super::*;
use crate::{image_authoring::publish_image_creation, AuthoringError, ImageMobjectOptions};

impl LiveSession<'_> {
    /// Create a detached image through the existing atomic execution publication.
    /// Failed semantic/compiler/runtime preparation cannot retain newly admitted pixels.
    pub fn create_image(
        &mut self,
        options: ImageMobjectOptions,
    ) -> Result<Mobject, LiveSessionError> {
        let mut store = self.store.borrow_mut();
        self.session
            .require_resource_creation_at_root(&store, self.root)?;
        let node = publish_image_creation(&mut store, options, |store, transaction| {
            self.session
                .apply_semantic_transaction_at_root(store, self.root, transaction)
                .map_err(AuthoringError::from)
        })?;
        drop(store);
        Mobject::from_node(Rc::clone(self.store), node).map_err(LiveSessionError::from)
    }

    pub fn set_image_sampling(
        &mut self,
        object: &Mobject,
        sampling: noon_core::RasterImageSampling,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(object)?;
        let image = object
            .state()?
            .content
            .image()
            .ok_or(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::ImageContent,
            ))?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_content(
            object.node_id(),
            noon_core::SemanticImageContent::with_sampling(image.resource(), sampling),
        );
        self.apply(transaction).map(|_| ())
    }
}
