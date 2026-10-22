#![cfg(feature = "prepared-prefix")]
mod raster_support;
use noon::{ExecutionSession, ImageMobjectOptions, ManimBecomeOptions, PrefixReader, Scene};
use noon_runtime::ReplayLimits;

fn fixture() -> (
    Scene,
    noon::Mobject,
    ExecutionSession,
    noon::PrefixPublisher,
    PrefixReader,
) {
    let mut scene = Scene::new();
    let anchor = scene.circle(0.15).unwrap();
    scene.add(&anchor).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 256,
            payloads: 8192,
        })
        .unwrap();
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(1.0).unwrap();
    let (publisher, mut reader) = source.create_prepared_prefix(boundary).unwrap();
    reader.reserve_object_capacity(16).unwrap();
    (scene, anchor, source, publisher, reader)
}
fn image(color: [u8; 4]) -> ImageMobjectOptions {
    let mut options = ImageMobjectOptions::rgba8(2, 2, color.repeat(4)).unwrap();
    options.set_height(1.5).unwrap();
    options
}

#[test]
fn late_image_creation_replacement_and_reentry_keep_immutable_versions() {
    let (scene, _, mut source, mut publisher, mut reader) = fixture();
    let a = scene
        .live(&mut source)
        .create_image(image([240, 20, 10, 255]))
        .unwrap();
    scene.live(&mut source).add(&a).unwrap();
    let first_handle = a.state().unwrap().content.image().unwrap().resource();
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(2.0).unwrap();
    let batch = source
        .publish_prepared_prefix(boundary, &mut publisher)
        .unwrap();
    assert_eq!(batch.stats().resource_payloads, 1);
    assert_eq!(batch.stats().new_object_slots, 1);
    reader.append(batch).unwrap();
    let b = scene
        .live(&mut source)
        .create_image(image([10, 30, 240, 255]))
        .unwrap();
    let second_handle = b.state().unwrap().content.image().unwrap().resource();
    assert_ne!(first_handle, second_handle);
    scene
        .live(&mut source)
        .become_mobject(&a, &b, ManimBecomeOptions::default())
        .unwrap();
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(3.0).unwrap();
    let batch = source
        .publish_prepared_prefix(boundary, &mut publisher)
        .unwrap();
    assert_eq!(batch.stats().resource_payloads, 1);
    reader.append(batch).unwrap();
    scene.live(&mut source).remove(&a).unwrap();
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(4.0).unwrap();
    reader
        .append(
            source
                .publish_prepared_prefix(boundary, &mut publisher)
                .unwrap(),
        )
        .unwrap();
    scene.live(&mut source).add(&a).unwrap();
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(5.0).unwrap();
    let batch = source
        .publish_prepared_prefix(boundary, &mut publisher)
        .unwrap();
    assert_eq!(batch.stats().new_object_slots, 0);
    assert_eq!(
        batch.stats().resource_payloads,
        0,
        "re-entry reuses delivered immutable resource"
    );
    reader.append(batch).unwrap();
    drop(a);
    drop(b);
    drop(scene);
    drop(source);
    drop(publisher);
    for (time, handle) in [
        (4.5, second_handle),
        (1.5, first_handle),
        (2.5, second_handle),
        (1.5, first_handle),
    ] {
        reader.seek(time).unwrap();
        let publication = reader.take_renderer_publication();
        let pixels = publication.raster_image_resources().get(handle).unwrap();
        assert_eq!(pixels.width(), 2);
        assert!(publication
            .painter_order()
            .iter()
            .any(|&i| publication.frame().objects[i as usize]
                .content
                .image()
                .is_some_and(|image| image.resource() == handle)));
    }
    reader.seek(3.5).unwrap();
    assert_eq!(reader.take_renderer_publication().painter_order().len(), 1);
}

