use super::*;
use crate::{
    Color, ImageMapping, ImagePaint, RasterImageSampling, SemanticImageContent,
    SemanticMutationImpact, SemanticMutationTransaction, SemanticMutationTransactionError,
    SemanticNodeCreation, SemanticNodeId, SemanticObjectState, SemanticPaint, SemanticStyle,
    StoredGeometry,
};

fn image(handle: RasterImageResourceHandle) -> SemanticPaint {
    SemanticPaint::Image(std::sync::Arc::new(ImagePaint::new(
        handle,
        ImageMapping::default(),
        RasterImageSampling::Linear,
    )))
}

fn insert(
    store: &mut SemanticStore,
    state: SemanticObjectState,
) -> Result<SemanticNodeId, SemanticMutationTransactionError> {
    let mut tx = SemanticMutationTransaction::new();
    tx.add_node(SemanticNodeCreation::object(state));
    let result = tx.apply(store)?;
    let [SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
        panic!("one object");
    };
    Ok(*node)
}

fn painted(
    store: &mut SemanticStore,
    pixels: Arc<[u8]>,
) -> (SemanticNodeId, RasterImageResourceHandle) {
    store
        .with_raster_image_rgba8(2, 1, pixels, |store, handle| {
            let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
            state.style.fill = Some(image(handle));
            Ok::<_, Box<dyn std::error::Error>>((insert(store, state)?, handle))
        })
        .unwrap()
}

#[test]
fn bitmap_and_fill_share_one_payload_until_the_last_semantic_owner_retires() {
    let mut store = SemanticStore::new();
    let pixels: Arc<[u8]> = Arc::from([17; 8]);
    let weak = Arc::downgrade(&pixels);
    let (fill, handle) = painted(&mut store, pixels);
    let bitmap = store
        .with_raster_image_rgba8(2, 1, vec![17; 8], |store, duplicate| {
            assert_eq!(duplicate, handle);
            insert(
                store,
                SemanticObjectState::new(SemanticImageContent::new(duplicate)),
            )
            .map_err(Box::<dyn std::error::Error>::from)
        })
        .unwrap();
    assert_eq!(store.raster_image_resources().len(), 1);
    assert_eq!(store.raster_image_resources().stats().pixel_bytes, 8);
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(bitmap);
    remove.apply(&mut store).unwrap();
    assert!(weak.upgrade().is_some());
    let mut replace = SemanticMutationTransaction::new();
    replace.replace_style(fill, SemanticStyle::default());
    replace.apply(&mut store).unwrap();
    assert!(store.raster_image_resources().is_empty());
    assert!(weak.upgrade().is_none());
    let (_, later) = painted(&mut store, Arc::from([17; 8]));
    assert_ne!(handle, later);
}

#[test]
fn every_style_channel_is_a_durable_root_even_when_not_currently_visible() {
    let mut store = SemanticStore::new();
    let (node, handle) = painted(&mut store, Arc::from([31; 8]));
    let mut style = store
        .semantic_object_state_checked(node)
        .unwrap()
        .style
        .clone();
    style.stroke = Some(image(handle));
    style.background_stroke = Some(image(handle));
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_style(node, style.clone());
    tx.apply(&mut store).unwrap();
    for channel in 0..3 {
        match channel {
            0 => style.fill = None,
            1 => style.stroke = None,
            _ => style.background_stroke = None,
        }
        let mut tx = SemanticMutationTransaction::new();
        tx.replace_style(node, style.clone());
        tx.apply(&mut store).unwrap();
        assert_eq!(
            store.raster_image_resources().get(handle).is_some(),
            channel < 2
        );
    }
}

#[test]
fn family_style_ownership_survives_materialization_and_releases_on_recolor() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let handle = store
        .with_raster_image_rgba8(2, 1, vec![47; 8], |store, handle| {
            let style = SemanticStyle {
                fill: Some(image(handle)),
                ..SemanticStyle::default()
            };
            let mut tx = SemanticMutationTransaction::new();
            tx.set_family_style(family, style);
            tx.apply(store)?;
            Ok::<_, Box<dyn std::error::Error>>(handle)
        })
        .unwrap();
    assert!(store.raster_image_resources().get(handle).is_some());
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_content(family, StoredGeometry::Circle { radius: 1.0 });
    tx.apply(&mut store).unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(family)
            .unwrap()
            .style
            .fill
            .as_ref()
            .unwrap()
            .as_image()
            .unwrap()
            .resource(),
        handle
    );
    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_color(family, Color::RED);
    tx.apply(&mut store).unwrap();
    assert!(store.raster_image_resources().is_empty());
}

