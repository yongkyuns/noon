//! Real lowering failure after pixel admission must leave the live publication intact.
use noon::integration::SemanticMutationTransaction;
use noon::{ExecutionSessionPublicationError, ImageMobjectOptions, Scene};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

fn image_options() -> ImageMobjectOptions {
    ImageMobjectOptions::rgba8(1, 1, vec![25, 50, 75, 128]).unwrap()
}

/// Exercise the real downstream compiler boundary, not a synthetic closure
/// error or the early stale/callback checks. The semantic transform is finite,
/// but cannot lower to the renderer's f32 rotation representation.
#[test]
fn late_compiler_failure_after_image_admission_rolls_back_and_recovers() -> TestResult {
    use noon::AuthoringError;
    use noon_core::{SemanticImageContent, SemanticNodeCreation, SemanticObjectProperty};
    use std::sync::Arc;

    for duplicate in [false, true] {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0)?;
        scene.add(&circle)?;
        let existing = scene.image(image_options())?;
        let existing_state = existing.state()?;
        let existing_handle = existing_state.content.image().unwrap().resource();
        let mut session = scene.execution_session()?;
        session.take_frame_changes();
        let context = session.publication_context();
        let frame = session.frame().clone();
        let circle_state = circle.state()?;
        let circle_execution = session.execution_object_id(circle.node_id());
        let mut store = scene.integration_store().borrow_mut();
        let before_resources = store.raster_image_resources().stats();
        let before_nodes = store.len();
        let original_pixels = store
            .raster_image_resources()
            .get_shared(existing_handle)
            .unwrap();
        let bytes: Arc<[u8]> = if duplicate {
            Arc::from([25, 50, 75, 128])
        } else {
            Arc::from([11, 22, 33, 44])
        };
        let weak = Arc::downgrade(&bytes);
        let mut admitted_handle = None;
        let result = store.with_raster_image_rgba8(1, 1, bytes, |store, handle| {
            admitted_handle = Some(handle);
            assert!(store.raster_image_resources().get(handle).is_some());
            assert_eq!(
                store.raster_image_resources().len(),
                before_resources.live_resources + usize::from(!duplicate),
            );
            let mut state = existing_state.clone();
            state.content = SemanticImageContent::new(handle).into();
            let transaction = || {
                let mut tx = SemanticMutationTransaction::new();
                tx.add_node(SemanticNodeCreation::object(state.clone()));
                tx.set_property(
                    circle.node_id(),
                    SemanticObjectProperty::RotationZ,
                    f64::MAX,
                );
                tx
            };
            // This passes semantic preflight. The subsequent error must come
            // from actual execution lowering, after the guard admitted pixels.
            drop(transaction().prepare(store)?);
            session
                .apply_semantic_transaction(store, transaction())
                .map_err(AuthoringError::from)
        });
        assert!(
            matches!(
                result,
                Err(AuthoringError::ExecutionPublication(
                    ExecutionSessionPublicationError::Lowering(_)
                ))
            ),
            "unexpected failure: {result:?}"
        );
        let rejected = admitted_handle.unwrap();
        assert!(
            weak.upgrade().is_none(),
            "rejected input bytes remain retained"
        );
        assert_eq!(store.raster_image_resources().stats(), before_resources);
        assert_eq!(store.len(), before_nodes);
        assert_eq!(store.scene_revision(), context.scene_revision());
        assert_eq!(
            store.semantic_object_state_checked(circle.node_id())?,
            &circle_state
        );
        assert_eq!(
            store.semantic_object_state_checked(existing.node_id())?,
            &existing_state
        );
        assert_eq!(
            store.raster_image_resources().get(rejected).is_some(),
            duplicate
        );
        assert!(Arc::ptr_eq(
            &original_pixels,
            &store
                .raster_image_resources()
                .get_shared(existing_handle)
                .unwrap(),
        ));
        assert_eq!(session.frame(), &frame);
        assert_eq!(session.publication_context(), context);
        assert_eq!(
            session.execution_object_id(circle.node_id()),
            circle_execution
        );
        assert!(session.take_frame_changes().is_empty());
        drop(store);

        // Retrying through the normal facade must publish a detached image,
        // deduplicate surviving pixels, and never resurrect an aborted handle.
        let retry_options = if duplicate {
            image_options()
        } else {
            ImageMobjectOptions::rgba8(1, 1, vec![11, 22, 33, 44])?
        };
        let retry = scene.live(&mut session).create_image(retry_options)?;
        let retry_handle = retry.state()?.content.image().unwrap().resource();
        if duplicate {
            assert_eq!(retry_handle, existing_handle);
        } else {
            assert_ne!(retry_handle, rejected);
            assert!(scene
                .integration_store()
                .borrow()
                .raster_image_resources()
                .get(rejected)
                .is_none());
        }
        assert!(session.execution_object_id(retry.node_id()).is_none());
        assert_eq!(
            session.execution_object_id(circle.node_id()),
            circle_execution
        );
    }
    Ok(())
}
