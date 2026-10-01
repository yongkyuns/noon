use noon::example_scenes::retained_locality;
use noon_core::Vec2;
use noon_render_wgpu::{text::TextDeviceMetrics, GpuRenderer, RetainedFramePreparer};

#[test]
fn one_typed_session_edit_stays_local_through_retained_upload() {
    let (scene, objects) = retained_locality::scene().expect("large sparse scene must build");
    let object_count = retained_locality::OBJECT_COUNT;
    let changed_index = retained_locality::TARGET_INDEX;

    let mut session = scene
        .execution_session()
        .expect("large scene must lower into one execution session");
    // This backend verifies renderer writes and command encoding, not GPU execution.
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut text_state = renderer.create_retained_text_state(&device, &queue);
    let mut preparer = RetainedFramePreparer::new();
    let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
    let visible_candidates = [changed_index];
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("retained locality test target"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target_view = target.create_view(&Default::default());

    {
        let initial = session.take_renderer_publication();
        let prepared = preparer
            .prepare_publication_visible(&device, &initial, &visible_candidates, metrics)
            .expect("initial publication must prepare");
        assert_eq!(prepared.stats.semantic_objects, object_count);
        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        assert!(upload.geometry.bytes_uploaded > 0);
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer
            .encode_retained(
                &mut encoder,
                &target_view,
                &prepared,
                &text_state,
                wgpu::Color::BLACK,
                None,
            )
            .expect("initial visible projection must encode");
        queue.submit([encoder.finish()]);
    }

    for edit in 0..64 {
        let translation_x = 0.75 + edit as f64 * 0.01;
        let translation = Vec2::new(translation_x as f32, -0.25);
        scene
            .live(&mut session)
            .set_translation(&objects[changed_index], translation_x, -0.25)
            .expect("one live edit must publish");

        let publication = session.take_renderer_publication();
        assert_eq!(publication.frame().objects.len(), object_count);
        assert_eq!(publication.changes().object_indices(), &[changed_index]);

        let prepared = preparer
            .prepare_publication_visible(&device, &publication, &visible_candidates, metrics)
            .expect("local publication must prepare incrementally");
        assert_eq!(prepared.stats.semantic_objects, object_count);
        assert_eq!(prepared.geometry_stats().full_rebuilds, 0);
        assert_eq!(prepared.geometry_stats().instances_repacked, 1);
        assert_eq!(prepared.geometry_stats().dirty_instance_count, 1);

        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        assert_eq!(
            upload.geometry.bytes_uploaded,
            std::mem::size_of::<noon_render_wgpu::CircleInstance>(),
            "edit {edit} must upload only the changed circle"
        );
        assert_eq!(upload.text.bytes_uploaded, 0);
        assert_eq!(upload.images.pixel_bytes_uploaded, 0);
        assert_eq!(upload.images.instance_bytes_uploaded, 0);

        let mut encoder = device.create_command_encoder(&Default::default());
        let draws = renderer
            .encode_retained(
                &mut encoder,
                &target_view,
                &prepared,
                &text_state,
                wgpu::Color::BLACK,
                None,
            )
            .expect("local visible projection must encode");
        assert_eq!(draws.instances_drawn(), 1, "edit {edit} visible submission");
        queue.submit([encoder.finish()]);

        let effective = scene
            .live(&mut session)
            .effective(&objects[changed_index])
            .expect("live query must resolve the changed identity");
        assert_eq!(effective.transform.translation, translation);
    }
}
