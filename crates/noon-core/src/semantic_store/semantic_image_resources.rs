//! Immutable image admission in the authoritative semantic resource namespace.
use std::sync::Arc;

use super::SemanticStore;
use crate::{RasterImageResourceArena, RasterImageResourceError, RasterImageResourceHandle};

impl SemanticStore {
    pub fn raster_image_resources(&self) -> &RasterImageResourceArena {
        &self.raster_image_resources
    }

    /// Admit normalized pixels for one atomic semantic/execution publication.
    ///
    /// A failed closure (including unwinding) discards only newly admitted
    /// content. A duplicated resource was already published and remains live.
    pub fn with_raster_image_rgba8<T, E>(
        &mut self,
        width: u32,
        height: u32,
        rgba8: impl Into<Arc<[u8]>>,
        publish: impl FnOnce(&mut Self, RasterImageResourceHandle) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<RasterImageResourceError>,
    {
        let (handle, fresh) = self
            .raster_image_resources
            .admit_rgba8(width, height, rgba8)?;
        let mut admission = ImageAdmission {
            store: self,
            handle,
            fresh,
            committed: false,
        };
        let result = publish(admission.store, handle);
        admission.committed = result.is_ok();
        result
    }
}

struct ImageAdmission<'a> {
    store: &'a mut SemanticStore,
    handle: RasterImageResourceHandle,
    fresh: bool,
    committed: bool,
}

