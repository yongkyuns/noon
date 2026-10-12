use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    FontResourceArena, GeometryRef, GeometryResourceArena, ObjectContentRef, ObjectId, Style,
    TextResourceArena, Transform2D, Vec2, VectorPath,
};
use noon_render_wgpu::text::TextDeviceMetrics;
use noon_render_wgpu::{RenderPrimitive, RetainedFramePreparer};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState, SceneInstance};

fn retained_geometry_frame(
    semantic_geometry: GeometryRef,
    render_geometry: Option<GeometryRef>,
    reveal: f32,
    morph: f32,
) -> FrameState {
    FrameState {
        family_animations: Vec::new(),
        family_animation_plan_indices: Vec::new(),
        time: 0.5,
        objects: vec![FrameObjectState {
            glow: None,
            spatial: None,
            z_index: 0.0,
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

fn assert_prepares_path(frame: &FrameState, expect_path: bool) {
    let texts = TextResourceArena::new();
    let fonts = FontResourceArena::new();
    let geometries = GeometryResourceArena::new();
    let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedFramePreparer::new();
    preparer.set_painter_order(&[0]);

    let prepared = preparer
        .prepare_with_changes(
            &device,
            frame,
            &FrameChanges::all(),
            &texts,
            &fonts,
            &geometries,
            metrics,
        )
        .unwrap();

    let has_path = prepared
        .geometry_render_chunks()
        .flat_map(|chunk| chunk.render_batches.iter())
        .any(|batch| matches!(batch.primitive, RenderPrimitive::Path { .. }));
    assert_eq!(has_path, expect_path);
    if !expect_path {
        assert!(prepared
            .geometry_render_chunks()
            .flat_map(|chunk| chunk.render_batches.iter())
            .any(|batch| batch.primitive == RenderPrimitive::Circle));
        assert_eq!(prepared.geometry_stats().geometry_cache_misses, 0);
    }
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
fn leased_external_path_reaches_retained_renderer_and_retires_on_release() {
    let object = ObjectId::new(1);
    let compiled = CompiledScene::compile_objects(
        vec![CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        )],
        &[],
    )
    .unwrap();
    let mut runtime = SceneInstance::new(compiled);
    let mut source = GeometryResourceArena::new();
    let handle = source.insert_path(
        VectorPath::new()
            .move_to(Vec2::new(-1.0, -1.0))
            .line_to(Vec2::new(1.0, -1.0))
            .line_to(Vec2::new(0.0, 1.0))
            .close(),
    );
    let prepared = runtime
        .prepare_effective_geometry_replacement(object, handle, &source, None)
        .unwrap();
    let lease = runtime
        .commit_effective_content_replacement(prepared)
        .unwrap();
    drop(source);

    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedFramePreparer::new();
    let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
    {
        let publication = runtime.take_renderer_publication();
        let frame = preparer
            .prepare_publication(&device, &publication, metrics)
            .unwrap();
        assert!(frame
            .geometry_render_chunks()
            .flat_map(|chunk| chunk.render_batches.iter())
            .any(|batch| matches!(batch.primitive, RenderPrimitive::Path { .. })));
    }

    runtime.release_effective_content(lease).unwrap();
    let publication = runtime.take_renderer_publication();
    let frame = preparer
        .prepare_publication(&device, &publication, metrics)
        .unwrap();
    assert!(!frame
        .geometry_render_chunks()
        .flat_map(|chunk| chunk.render_batches.iter())
        .any(|batch| matches!(batch.primitive, RenderPrimitive::Path { .. })));
}

#[test]
fn retained_circle_create_uses_analytic_primitive_in_painter_order() {
    let frame = retained_geometry_frame(GeometryRef::circle(1.0), None, 0.5, 0.0);
    assert_prepares_path(&frame, false);
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

    assert_prepares_path(&frame, true);
}
