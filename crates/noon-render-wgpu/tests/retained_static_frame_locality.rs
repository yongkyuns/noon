use noon_core::{
    FontResourceArena, GeometryRef, GeometryResourceArena, ObjectContentRef, ObjectId, Style,
    TextResourceArena, Transform2D,
};
use noon_render_wgpu::{RetainedFrameIncrementalStats, RetainedFramePreparer};
use noon_runtime::{FrameChanges, RetainedFrameObjectState, RetainedFrameState};
use noon_text_render_wgpu::TextDeviceMetrics;

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
        }
    );
}