impl Drop for ImageAdmission<'_> {
    fn drop(&mut self) {
        if self.fresh && !self.committed {
            self.store
                .raster_image_resources
                .discard_unpublished(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RasterImageSampling, SemanticImageContent, SemanticMutationImpact,
        SemanticMutationTransaction, SemanticMutationTransactionError, SemanticNodeCreation,
        SemanticNodeId, SemanticObjectState, StoredGeometry,
    };

    #[derive(Debug, PartialEq)]
    enum Error {
        Image(RasterImageResourceError),
        Semantic(SemanticMutationTransactionError),
        Preparation,
    }

    impl From<RasterImageResourceError> for Error {
        fn from(value: RasterImageResourceError) -> Self {
            Self::Image(value)
        }
    }

    impl From<SemanticMutationTransactionError> for Error {
        fn from(value: SemanticMutationTransactionError) -> Self {
            Self::Semantic(value)
        }
    }

    fn create(store: &mut SemanticStore, pixels: Arc<[u8]>) -> SemanticNodeId {
        store
            .with_raster_image_rgba8(2, 1, pixels, |store, handle| {
                let mut transaction = SemanticMutationTransaction::new();
                transaction.add_node(SemanticNodeCreation::object(SemanticObjectState::new(
                    SemanticImageContent::with_sampling(handle, RasterImageSampling::Nearest),
                )));
                let result = transaction.apply(store)?;
                let [SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
                    panic!("one node")
                };
                Ok::<_, Error>(*node)
            })
            .unwrap()
    }

    #[test]
    fn identical_images_share_pixels_but_retain_distinct_semantic_identity() {
        let mut store = SemanticStore::new();
        let first = create(&mut store, Arc::from([17; 8]));
        let second = create(&mut store, Arc::from([17; 8]));

        assert_ne!(first, second);
        assert_eq!(
            store.semantic_object_state_checked(first).unwrap().content,
            store.semantic_object_state_checked(second).unwrap().content
        );
        assert_eq!(store.raster_image_resources().len(), 1);
        assert_eq!(store.raster_image_resources().stats().pixel_bytes, 8);
    }

    #[test]
    fn failed_fresh_admission_releases_pixels_and_does_not_reuse_handle() {
        let mut store = SemanticStore::new();
        let before = store.raster_image_resources().stats();
        let pixels: Arc<[u8]> = Arc::from([29; 8]);
        let weak = Arc::downgrade(&pixels);
        let mut failed = None;
        let result = store.with_raster_image_rgba8(2, 1, pixels, |_, handle| {
            failed = Some(handle);
            Err::<(), _>(Error::Preparation)
        });

        assert_eq!(result, Err(Error::Preparation));
        let failed = failed.unwrap();
        assert!(weak.upgrade().is_none());
        assert!(store.raster_image_resources().get(failed).is_none());
        assert_eq!(store.raster_image_resources().stats(), before);

        let retry = create(&mut store, Arc::from([29; 8]));
        let retry_resource = store
            .semantic_object_state_checked(retry)
            .unwrap()
            .content
            .image()
            .unwrap()
            .resource();
        assert_ne!(failed, retry_resource);
    }

    #[test]
    fn failed_duplicate_admission_preserves_the_existing_resource() {
        let mut store = SemanticStore::new();
        let image = create(&mut store, Arc::from([31; 8]));
        let handle = store
            .semantic_object_state_checked(image)
            .unwrap()
            .content
            .image()
            .unwrap()
            .resource();
        let before = store.raster_image_resources().stats();

        let result = store.with_raster_image_rgba8(2, 1, Arc::<[u8]>::from([31; 8]), |_, got| {
            assert_eq!(got, handle);
            Err::<(), _>(Error::Preparation)
        });

        assert_eq!(result, Err(Error::Preparation));
        assert_eq!(store.raster_image_resources().stats(), before);
        assert!(store.raster_image_resources().get(handle).is_some());
    }

    #[test]
    fn invalid_input_does_not_run_publication_or_change_resource_state() {
        let mut store = SemanticStore::new();
        create(&mut store, Arc::from([17; 8]));
        let before = store.raster_image_resources().stats();
        let revision = store.scene_revision();
        for (width, height, pixels) in [
            (0, 1, vec![]),
            (2, 1, vec![0; 7]),
            (u32::MAX, u32::MAX, vec![]),
        ] {
            let result =
                store.with_raster_image_rgba8(width, height, pixels, |_, _| -> Result<(), Error> {
                    panic!("invalid pixels must fail before publication")
                });
            assert!(matches!(result, Err(Error::Image(_))));
            assert_eq!(store.raster_image_resources().stats(), before);
            assert_eq!(store.scene_revision(), revision);
        }
    }

    #[test]
    fn semantic_failure_after_admission_discards_only_fresh_content() {
        let mut store = SemanticStore::new();
        let existing = create(&mut store, Arc::from([17; 8]));
        let existing_state = store
            .semantic_object_state_checked(existing)
            .unwrap()
            .clone();
        let before = store.raster_image_resources().stats();
        let pixels: Arc<[u8]> = Arc::from([29; 8]);
        let weak = Arc::downgrade(&pixels);
        let mut failed = None;
        let result = store.with_raster_image_rgba8(2, 1, pixels, |store, handle| {
            failed = Some(handle);
            let mut state = SemanticObjectState::new(SemanticImageContent::new(handle));
            state.transform.rotation_z = f64::NAN;
            let mut transaction = SemanticMutationTransaction::new();
            transaction.add_node(SemanticNodeCreation::object(state));
            transaction.apply(store).map_err(Error::from)
        });
        assert!(matches!(result, Err(Error::Semantic(_))));
        assert!(weak.upgrade().is_none());
        assert!(store
            .raster_image_resources()
            .get(failed.unwrap())
            .is_none());
        assert_eq!(store.raster_image_resources().stats(), before);
        assert_eq!(
            store.semantic_object_state_checked(existing).unwrap(),
            &existing_state
        );
    }

    #[test]
    fn preflight_only_failure_releases_pixels() {
        let mut store = SemanticStore::new();
        let pixels: Arc<[u8]> = Arc::from([23; 8]);
        let weak = Arc::downgrade(&pixels);
        let result = store.with_raster_image_rgba8(2, 1, pixels, |store, handle| {
            let mut transaction = SemanticMutationTransaction::new();
            transaction.add_node(SemanticNodeCreation::object(SemanticObjectState::new(
                SemanticImageContent::new(handle),
            )));
            let _prepared = transaction.prepare(store)?;
            Err::<(), _>(Error::Preparation)
        });
        assert_eq!(result, Err(Error::Preparation));
        assert!(weak.upgrade().is_none());
        assert!(store.raster_image_resources().is_empty());
    }

    #[test]
    fn unwinding_publication_does_not_retain_fresh_pixels() {
        let mut store = SemanticStore::new();
        let pixels: Arc<[u8]> = Arc::from([37; 8]);
        let weak = Arc::downgrade(&pixels);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = store.with_raster_image_rgba8(2, 1, pixels, |_, _| -> Result<(), Error> {
                panic!("publication preparation panic")
            });
        }));
        assert!(result.is_err());
        assert!(weak.upgrade().is_none());
        assert!(store.raster_image_resources().is_empty());
    }

    #[test]
    fn clone_remaps_handles_and_shares_immutable_pixels() {
        let mut original = SemanticStore::new();
        let object = create(&mut original, Arc::from([41; 8]));
        let cloned = original.clone();
        let source = original
            .semantic_object_state_checked(object)
            .unwrap()
            .content
            .image()
            .unwrap();
        let target = cloned
            .semantic_object_state_checked(object)
            .unwrap()
            .content
            .image()
            .unwrap();
        assert_eq!(source.sampling(), target.sampling());
        assert_ne!(source.resource().arena, target.resource().arena);
        assert!(original
            .raster_image_resources()
            .get(target.resource())
            .is_none());
        assert!(cloned
            .raster_image_resources()
            .get(source.resource())
            .is_none());
        assert!(Arc::ptr_eq(
            &original
                .raster_image_resources()
                .get_shared(source.resource())
                .unwrap(),
            &cloned
                .raster_image_resources()
                .get_shared(target.resource())
                .unwrap(),
        ));
        assert_eq!(
            original.raster_image_resources().stats(),
            cloned.raster_image_resources().stats()
        );
    }

    #[test]
    fn foreign_or_stale_image_replacement_fails_before_touching_object() {
        let mut source = SemanticStore::new();
        let image = create(&mut source, Arc::from([43; 8]));
        let handle = source
            .semantic_object_state_checked(image)
            .unwrap()
            .content
            .image()
            .unwrap()
            .resource();
        let mut target = SemanticStore::new();
        let object =
            target.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let before = target
            .semantic_object_state_checked(object)
            .unwrap()
            .clone();
        for resource in [
            handle,
            RasterImageResourceHandle {
                version: 1,
                ..handle
            },
        ] {
            let mut transaction = SemanticMutationTransaction::new();
            transaction.replace_content(object, SemanticImageContent::new(resource));
            assert!(matches!(
                transaction.apply(&mut target),
                Err(SemanticMutationTransactionError::InvalidImageResource { .. })
            ));
            assert_eq!(
                target.semantic_object_state_checked(object).unwrap(),
                &before
            );
        }
    }

    #[test]
    fn transform_to_allows_same_image_resource_and_rejects_resource_morph() {
        let mut store = SemanticStore::new();
        let source = create(&mut store, Arc::from([47; 8]));
        let same_resource_target = create(&mut store, Arc::from([47; 8]));
        let different_resource_target = create(&mut store, Arc::from([53; 8]));

        let mut affine = SemanticMutationTransaction::new();
        affine.create_transform_animation(
            source,
            same_resource_target,
            crate::AnimationOptions::new(),
        );
        assert!(affine.apply(&mut store).is_ok());

        let mut morph = SemanticMutationTransaction::new();
        morph.create_transform_animation(
            source,
            different_resource_target,
            crate::AnimationOptions::new(),
        );
        assert!(matches!(
            morph.apply(&mut store),
            Err(SemanticMutationTransactionError::UnsupportedImageAnimation { .. })
        ));
    }

    #[test]
    fn image_recolor_or_stroke_style_is_rejected_during_transaction_preflight() {
        let mut store = SemanticStore::new();
        let result =
            store.with_raster_image_rgba8(2, 1, Arc::<[u8]>::from([61; 8]), |store, handle| {
                let mut state = SemanticObjectState::new(SemanticImageContent::new(handle));
                state.style.fill = Some(crate::SemanticPaint::Solid(crate::Color::rgba(
                    0.5, 1.0, 1.0, 1.0,
                )));
                let mut transaction = SemanticMutationTransaction::new();
                transaction.add_node(SemanticNodeCreation::object(state));
                transaction.apply(store).map_err(Error::from)
            });
        assert!(matches!(
            result,
            Err(Error::Semantic(
                SemanticMutationTransactionError::UnsupportedImageStyle { .. }
            ))
        ));
        assert!(store.raster_image_resources().is_empty());
    }
}