#[test]
fn unrendered_family_removal_and_clone_remap_cover_paint_dependencies() {
    let mut original = SemanticStore::new();
    let (object, handle) = painted(&mut original, Arc::from([59; 8]));
    let family = original.insert_family();
    let style = SemanticStyle {
        fill: Some(image(handle)),
        ..SemanticStyle::default()
    };
    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_style(family, style);
    tx.apply(&mut original).unwrap();
    let mut cloned = original.clone();
    let cloned_paint = *cloned
        .semantic_object_state_checked(object)
        .unwrap()
        .style
        .fill
        .as_ref()
        .unwrap()
        .as_image()
        .unwrap();
    let cloned_family = *cloned
        .node(family)
        .unwrap()
        .family_style()
        .unwrap()
        .fill
        .as_ref()
        .unwrap()
        .as_image()
        .unwrap();
    assert_eq!(cloned_paint, cloned_family);
    assert_ne!(cloned_paint.resource().arena, handle.arena);
    assert!(original
        .raster_image_resources()
        .get(cloned_paint.resource())
        .is_none());
    assert!(cloned.raster_image_resources().get(handle).is_none());
    assert!(Arc::ptr_eq(
        &original
            .raster_image_resources()
            .get_shared(handle)
            .unwrap(),
        &cloned
            .raster_image_resources()
            .get_shared(cloned_paint.resource())
            .unwrap()
    ));
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(object);
    tx.apply(&mut cloned).unwrap();
    assert!(cloned
        .raster_image_resources()
        .get(cloned_family.resource())
        .is_some());
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(family);
    tx.apply(&mut cloned).unwrap();
    assert!(cloned.raster_image_resources().is_empty());
    assert!(original.raster_image_resources().get(handle).is_some());
}

#[test]
fn foreign_and_stale_paint_replacement_reject_before_any_earlier_mutation_commits() {
    let mut source = SemanticStore::new();
    let (_, handle) = painted(&mut source, Arc::from([71; 8]));
    let mut store = SemanticStore::new();
    let (node, own) = painted(&mut store, Arc::from([73; 8]));
    let before = store.semantic_object_state_checked(node).unwrap().clone();
    let stats = store.raster_image_resources().stats();
    for resource in [
        handle,
        RasterImageResourceHandle {
            version: own.version + 1,
            ..own
        },
    ] {
        let mut style = before.style.clone();
        style.fill = Some(image(resource));
        let mut tx = SemanticMutationTransaction::new();
        tx.set_z_index(node, 77.0).replace_style(node, style);
        assert!(matches!(
            tx.apply(&mut store),
            Err(SemanticMutationTransactionError::InvalidPaintImageResource { .. })
        ));
        assert_eq!(store.semantic_object_state_checked(node).unwrap(), &before);
        assert_eq!(store.raster_image_resources().stats(), stats);
    }
}

#[test]
fn successful_admission_without_a_durable_owner_does_not_leak_pixels() {
    let mut store = SemanticStore::new();
    for _ in 0..32 {
        let handle = store
            .with_raster_image_rgba8(2, 1, vec![83; 8], |_, handle| {
                Ok::<_, crate::RasterImageResourceError>(handle)
            })
            .unwrap();
        assert!(store.raster_image_resources().get(handle).is_none());
        assert_eq!(store.raster_image_resources().stats().pixel_bytes, 0);
        store
            .with_raster_image_rgba8(2, 1, vec![89; 8], |store, handle| {
                let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
                state.style.fill = Some(image(handle));
                let mut tx = SemanticMutationTransaction::new();
                let pending = tx.create_node(SemanticNodeCreation::object(state));
                tx.remove_node(pending);
                tx.apply(store)?;
                Ok::<_, Box<dyn std::error::Error>>(())
            })
            .unwrap();
        assert!(store.raster_image_resources().is_empty());
    }
}

#[test]
fn failed_fresh_paint_publication_releases_only_its_provisional_pixels() {
    let mut store = SemanticStore::new();
    let (node, handle) = painted(&mut store, Arc::from([97; 8]));
    let before = store.semantic_object_state_checked(node).unwrap().clone();
    let stats = store.raster_image_resources().stats();
    let result = store.with_raster_image_rgba8(2, 1, vec![101; 8], |store, provisional| {
        let mut state = before.clone();
        state.style.fill = Some(image(provisional));
        state.style.fill_opacity = f64::NAN;
        let mut tx = SemanticMutationTransaction::new();
        tx.replace_style(node, state.style);
        tx.apply(store)?;
        Ok::<_, Box<dyn std::error::Error>>(())
    });
    assert!(result.is_err());
    assert_eq!(store.semantic_object_state_checked(node).unwrap(), &before);
    assert_eq!(store.raster_image_resources().stats(), stats);
    assert!(store.raster_image_resources().get(handle).is_some());
}

