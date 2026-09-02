use noon_core::{
    FontResourceArena, GeometryRef, GeometryResourceArena, ObjectContentRef, ObjectId, Style,
    TextResourceArena, Transform2D, Vec2,
};
use noon_render_wgpu::{GpuRenderer, RetainedFrameIncrementalStats, RetainedFramePreparer};
use noon_runtime::{FrameChanges, RetainedFrameObjectState, RetainedFrameState};
use noon_text_render_wgpu::TextDeviceMetrics;
use noon_typst::{compile_typst_resource, TypstMode};

const STATIC_OBJECTS: usize = 10_000;
const STATIC_FRAMES: u64 = 128;

fn static_geometry_frame() -> RetainedFrameState {
    let objects = (0..STATIC_OBJECTS)
        .map(|index| RetainedFrameObjectState {
            id: ObjectId::new(index as u64),
            content: ObjectContentRef::Geometry(GeometryRef::circle(0.5)),
            transform: Transform2D::default(),
            style: Style::default(),
            appearance: 1.0,
        })
        .collect();

    RetainedFrameState {
        time: 0.0,
        objects,
        presences: vec![true; STATIC_OBJECTS],
        reveals: vec![1.0; STATIC_OBJECTS],
        morphs: vec![0.0; STATIC_OBJECTS],
        render_geometries: vec![None; STATIC_OBJECTS],
    }
}

#[test]
fn unchanged_large_retained_scene_reuses_preparation_scratch_after_warmup() {
    let texts = TextResourceArena::new();
    let fonts = FontResourceArena::new();
    let geometries = GeometryResourceArena::new();
    let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedFramePreparer::new();
    let mut frame = static_geometry_frame();

    {
        let prepared = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        assert_eq!(prepared.stats.semantic_objects, STATIC_OBJECTS);
    }

    let no_changes = FrameChanges::default();
    for frame_number in 1..=STATIC_FRAMES {
        frame.time = frame_number as f64 / 60.0;
        let prepared = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &no_changes,
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        assert_eq!(prepared.stats.semantic_objects, STATIC_OBJECTS);
    }

    assert_eq!(
        preparer.incremental_stats(),
        RetainedFrameIncrementalStats {
            scratch_rebuilds: 1,
            scratch_reuses: STATIC_FRAMES,
            text_snapshot_copies: 1,
            mixed_order_rebuilds: 1,
        }
    );
}

#[test]
fn one_fast_text_update_reuses_parent_scratch_snapshot_and_order() {
    let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
    let mut texts = TextResourceArena::new();
    let text = texts.insert(artifact.resource).unwrap();
    let mut frame = RetainedFrameState {
        time: 0.0,
        objects: (0..STATIC_OBJECTS)
            .map(|index| RetainedFrameObjectState {
                id: ObjectId::new(index as u64),
                content: ObjectContentRef::Text(text),
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
            })
            .collect(),
        presences: vec![true; STATIC_OBJECTS],
        reveals: vec![1.0; STATIC_OBJECTS],
        morphs: vec![0.0; STATIC_OBJECTS],
        render_geometries: vec![None; STATIC_OBJECTS],
    };
    let fonts = artifact.fonts;
    let geometries = GeometryResourceArena::new();
    let metrics = TextDeviceMetrics::uniform(67.5).unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedFramePreparer::new();

    let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut text_gpu = renderer.create_retained_text_state(&device, &queue);
    let first_upload = {
        let prepared = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        renderer.upload_retained(&device, &queue, &prepared, &mut text_gpu)
    };
    assert!(first_upload.text.bytes_uploaded > 0);

    // Prepare one local generation without submitting it. The next local generation
    // must not patch over the skipped generation's stale GPU ranges.
    frame.time = 1.0 / 60.0;
    frame.objects[STATIC_OBJECTS / 2].transform.translation = Vec2::new(0.01, -0.005);
    preparer
        .prepare_with_changes(
            &device,
            &queue,
            &frame,
            &FrameChanges::objects(vec![STATIC_OBJECTS / 2]),
            &texts,
            &fonts,
            &geometries,
            metrics,
        )
        .unwrap();

    frame.time = 2.0 / 60.0;
    frame.objects[STATIC_OBJECTS / 2 + 1].transform.translation = Vec2::new(0.02, -0.01);
    let upload_after_skipped_generation = {
        let prepared = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::objects(vec![STATIC_OBJECTS / 2 + 1]),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        renderer.upload_retained(&device, &queue, &prepared, &mut text_gpu)
    };
    assert_eq!(
        upload_after_skipped_generation.text.bytes_uploaded,
        first_upload.text.bytes_uploaded
    );

    for frame_number in 3..=(STATIC_FRAMES + 2) {
        frame.time = frame_number as f64 / 60.0;
        frame.objects[STATIC_OBJECTS / 2].transform.translation =
            Vec2::new(frame_number as f32 * 0.01, -(frame_number as f32) * 0.005);
        frame.objects[STATIC_OBJECTS / 2].style.opacity = 0.75;
        let upload = {
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::objects(vec![STATIC_OBJECTS / 2]),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                )
                .unwrap();
            renderer.upload_retained(&device, &queue, &prepared, &mut text_gpu)
        };
        assert!(upload.text.bytes_uploaded > 0);
        assert!(upload.text.bytes_uploaded < first_upload.text.bytes_uploaded);
    }

    assert_eq!(
        preparer.incremental_stats(),
        RetainedFrameIncrementalStats {
            scratch_rebuilds: 1,
            scratch_reuses: STATIC_FRAMES + 2,
            text_snapshot_copies: 1,
            mixed_order_rebuilds: 1,
        }
    );
}
