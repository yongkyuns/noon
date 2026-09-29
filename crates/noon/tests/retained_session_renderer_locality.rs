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
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut text_state = renderer.create_retained_text_state(&device, &queue);
    let mut preparer = RetainedFramePreparer::new();
    let metrics = TextDeviceMetrics::uniform(100.0).unwrap();

    {
        let initial = session.take_renderer_publication();
        let prepared = preparer
            .prepare_publication(&device, &initial, metrics)
            .expect("initial publication must prepare");
        assert_eq!(prepared.stats.semantic_objects, object_count);
        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        assert!(upload.geometry.bytes_uploaded > 0);
    }

    scene
        .live(&mut session)
        .set_translation(&objects[changed_index], 0.75, -0.25)
        .expect("one live edit must publish");

    let publication = session.take_renderer_publication();
    assert_eq!(publication.frame().objects.len(), object_count);
    assert_eq!(publication.changes().object_indices(), &[changed_index]);

    let prepared = preparer
        .prepare_publication(&device, &publication, metrics)
        .expect("local publication must prepare incrementally");
    assert_eq!(prepared.stats.semantic_objects, object_count);
    assert_eq!(prepared.geometry_stats().full_rebuilds, 0);
    assert_eq!(prepared.geometry_stats().instances_repacked, 1);

    let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
    assert_eq!(
        upload.geometry.bytes_uploaded,
        std::mem::size_of::<noon_render_wgpu::CircleInstance>()
    );
    assert_eq!(upload.text.bytes_uploaded, 0);
    assert_eq!(upload.images.pixel_bytes_uploaded, 0);
    assert_eq!(upload.images.instance_bytes_uploaded, 0);

    let effective = scene
        .live(&mut session)
        .effective(&objects[changed_index])
        .expect("live query must resolve the changed identity");
    assert_eq!(effective.transform.translation, Vec2::new(0.75, -0.25));
}
