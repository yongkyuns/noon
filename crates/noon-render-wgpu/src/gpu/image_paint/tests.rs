use super::*;
use std::sync::Arc;

#[test]
fn image_paint_pipelines_validate_for_analytic_and_path_coverage() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let images = super::super::raster_image_gpu::RasterImageGpuRenderer::new(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    ImagePaintGpu::new(
        &device,
        &renderer.camera_bind_group_layout,
        wgpu::TextureFormat::Rgba8Unorm,
        images.texture_layout(),
    );
}

use super::super::RetainedFramePreparer;
use crate::{text::TextDeviceMetrics, FramePreparer};
use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Color, FontResourceArena, GeometryRef, GeometryResourceArena, ImageAddressMode, ImageFit,
    ImageMapping, ImagePaint, ImageUvTransform, ObjectContentRef, RasterImageContentRef,
    RasterImageResourceArena, RasterImageSampling, SemanticImageContent, Style, TextResourceArena,
    Transform2D,
};
use noon_runtime::{FrameChanges, FrameState, SceneInstance};

fn objects(count: u64) -> FrameState {
    SceneInstance::new(
        CompiledScene::compile_objects(
            (0..count)
                .map(|index| {
                    CompiledObject::new(
                        ObjectId::new(index),
                        GeometryRef::circle(1.0),
                        Transform2D::IDENTITY,
                        Style {
                            fill: Some(Color::WHITE),
                            stroke: None,
                            ..Style::default()
                        },
                    )
                })
                .collect(),
            &[],
        )
        .unwrap(),
    )
    .frame()
    .clone()
}

fn resource() -> (RasterImageResourceArena, ImagePaint) {
    let mut arena = RasterImageResourceArena::new();
    let handle = arena
        .intern_rgba8(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 128, 255, 255, 255, 0, 0, 0, 255, 255,
            ],
        )
        .unwrap();
    (
        arena,
        ImagePaint::new(handle, ImageMapping::default(), RasterImageSampling::Linear),
    )
}

#[test]
fn image_fill_edit_in_large_solid_frame_updates_one_sparse_route() {
    let (_arena, paint) = resource();
    let mut frame = objects(100_000);
    let mut preparer = FramePreparer::new();
    preparer.prepare(&frame);
    frame.objects[50_000].fill_image = Some(Arc::new(paint));
    let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![50_000]));
    assert_eq!(
        prepared
            .painted_indices(RenderPrimitive::Circle, 0..100_000)
            .collect::<Vec<_>>(),
        [50_000]
    );
    assert_eq!(prepared.stats.instances_repacked, 1);
    assert_eq!(prepared.stats.dirty_instance_count, 0);
    assert_eq!(prepared.stats.geometry_cache_misses, 0);
    assert_eq!(prepared.stats.render_order_positions_visited, 0);
    assert_eq!(prepared.image_paint_changes, &[ObjectId::new(50_000)]);
    let clean = preparer.prepare_incremental(&frame, &FrameChanges::default());
    assert!(clean.image_paint_changes.is_empty());
    assert_eq!(clean.stats.instances_repacked, 0);
}

