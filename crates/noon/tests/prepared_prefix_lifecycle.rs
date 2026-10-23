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
            assert_eq!(
                &raster.capture(&reader.take_renderer_publication()),
                expected
            );
        }
        // No seek or manufactured full delta before renderer recreation.
        let publication = reader.take_renderer_publication();
        assert!(publication.changes().is_empty());
        let context = publication.context();
        assert_cold_image_recovery(&publication, &endpoint).await;
        drop(publication);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert_eq!(reader.frame().time, 1.0);
        assert_eq!(reader.end_time(), Some(1.0));

        // A new renderer must also reconstruct an unchanged image when its
        // first publication dirties only an unrelated analytic object.
        let object = reader
            .frame()
            .objects
            .iter()
            .find(|row| {
                matches!(
                    row.content.geometry(),
                    Some(noon_core::GeometryRef::Circle { .. })
                )
            })
            .unwrap()
            .id;
        let binding = noon_core::SemanticClickIndicate::new(1.25, noon_core::Color::YELLOW, 0.5);
        let prepared = reader
            .prepare_click_indicate(object, binding)
            .unwrap()
            .unwrap();
        reader.start_transient_animation(prepared).unwrap();
        reader.advance_interactions(10.0).unwrap();
        reader.advance_interactions(10.25).unwrap();
        let publication = reader.take_renderer_publication();
        let image_index = publication
            .frame()
            .objects
            .iter()
            .position(|row| row.content.image().is_some())
            .unwrap();
        assert!(!publication.changes().is_empty());
        assert!(!publication.changes().is_all());
        assert!(!publication
            .changes()
            .object_indices()
            .contains(&image_index));
        let context = publication.context();
        let expected = raster.capture(&publication);
        assert_cold_image_recovery(&publication, &expected).await;
        drop(publication);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert!(reader.interactions_active());
        assert_eq!(reader.frame().time, 1.0);
        reader.advance_interactions(10.5).unwrap();
        assert_eq!(
            raster.capture(&reader.take_renderer_publication()),
            endpoint
        );
        eprintln!("PREFIX_TERMINAL_RASTER: exact endpoint, unchanged append, rewind, source destruction, clean/sparse cold recovery, visible projection, zero-upload warm reuse");
    });
}

async fn assert_cold_image_recovery(
    publication: &noon_runtime::RendererPublication<'_>,
    expected: &[u8],
) {
    let visible: Vec<_> = publication
        .painter_order()
        .iter()
        .map(|&index| index as usize)
        .collect();
    let mut fresh = raster_support::Raster::new().await;
    assert_eq!(fresh.capture(publication).as_slice(), expected);
    if publication.changes().is_empty() {
        assert_eq!(fresh.capture(publication).as_slice(), expected);
        assert_eq!(fresh.last_scene_upload_bytes, 0);
        assert_eq!(fresh.last_scene_repacked, 0);
    }

    let mut viewport = raster_support::Raster::new().await;
    assert_eq!(
        viewport.capture_visible(publication, &visible).as_slice(),
        expected
    );
    if publication.changes().is_empty() {
        assert_eq!(
            viewport.capture_visible(publication, &visible).as_slice(),
            expected
        );
        assert_eq!(viewport.last_scene_upload_bytes, 0);
        assert_eq!(viewport.last_scene_repacked, 0);
    }
}

#[test]
#[ignore = "requires Vulkan; explicitly invoked by focused qualification"]
fn pointer_driven_prefix_effect_pixels_restore_after_terminal_delivery_and_source_drop() {
    pollster::block_on(async {
        use noon::{AnimationOptions, IndicateOptions};
        use noon_core::{
            Color, NativeInputModifiers, NativePointerId, NativePointerInput,
            NativePointerInputKind, Vec2,
        };
        let mut scene = Scene::new();
        let mut target = scene.circle(0.65).unwrap();
        target.set_fill(0.15, 0.4, 0.9, 1.0).unwrap();
        scene.add(&target).unwrap();
        scene
            .on_click_indicate(
                &target,
                IndicateOptions::new(1.4, Color::YELLOW),
                AnimationOptions::new().run_time(0.5),
            )
            .unwrap();
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
        reader
            .configure_native_pointer_input(
                NativePointerId {
                    source: 1,
                    pointer: 1,
                },
                1,
            )
            .unwrap();
        let mut raster = raster_support::Raster::new().await;
        let baseline = raster.capture(&reader.take_renderer_publication());
        for sequence in 0..2 {
            let view = reader
                .inspection_pointer_view(1, Vec2::new(256.0, 256.0))
                .unwrap();
            let snapshot = reader.capture_pointer_frame(view).unwrap();
            let token = reader
                .pointer_input_token_for_frame(&snapshot, view)
                .unwrap();
            let position = snapshot.position(Vec2::new(128.0, 128.0)).unwrap();
            let input = NativePointerInput::new(
                sequence,
                token.pointer(),
                token.context(),
                NativeInputModifiers::default(),
                if sequence == 0 {
                    NativePointerInputKind::Press {
                        position,
                        button: 0,
                    }
                } else {
                    NativePointerInputKind::Release {
                        position,
                        button: 0,
                    }
                },
            );
            let receipt = reader.submit_native_pointer_input(&token, input).unwrap();
            if sequence == 1 {
                assert_eq!(
                    receipt.selection_click().unwrap().target(),
                    Some(target.node_id())
                );
            }
        }
        assert!(reader.interactions_active());
        let history = reader.replay_stats();
        reader.advance_interactions(10.0).unwrap();
        reader.advance_interactions(10.25).unwrap();
        let midpoint = raster.capture(&reader.take_renderer_publication());
        let changed = baseline
            .as_chunks::<4>()
            .0
            .iter()
            .zip(midpoint.as_chunks::<4>().0.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            changed > 100,
            "pointer action must visibly change its historical target"
        );
        assert_eq!(reader.frame().time, 0.5);
        assert_eq!(reader.replay_stats(), history);
        let context = reader.publication_context();
        source.seal_replay().unwrap();
        let terminal = source
            .finish_prepared_prefix(boundary, &mut publisher)
            .unwrap();
        reader.append(terminal).unwrap();
        assert_eq!(reader.publication_context(), context);
        assert_eq!(
            raster.capture(&reader.take_renderer_publication()),
            midpoint
        );
        drop(target);
        drop(scene);
        drop(source);
        drop(publisher);
        reader.advance_interactions(10.5).unwrap();
        assert_eq!(
            raster.capture(&reader.take_renderer_publication()),
            baseline
        );
        reader.advance_to(reader.end_time().unwrap()).unwrap();
        assert_eq!(
            raster.capture(&reader.take_renderer_publication()),
            baseline
        );
        let mut fresh = raster_support::Raster::new().await;
        assert_eq!(fresh.capture(&reader.take_renderer_publication()), baseline);
        eprintln!("PREFIX_POINTER_RASTER: {changed} changed midpoint pixels; exact restoration after terminal adoption, source drop, final advance and renderer recreation");
    });
}
