use noon_core::{
    FontResourceArena, GeometryId, GeometryRef, GeometryResourceArena, ObjectContentRef, ObjectId,
    Style, TextResourceArena, Transform2D,
};
use noon_render_wgpu::{
    RetainedFrameIncrementalStats, RetainedFramePreparer, RetainedPrepareError,
};
use noon_runtime::{FrameChanges, RetainedFrameObjectState, RetainedFrameState};
use noon_text_render_wgpu::TextDeviceMetrics;

fn geometry_frame(geometry: GeometryRef) -> RetainedFrameState {
    RetainedFrameState {
        time: 0.0,
        objects: vec![RetainedFrameObjectState {
            id: ObjectId::new(1),
            content: ObjectContentRef::Geometry(geometry),
            transform: Transform2D::default(),
            style: Style::default(),
            appearance: 1.0,
        }],
        presences: vec![true],
        reveals: vec![1.0],
        morphs: vec![0.0],
        render_geometries: vec![None],
        render_transforms: vec![None],
    }
}

#[test]
fn failed_scratch_rebuild_cannot_be_reused_by_empty_changes() {
    let texts = TextResourceArena::new();
    let fonts = FontResourceArena::new();
    let geometries = GeometryResourceArena::new();
    let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedFramePreparer::new();

    let mut frame = geometry_frame(GeometryRef::circle(1.0));
    preparer
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
    assert_eq!(
        preparer.incremental_stats(),
        RetainedFrameIncrementalStats {
            scratch_rebuilds: 1,
            scratch_reuses: 0,
            text_snapshot_copies: 1,
            mixed_order_rebuilds: 1,
        }
    );

    frame.objects[0].content =
        ObjectContentRef::Geometry(GeometryRef::External(GeometryId::new(999)));
    let error = preparer
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
        .err()
        .expect("missing geometry rebuild must fail");
    assert_eq!(error, RetainedPrepareError::MissingGeometryResource);

    let retry = preparer
        .prepare_with_changes(
            &device,
            &queue,
            &frame,
            &FrameChanges::default(),
            &texts,
            &fonts,
            &geometries,
            metrics,
        )
        .err()
        .expect("empty-change retry must not reuse failed scratch generation");
    assert_eq!(retry, RetainedPrepareError::MissingGeometryResource);
    assert_eq!(
        preparer.incremental_stats(),
        RetainedFrameIncrementalStats {
            scratch_rebuilds: 1,
            scratch_reuses: 0,
            text_snapshot_copies: 1,
            mixed_order_rebuilds: 1,
        }
    );
}