#[test]
#[ignore = "requires Vulkan; explicitly invoked by focused qualification"]
fn lifecycle_resource_pixels_match_source_after_appends_drop_and_renderer_recreation() {
    pollster::block_on(async {
        let mut scene = Scene::new();
        let anchor = scene.circle(0.15).unwrap();
        scene.add(&anchor).unwrap();
        let mut source = scene.execution_session().unwrap();
        source
            .begin_replay_retention(ReplayLimits {
                revisions: 256,
                payloads: 8192,
            })
            .unwrap();
        let first = source.wait_segment(1.0).unwrap();
        let mut oracle = raster_support::Raster::new().await;
        let mut raster = raster_support::Raster::new().await;
        source.advance_to(0.5).unwrap();
        let mut expected = vec![(0.5, oracle.capture(&source.take_renderer_publication()))];
        source.advance_to(1.0).unwrap();
        let (mut publisher, mut reader) = source.create_prepared_prefix(first).unwrap();
        reader.reserve_object_capacity(16).unwrap();
        let a = scene
            .live(&mut source)
            .create_image(image([230, 35, 15, 255]))
            .unwrap();
        scene.live(&mut source).add(&a).unwrap();
        let b = scene
            .live(&mut source)
            .create_image(image([20, 80, 240, 255]))
            .unwrap();
        for step in 1..=4 {
            match step {
                2 => {
                    scene
                        .live(&mut source)
                        .become_mobject(&a, &b, ManimBecomeOptions::default())
                        .unwrap();
                }
                3 => {
                    scene.live(&mut source).remove(&a).unwrap();
                }
                4 => {
                    scene.live(&mut source).add(&a).unwrap();
                }
                _ => {}
            }
            let boundary = source.wait_segment(1.0).unwrap();
            source.advance_to(step as f64 + 0.5).unwrap();
            expected.push((
                source.frame().time,
                oracle.capture(&source.take_renderer_publication()),
            ));
            source.advance_to(boundary.end_time()).unwrap();
            reader
                .append(
                    source
                        .publish_prepared_prefix(boundary, &mut publisher)
                        .unwrap(),
                )
                .unwrap();
            for (time, pixels) in expected.iter().rev() {
                reader.seek(*time).unwrap();
                assert_eq!(&raster.capture(&reader.take_renderer_publication()), pixels);
            }
        }
        assert_ne!(
            expected[1].1, expected[2].1,
            "version change must visibly change pixels"
        );
        drop(a);
        drop(b);
        drop(source);
        drop(scene);
        drop(publisher);
        for _ in 0..4 {
            for (time, pixels) in expected.iter().rev() {
                reader.seek(*time).unwrap();
                assert_eq!(&raster.capture(&reader.take_renderer_publication()), pixels);
            }
        }
        let mut fresh = raster_support::Raster::new().await;
        reader.seek(1.5).unwrap();
        assert_eq!(
            fresh.capture(&reader.take_renderer_publication()),
            expected[1].1
        );
        eprintln!("PREFIX_LIFECYCLE_RASTER: 35 historical comparisons passed across late image creation, replacement, removal, reentry, producer destruction, fresh renderer");
    });
}

#[test]
#[ignore = "requires Vulkan; explicitly invoked by focused qualification"]
fn terminal_endpoint_image_pixels_match_source_after_drop_and_renderer_recreation() {
    pollster::block_on(async {
        let mut scene = Scene::new();
        let anchor = scene.circle(0.15).unwrap();
        scene.add(&anchor).unwrap();
        let mut source = scene.execution_session().unwrap();
        source
            .begin_replay_retention(ReplayLimits {
                revisions: 128,
                payloads: 2048,
            })
            .unwrap();
        let boundary = source.wait_segment(1.0).unwrap();
        source.advance_to(1.0).unwrap();
        let (mut publisher, mut reader) = source.create_prepared_prefix(boundary).unwrap();
        reader.seek(0.5).unwrap();
        let mut raster = raster_support::Raster::new().await;
        let before = raster.capture(&reader.take_renderer_publication());
        let image = scene
            .live(&mut source)
            .create_image(image([230, 35, 15, 255]))
            .unwrap();
        scene.live(&mut source).add(&image).unwrap();
        source.seal_replay().unwrap();
        let mut oracle = raster_support::Raster::new().await;
        let endpoint = oracle.capture(&source.take_renderer_publication());
        assert_ne!(
            before, endpoint,
            "terminal resource must visibly change the frame"
        );
        let terminal = source
            .finish_prepared_prefix(boundary, &mut publisher)
            .unwrap();
        assert_eq!(terminal.stats().resource_payloads, 1);
        assert_eq!(terminal.stats().new_object_slots, 1);
        reader
            .reserve_object_capacity(terminal.required_object_capacity())
            .unwrap();
        reader.append(terminal).unwrap();
        assert_eq!(reader.end_time(), Some(1.0));
        assert_eq!(raster.capture(&reader.take_renderer_publication()), before);
        drop(image);
        drop(anchor);
        drop(scene);
        drop(source);
        drop(publisher);
        for time in [1.0, 0.5, 1.0, 0.0, 1.0] {
            reader.seek(time).unwrap();
            let expected = if time == 1.0 { &endpoint } else { &before };
            assert_eq!(&raster.capture(&reader.take_renderer_publication()), expected);
        }
        let mut fresh = raster_support::Raster::new().await;
        assert_eq!(fresh.capture(&reader.take_renderer_publication()), endpoint);
        eprintln!("PREFIX_TERMINAL_RASTER: exact endpoint image, unchanged append frame, repeated rewind, producer destruction, fresh renderer");
    });
}