#[test]
fn image_fills_share_texture_residency_with_images_and_retain_local_uniforms() {
    let (arena, paint) = resource();
    let mut frame = objects(3);
    frame.objects[0].fill_image = Some(Arc::new(paint));
    frame.objects[1].fill_image = Some(Arc::new(paint));
    frame.objects[2].content = ObjectContentRef::Image(RasterImageContentRef::from_resource(
        SemanticImageContent::new(paint.resource()),
        arena.get(paint.resource()).unwrap(),
    ));
    frame.objects[2].style.stroke_width = 0.0;
    let texts = TextResourceArena::new();
    let fonts = FontResourceArena::new();
    let geometries = GeometryResourceArena::new();
    let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedFramePreparer::new();
    preparer.set_painter_order(&[0, 1, 2]);
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut text_state = renderer.create_retained_text_state(&device, &queue);
    {
        let prepared = preparer
            .prepare_with_image_resources(
                &device,
                &frame,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                &arena,
                metrics,
            )
            .unwrap();
        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        assert_eq!(upload.images.textures_uploaded, 1);
        assert_eq!(upload.images.pixel_bytes_uploaded, 16);
        assert_eq!(upload.images.instances_uploaded, 1);
        assert_eq!(renderer.image_residency_stats().textures, 1);
        assert_eq!(renderer.image_residency_stats().objects, 3);
    }
    let mapping = ImageMapping::new(
        ImageFit::Cover,
        [0.25, 0.75],
        ImageAddressMode::Repeat,
        ImageUvTransform::new([0.125, -0.25], [0.5, 0.75], 0.2).unwrap(),
    )
    .unwrap();
    frame.objects[1].fill_image = Some(Arc::new(ImagePaint::new(
        paint.resource(),
        mapping,
        paint.sampling(),
    )));
    {
        let prepared = preparer
            .prepare_with_image_resources(
                &device,
                &frame,
                &FrameChanges::objects(vec![1]),
                &texts,
                &fonts,
                &geometries,
                &arena,
                metrics,
            )
            .unwrap();
        assert_eq!(prepared.geometry.stats.full_rebuilds, 0);
        assert_eq!(prepared.geometry.stats.geometry_cache_misses, 0);
        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        assert_eq!(
            upload.images,
            super::super::raster_image_gpu::RasterImageUploadStats::default()
        );
        assert_eq!(
            upload.geometry.bytes_uploaded,
            size_of::<ImagePaintUniform>()
        );
        assert_eq!(upload.geometry.buffer_reallocations, 0);
    }
    {
        let prepared = preparer
            .prepare_with_image_resources(
                &device,
                &frame,
                &FrameChanges::default(),
                &texts,
                &fonts,
                &geometries,
                &arena,
                metrics,
            )
            .unwrap();
        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        assert_eq!(upload.geometry, UploadStats::default());
        assert_eq!(
            upload.images,
            super::super::raster_image_gpu::RasterImageUploadStats::default()
        );
    }
    frame.objects[0].fill_image = None;
    {
        let prepared = preparer
            .prepare_with_image_resources(
                &device,
                &frame,
                &FrameChanges::objects(vec![0]),
                &texts,
                &fonts,
                &geometries,
                &arena,
                metrics,
            )
            .unwrap();
        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        assert_eq!(upload.images.textures_retired, 0);
        assert_eq!(renderer.image_residency_stats().textures, 1);
        assert_eq!(renderer.image_residency_stats().objects, 2);
    }
    frame.objects[1].fill_image = None;
    frame.presences[2] = false;
    {
        let prepared = preparer
            .prepare_with_image_resources(
                &device,
                &frame,
                &FrameChanges::objects(vec![1, 2]),
                &texts,
                &fonts,
                &geometries,
                &arena,
                metrics,
            )
            .unwrap();
        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        assert_eq!(upload.images.textures_retired, 1);
        assert_eq!(renderer.image_residency_stats().textures, 0);
        assert_eq!(renderer.image_residency_stats().objects, 0);
        assert!(renderer
            .fill_image_paints
            .as_ref()
            .unwrap()
            .residents
            .is_empty());
    }
}

#[test]
fn missing_image_fill_resource_rejects_before_replacing_retained_rows() {
    let (arena, paint) = resource();
    let (_foreign, foreign) = resource();
    let mut frame = objects(1);
    frame.objects[0].fill_image = Some(Arc::new(paint));
    let mut prepared = super::super::raster_image_prepare::RasterImageFramePreparer::default();
    let initial = prepared
        .stage(&frame, &FrameChanges::all(), Some(&arena), 4096)
        .unwrap();
    prepared.commit(initial);
    let generation = prepared.generation;
    frame.objects[0].fill_image = Some(Arc::new(foreign));
    assert!(matches!(
        prepared.stage(&frame, &FrameChanges::objects(vec![0]), Some(&arena), 4096),
        Err(super::super::raster_image_prepare::RasterImagePrepareError::MissingResource(_))
    ));
    assert_eq!(prepared.generation, generation);
    assert_eq!(prepared.paint_objects[&0].handle, paint.resource());
    frame.objects[0].fill_image = Some(Arc::new(paint));
    let restored = prepared
        .stage(&frame, &FrameChanges::default(), Some(&arena), 4096)
        .unwrap();
    prepared.commit(restored);
    assert_eq!(prepared.paint_objects[&0].handle, paint.resource());
}

