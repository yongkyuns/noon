use super::*;
use crate::InstalledRetainedExecutionMirror;
use noon::{ImageMapping, ImagePaintOptions, ImageUvTransform};
use std::sync::Arc;

#[test]
fn worker_fill_only_snapshot_installs_pixels_and_remaps_the_shared_capability() {
    let mut scene = noon::Scene::new();
    let mut source = scene.circle(1.0).unwrap();
    source
        .set_fill_image(
            ImagePaintOptions::rgba8(1, 1, [17, 73, 191, 128]).unwrap(),
            Some(0.75),
        )
        .unwrap();
    scene.add(&source).unwrap();
    let native = source
        .state()
        .unwrap()
        .style
        .fill
        .as_ref()
        .unwrap()
        .as_image()
        .unwrap()
        .resource();
    let mut player =
        SemanticExecutionPlayer::from_scene_execution(scene.into_execution().unwrap(), 4.0, 73)
            .unwrap();
    let bytes = player.resource_bundle_bytes();
    let bundle = RetainedResourceBundle::decode_binary(&bytes).unwrap();
    assert_eq!(bundle.image_count(), 1);
    assert_eq!(bundle.image_bytes(), 4);
    let delta = player.delta(true).unwrap().unwrap();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply_family(delta).unwrap();
    let image = *mirror.frame().unwrap().objects[0]
        .fill_paint
        .as_ref()
        .unwrap()
        .as_image()
        .unwrap();
    assert_ne!(image.resource().arena, native.arena);
    assert_eq!(
        mirror
            .resources()
            .images()
            .get(image.resource())
            .unwrap()
            .rgba8(),
        &[17, 73, 191, 128]
    );
}

#[test]
fn worker_bitmap_and_fill_share_pixels_and_retire_only_after_both_roots_leave() {
    let mut scene = noon::Scene::new();
    let bitmap = scene
        .image_rgba8(2, 1, [255, 0, 0, 255, 0, 255, 255, 0])
        .unwrap();
    let mut source = scene.circle(1.0).unwrap();
    source
        .set_fill_image(ImagePaintOptions::from_image(&bitmap).unwrap(), Some(1.0))
        .unwrap();
    scene.add(&bitmap).unwrap();
    scene.add(&source).unwrap();
    let mut player =
        SemanticExecutionPlayer::from_scene_execution(scene.into_execution().unwrap(), 4.0, 74)
            .unwrap();
    let bytes = player.resource_bundle_bytes();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    let initial = player.delta(true).unwrap().unwrap();
    mirror.apply_family(initial.clone()).unwrap();
    assert_eq!(mirror.resources().image_count(), 1);
    let frame = mirror.frame().unwrap();
    assert_eq!(
        frame.objects[0].content.image().unwrap().resource(),
        frame.objects[1]
            .fill_paint
            .as_ref()
            .unwrap()
            .as_image()
            .unwrap()
            .resource()
    );
    let old_pixels = mirror
        .resources()
        .images()
        .get(frame.objects[0].content.image().unwrap().resource())
        .unwrap()
        .clone();
    let transport = initial.retained.objects[1]
        .fill_paint
        .as_ref()
        .unwrap()
        .as_image()
        .unwrap()
        .resource();

    let mut bad = initial.clone();
    bad.retained.snapshot = false;
    bad.retained.sequence = 1;
    bad.retained.painter_order = None;
    bad.retained.objects = vec![bad.retained.objects[1].clone()];
    let image = *bad.retained.objects[0]
        .fill_paint
        .as_ref()
        .unwrap()
        .as_image()
        .unwrap();
    let mut missing = image.resource();
    missing.version += 1;
    bad.retained.objects[0].fill_paint = Some(Arc::new(noon_core::SemanticPaint::Image(Arc::new(
        noon_core::ImagePaint::new(missing, image.mapping(), image.sampling()),
    ))));
    let before = mirror.frame().unwrap().clone();
    assert!(mirror.apply_family(bad).is_err());
    assert_eq!(mirror.frame().unwrap(), &before);
    let mut premature = initial.clone();
    premature.retained.snapshot = false;
    premature.retained.sequence = 1;
    premature.retained.objects.clear();
    premature.retained.painter_order = None;
    premature.resource_retirements.images.push(transport.into());
    assert!(mirror.apply_family(premature).is_err());
    assert_eq!(mirror.frame().unwrap(), &before);

    player
        .with_live_session(|live| live.remove(&bitmap))
        .unwrap();
    let delta = player.delta(false).unwrap().unwrap();
    assert!(delta.resource_retirements.images.is_empty());
    mirror.apply_family(delta).unwrap();
    assert_eq!(mirror.resources().image_count(), 1);
    let mapping = ImageMapping::new(
        noon::ImageFit::Cover,
        [0.5; 2],
        noon::ImageAddressMode::Clamp,
        ImageUvTransform::new([0.173, -0.231], [0.63, 1.37], 0.37).unwrap(),
    )
    .unwrap();
    player
        .with_live_session(|live| live.set_fill_image_mapping(&source, mapping, Some(0.5)))
        .unwrap();
    let delta = player.delta(false).unwrap().unwrap();
    assert!(delta.resource_additions.is_none());
    assert!(delta.resource_retirements.images.is_empty());
    mirror.apply_family(delta).unwrap();
    assert_eq!(mirror.resources().image_count(), 1);
    let index = mirror
        .frame()
        .unwrap()
        .objects
        .iter()
        .position(|row| row.id == noon_compile::semantic_execution_object_id(source.node_id()))
        .unwrap();
    assert_eq!(
        mirror.frame().unwrap().objects[index]
            .fill_paint
            .as_ref()
            .unwrap()
            .as_image()
            .unwrap()
            .mapping(),
        mapping
    );
    player
        .with_live_session(|live| live.set_fill(&source, 0.2, 0.4, 0.8, 1.0))
        .unwrap();
    let delta = player.delta(false).unwrap().unwrap();
    assert_eq!(delta.resource_retirements.images, [transport.into()]);
    mirror.apply_family(delta).unwrap();
    assert_eq!(mirror.resources().image_count(), 0);
    assert_eq!(old_pixels.rgba8(), &[255, 0, 0, 255, 0, 255, 255, 0]);
}
