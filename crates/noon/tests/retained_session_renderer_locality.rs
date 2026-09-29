use noon::{MobjectTarget, Scene};
use noon_core::Vec2;
use noon_render_wgpu::{text::TextDeviceMetrics, GpuRenderer, RetainedFramePreparer};

const OBJECT_COUNT: usize = 100_000;
const CHANGED_INDEX: usize = OBJECT_COUNT / 2;

#[test]
fn one_typed_session_edit_stays_local_through_retained_upload() {
    let mut scene = Scene::new();
    let objects = (0..OBJECT_COUNT)
        .map(|_| scene.circle(0.5).expect("circle authoring must succeed"))
        .collect::<Vec<_>>();
    let targets = objects.iter().map(MobjectTarget::from).collect::<Vec<_>>();
    scene
        .add_many(&targets)
        .expect("large scene membership must publish");

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
        assert_eq!(prepared.stats.semantic_objects, OBJECT_COUNT);
        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        assert!(upload.geometry.bytes_uploaded > 0);
    }

    scene
        .live(&mut session)
        .set_translation(&objects[CHANGED_INDEX], 0.75, -0.25)
        .expect("one live edit must publish");

    let publication = session.take_renderer_publication();
    assert_eq!(publication.frame().objects.len(), OBJECT_COUNT);
    assert_eq!(publication.changes().object_indices(), &[CHANGED_INDEX]);

    let prepared = preparer
        .prepare_publication(&device, &publication, metrics)
        .expect("local publication must prepare incrementally");
    assert_eq!(prepared.stats.semantic_objects, OBJECT_COUNT);
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
        .effective(&objects[CHANGED_INDEX])
        .expect("live query must resolve the changed identity");
    assert_eq!(effective.transform.translation, Vec2::new(0.75, -0.25));
}
