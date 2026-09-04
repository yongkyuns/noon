use noon_core::{
    FontResourceArena, GeometryRef, GeometryResourceArena, ObjectContentRef, ObjectId, Style,
    TextResourceArena, Transform2D, Vec2, VectorPath,
};
use noon_render_wgpu::{RenderPrimitive, RetainedFramePreparer, RetainedRenderItem};
use noon_runtime::{FrameChanges, RetainedFrameObjectState, RetainedFrameState};
use noon_text_render_wgpu::TextDeviceMetrics;

fn retained_geometry_frame(
    semantic_geometry: GeometryRef,
    render_geometry: Option<GeometryRef>,
    reveal: f32,
    morph: f32,
) -> RetainedFrameState {
    RetainedFrameState {
        time: 0.5,
        objects: vec![RetainedFrameObjectState {
            id: ObjectId::new(1),
            content: ObjectContentRef::Geometry(semantic_geometry),
            transform: Transform2D::default(),
            style: Style::default(),
            appearance: 1.0,
        }],
        presences: vec![true],
        reveals: vec![reveal],
        morphs: vec![morph],
        render_geometries: vec![render_geometry],
    }
}

fn assert_prepares_path(frame: &RetainedFrameState) {
    let texts = TextResourceArena::new();
    let fonts = FontResourceArena::new();
    let geometries = GeometryResourceArena::new();
    let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedFramePreparer::new();

    let prepared = preparer
        .prepare_with_changes(
            &device,
            &queue,
            frame,
            &FrameChanges::all(),
            &texts,
            &fonts,
            &geometries,
            metrics,
        )
        .unwrap();

    assert!(prepared.render_items.iter().any(|item| {
        matches!(
            item,
            RetainedRenderItem::Geometry {
                object_id,
                batch,
            } if *object_id == ObjectId::new(1)
                && matches!(batch.primitive, RenderPrimitive::Path { .. })
        )
    }));
}

#[test]
fn retained_analytic_create_uses_prepared_path_primitive_in_painter_order() {
    let frame = retained_geometry_frame(GeometryRef::circle(1.0), None, 0.5, 0.0);
    assert_prepares_path(&frame);
}

#[test]
fn retained_transform_preserves_runtime_effective_render_geometry() {
    let source = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(0.0, 1.0))
        .line_to(Vec2::new(1.0, 0.0))
        .line_to(Vec2::new(0.0, -1.0))
        .close();
    let target = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(-1.0, 1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .close();
    let render_geometry = GeometryRef::path(source.with_morph_target(target));
    let frame = retained_geometry_frame(
        GeometryRef::circle(1.0),
        Some(render_geometry),
        1.0,
        0.5,
    );

    assert_prepares_path(&frame);
}