#[test]
fn unsupported_image_fill_geometry_and_coordinate_overrides_preserve_prepared_rows() {
    let (arena, paint) = resource();
    let mut frame = objects(1);
    frame.objects[0].fill_image = Some(Arc::new(paint));
    let mut prepared = super::super::raster_image_prepare::RasterImageFramePreparer::default();
    let initial = prepared
        .stage(&frame, &FrameChanges::all(), Some(&arena), 4096)
        .unwrap();
    prepared.commit(initial);
    let generation = prepared.generation;
    let valid = frame.clone();
    for case in 0..4 {
        frame = valid.clone();
        match case {
            0 => {
                frame.objects[0].content =
                    GeometryRef::line(noon_core::Vec2::ZERO, noon_core::Vec2::new(1.0, 1.0)).into()
            }
            1 => frame.render_transforms[0] = Some(Transform2D::IDENTITY),
            2 => frame.render_geometries[0] = Some(std::sync::Arc::new(GeometryRef::circle(1.0))),
            _ => frame.reveals[0] = 0.5,
        }
        assert!(matches!(prepared.stage(&frame, &FrameChanges::objects(vec![0]), Some(&arena), 4096),
            Err(super::super::raster_image_prepare::RasterImagePrepareError::InvalidPresentation(_))), "case {case}");
        assert_eq!(prepared.generation, generation);
        assert_eq!(prepared.paint_objects[&0].handle, paint.resource());
    }
}

#[test]
#[should_panic(
    expected = "image fills require retained image resource preparation and upload_retained"
)]
fn geometry_only_upload_rejects_unprepared_image_fill_resources() {
    let (_arena, paint) = resource();
    let mut frame = objects(1);
    frame.objects[0].fill_image = Some(Arc::new(paint));
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(&frame);
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.upload(&device, &queue, &prepared);
}

#[test]
fn image_fill_switches_only_its_path_submission_without_retessellation() {
    let (_arena, paint) = resource();
    let mut frame = objects(1024);
    for (index, row) in frame.objects.iter_mut().enumerate() {
        let radius = 1.0 + index as f32 / 2048.0;
        row.content = GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(noon_core::Vec2::new(-radius, -1.0))
                .line_to(noon_core::Vec2::new(radius, -1.0))
                .line_to(noon_core::Vec2::new(radius, 1.0))
                .line_to(noon_core::Vec2::new(-radius, 1.0))
                .close(),
        )
        .into();
    }
    let mut preparer = FramePreparer::new();
    preparer.prepare(&frame);
    for value in [Some(paint), None] {
        frame.objects[500].fill_image = value.map(Arc::new);
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![500]));
        assert_eq!(prepared.image_paint_changes, &[ObjectId::new(500)]);
        assert_eq!(prepared.stats.full_rebuilds, 0);
        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.path_vertices_repacked, 0);
        assert_eq!(prepared.stats.path_indices_repacked, 0);
        assert!(prepared.stats.render_order_positions_visited <= 256);
        if value.is_some() {
            let route = prepared
                .image_paint_indices(RenderPrimitive::Path { batch: 0 }, 0..1024)
                .unwrap()
                .next()
                .copied()
                .unwrap();
            assert!(prepared.ordered_render_batches().any(|draw| matches!(
                draw.batch.primitive,
                RenderPrimitive::Path { .. }
            ) && draw
                .batch
                .instance_range
                .contains(&route)));
        }
    }
}
