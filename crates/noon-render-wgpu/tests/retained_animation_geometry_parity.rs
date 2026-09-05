use noon_core::{
    FontResourceArena, GeometryRef, GeometryResourceArena, ObjectContentRef, ObjectId, Style,
    TextResourceArena, Transform2D, Vec2, VectorPath,
};
use noon_render_wgpu::{RenderPrimitive, RetainedFramePreparer};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};
use noon_text_render_wgpu::TextDeviceMetrics;

fn retained_geometry_frame(
    semantic_geometry: GeometryRef,
    render_geometry: Option<GeometryRef>,
    reveal: f32,
    morph: f32,
) -> FrameState {
    FrameState {
        time: 0.5,
        objects: vec![FrameObjectState {
            id: ObjectId::new(1),
            content: ObjectContentRef::Geometry(semantic_geometry),
            text_bounds: None,
            transform: Transform2D::default(),
            style: Style::default(),
            appearance: 1.0,
        }],
        presences: vec![true],
        reveals: vec![reveal],
        morphs: vec![morph],
        render_geometries: vec![render_geometry.map(Into::into)],
        render_transforms: vec![None],
    }
}

fn assert_prepares_path(frame: &FrameState) {
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

    assert!(prepared
        .geometry_render_batches()
        .iter()
        .any(|batch| matches!(batch.primitive, RenderPrimitive::Path { .. })));
    if frame.render_transforms[0].is_some() {
        assert_eq!(prepared.geometry_stats().geometry_cache_misses, 1);
        let mut next = frame.clone();
        next.time += 0.1;
        next.morphs[0] = 0.6;
        next.objects[0].transform.rotation += 0.3;
        next.objects[0].transform.scale = Vec2::new(2.3, 0.6);
        let warm = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &next,
                &FrameChanges::objects(vec![0]),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        assert_eq!(warm.geometry_stats().geometry_cache_misses, 0);
        assert_eq!(warm.geometry_stats().path_vertices_repacked, 0);
        assert_eq!(warm.geometry_stats().path_indices_repacked, 0);
    }
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
    let mut frame =
        retained_geometry_frame(GeometryRef::circle(1.0), Some(render_geometry), 1.0, 0.5);
    frame.objects[0].style.stroke_width_mode = noon_core::StrokeWidthMode::ScreenSpace;
    frame.objects[0].transform = Transform2D {
        translation: Vec2::new(2.0, -1.0),
        rotation: 0.7,
        scale: Vec2::new(1.3, 0.8),
    };
    frame.render_transforms[0] = Some(Transform2D::IDENTITY);

    assert_prepares_path(&frame);
}