fn image_transition(handle: RasterImageResourceHandle) -> Arc<crate::PaintTransition> {
    let from = image(handle);
    let to = SemanticPaint::Image(std::sync::Arc::new(ImagePaint::new(
        handle,
        crate::ImageMapping::new(
            crate::ImageFit::Cover,
            [0.75, 0.25],
            crate::ImageAddressMode::Clamp,
            crate::ImageUvTransform::new([0.5, -0.25], [2.0, 0.5], 0.75).unwrap(),
        )
        .unwrap(),
        RasterImageSampling::Linear,
    )));
    Arc::new(
        crate::PaintTransition::new(
            Some(from),
            0.12345678912345678,
            Some(to),
            0.8765432123456789,
        )
        .unwrap(),
    )
}

#[test]
fn authored_image_track_endpoints_remain_resource_roots_and_clone_with_the_store() {
    let mut store = SemanticStore::new();
    let pixels: Arc<[u8]> = Arc::from([113; 8]);
    let weak = Arc::downgrade(&pixels);
    let (owner, handle) = painted(&mut store, pixels);
    let target = insert(
        &mut store,
        SemanticObjectState::new(StoredGeometry::Circle { radius: 2.0 }),
    )
    .unwrap();
    let transition = image_transition(handle);
    let mut tx = SemanticMutationTransaction::new();
    tx.create_object_property_track(
        target,
        crate::SemanticObjectTrackProperty::Fill,
        crate::SemanticObjectTrackValues::Paint {
            transition: transition.clone(),
        },
        crate::TrackTiming::new(0.0, 1.0, crate::RateFunction::Linear),
        Default::default(),
    );
    let result = tx.apply(&mut store).unwrap();
    let [SemanticMutationImpact::AnimationAdded { animation }] = result.impacts() else {
        panic!("one animation");
    };
    let animation = *animation;
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(owner);
    tx.apply(&mut store).unwrap();
    assert!(store.raster_image_resources().get(handle).is_some());
    let mut cloned = store.clone();
    let crate::SemanticAnimationIntent::ObjectPropertyTrack {
        values: crate::SemanticObjectTrackValues::Paint { transition: copy },
        ..
    } = cloned.semantic_animation_state(animation).unwrap().intent()
    else {
        panic!("paint track");
    };
    let copied = copy.endpoints()[0]
        .0
        .unwrap()
        .as_image()
        .unwrap()
        .resource();
    assert_ne!(copied.arena, handle.arena);
    assert_eq!(copy.endpoints()[0].1, transition.endpoints()[0].1);
    assert_eq!(copy.endpoints()[1].1, transition.endpoints()[1].1);
    let (a, alpha_a) = copy.sample(0.5).unwrap();
    let (b, alpha_b) = transition.sample(0.5).unwrap();
    assert_eq!(
        a.unwrap().as_image().unwrap().mapping(),
        b.unwrap().as_image().unwrap().mapping()
    );
    assert_eq!(alpha_a, alpha_b);
    assert!(Arc::ptr_eq(
        &store.raster_image_resources().get_shared(handle).unwrap(),
        &cloned.raster_image_resources().get_shared(copied).unwrap()
    ));
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_animation(animation);
    tx.apply(&mut store).unwrap();
    assert!(store.raster_image_resources().is_empty());
    assert!(weak.upgrade().is_some());
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_animation(animation);
    tx.apply(&mut cloned).unwrap();
    assert!(cloned.raster_image_resources().is_empty());
    assert!(weak.upgrade().is_none());
}

#[test]
fn foreign_image_track_preflight_rolls_back_earlier_edits_and_node_creation() {
    let mut source = SemanticStore::new();
    let (_, handle) = painted(&mut source, Arc::from([127; 8]));
    let mut store = SemanticStore::new();
    let target = insert(
        &mut store,
        SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }),
    )
    .unwrap();
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_z_index(target, 13.0);
    tx.create_object_property_track(
        target,
        crate::SemanticObjectTrackProperty::Fill,
        crate::SemanticObjectTrackValues::Paint {
            transition: image_transition(handle),
        },
        crate::TrackTiming::new(0.0, 1.0, crate::RateFunction::Linear),
        Default::default(),
    );
    assert!(matches!(
        tx.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidPaintImageResource { .. })
    ));
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before
    );
    assert!(store.raster_image_resources().is_empty());
}
