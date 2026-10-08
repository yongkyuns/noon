use super::*;
use crate::FramePreparer;
use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{Color, GlowUpdate, Pixels, Style};
use noon_runtime::{FrameChanges, SceneInstance};

fn runtime() -> SceneInstance {
    SceneInstance::new(
        CompiledScene::compile_objects(
            (1..=3)
                .map(|id| {
                    CompiledObject::new(
                        ObjectId::new(id),
                        GeometryRef::circle(0.4),
                        Transform2D {
                            translation: Vec2::new(id as f32 - 2.0, 0.0),
                            ..Transform2D::IDENTITY
                        },
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
}

fn glow(intensity: f64) -> AnalyticGlowRequest {
    AnalyticGlowRequest {
        object_index: 1,
        definition: Glow::new(
            GlowUpdate::default()
                .radius(Pixels(3.0))
                .intensity(intensity),
        )
        .unwrap(),
        texture_budget_bytes: 1_000_000,
    }
}

#[test]
fn tile_camera_preserves_full_output_pixel_grid() {
    let camera = Camera2D::new(Vec2::ZERO, Vec2::new(20.0, 10.0)).unwrap();
    let parameters = GlowParameters {
        definition: glow(1.0).definition,
        output_height: 100,
        world_view_height: 10.0,
        scope_opacity: 1.0,
        scratch_budget_bytes: 1_000_000,
    };
    let tile = GlowCaptureTile::prepare(
        GlowPixelBounds {
            min: [-8.0, 20.0],
            max: [-4.0, 24.0],
        },
        [200, 100],
        parameters,
        4096,
        1_000_000,
    )
    .unwrap()
    .unwrap();
    let local = tile_camera(camera, [200, 100], tile).unwrap();
    let left = f64::from(local.center.x) - f64::from(local.world_size.x) * 0.5;
    let top = f64::from(local.center.y) + f64::from(local.world_size.y) * 0.5;
    assert!((left - (-10.0 + f64::from(tile.origin[0]) * 0.1)).abs() < 1e-6);
    assert!((top - (5.0 - f64::from(tile.origin[1]) * 0.1)).abs() < 1e-6);
}

#[cfg(feature = "ci-noop")]
#[test]
fn capture_and_anchor_are_retained_and_intensity_does_not_recapture() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let runtime = runtime();
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(runtime.frame());
    renderer.upload(&device, &queue, &prepared);
    let target = CaptureImage::new(&device, wgpu::TextureFormat::Rgba8Unorm, [80, 60]);
    let mut encoder = device.create_command_encoder(&Default::default());
    let neutral = renderer
        .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, glow(0.0))
        .unwrap();
    assert_eq!(neutral, AnalyticGlowStats::default());
    assert!(renderer.analytic_glows.is_none());
    let first = renderer
        .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, glow(1.0))
        .unwrap();
    assert_eq!(
        (
            first.source_passes,
            first.filter.blur_passes,
            first.filter.composite_passes
        ),
        (1, 2, 1)
    );
    assert_eq!(
        (
            first.source_texture_allocations,
            first.filter.texture_allocations
        ),
        (1, 3)
    );
    let draws = renderer.encode(
        &mut encoder,
        &target.view,
        &prepared,
        wgpu::Color::TRANSPARENT,
    );
    assert_eq!(
        draws.draw_calls, 3,
        "before-source, effect-source, after-source"
    );
    queue.submit([encoder.finish()]);
    let source = renderer.analytic_glows.as_ref().unwrap().scopes[&(0, 1)]
        .source
        .texture
        .clone();
    let prepared = preparer.prepare_incremental(runtime.frame(), &FrameChanges::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    let clean = renderer
        .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, glow(1.0))
        .unwrap();
    assert_eq!(
        clean.source_passes + clean.source_bytes_uploaded + clean.placement_bytes_uploaded,
        0
    );
    assert_eq!(
        clean.filter.blur_passes + clean.filter.composite_passes + clean.filter.bytes_uploaded,
        0
    );
    queue.submit([encoder.finish()]);
    let mut encoder = device.create_command_encoder(&Default::default());
    let edit = renderer
        .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, glow(0.4))
        .unwrap();
    assert_eq!(
        edit.source_passes + edit.source_bytes_uploaded + edit.source_texture_allocations,
        0
    );
    assert_eq!(
        edit.filter.texture_allocations + edit.filter.pipeline_compiles + edit.filter.kernel_builds,
        0
    );
    assert_eq!(
        (
            edit.filter.bytes_uploaded,
            edit.filter.blur_passes,
            edit.filter.composite_passes
        ),
        (32, 0, 1)
    );
    assert_eq!(
        renderer.analytic_glows.as_ref().unwrap().scopes[&(0, 1)]
            .source
            .texture,
        source
    );
    queue.submit([encoder.finish()]);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, glow(0.0))
        .unwrap();
    assert!(!renderer.has_analytic_glows());
    assert_eq!(
        renderer
            .encode(
                &mut encoder,
                &target.view,
                &prepared,
                wgpu::Color::TRANSPARENT
            )
            .draw_calls,
        1
    );
    queue.submit([encoder.finish()]);
    renderer.remove_analytic_glow(ObjectId::new(2));
    assert_eq!(renderer.analytic_glow_texture_bytes(), 0);
    assert!(renderer.analytic_glows.as_ref().unwrap().scopes.is_empty());
}

#[cfg(feature = "ci-noop")]
#[test]
fn budget_rejection_is_atomic_and_accounts_for_other_scopes() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let runtime = runtime();
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(runtime.frame());
    renderer.upload(&device, &queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    assert_eq!(
        renderer.prepare_analytic_glow(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            AnalyticGlowRequest {
                texture_budget_bytes: 1,
                ..glow(1.0)
            }
        ),
        Err(GlowPrepareError::ScratchBudgetExceeded)
    );
    assert!(renderer.analytic_glows.is_none());
    renderer
        .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, glow(1.0))
        .unwrap();
    queue.submit([encoder.finish()]);
    let bytes = renderer.analytic_glow_texture_bytes();
    let before = renderer.analytic_glows.as_ref().unwrap().scopes[&(0, 1)]
        .filter
        .output()
        .unwrap()
        .clone();
    let mut encoder = device.create_command_encoder(&Default::default());
    assert_eq!(
        renderer.prepare_analytic_glow(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            AnalyticGlowRequest {
                object_index: 0,
                texture_budget_bytes: bytes,
                ..glow(1.0)
            }
        ),
        Err(GlowPrepareError::ScratchBudgetExceeded)
    );
    assert_eq!(renderer.analytic_glow_texture_bytes(), bytes);
    assert_eq!(renderer.analytic_glows.as_ref().unwrap().scopes.len(), 1);
    assert_eq!(
        renderer.analytic_glows.as_ref().unwrap().scopes[&(0, 1)]
            .filter
            .output(),
        Some(&before)
    );
    queue.submit([encoder.finish()]);
}

#[cfg(feature = "ci-noop")]
#[test]
fn transparent_paint_does_not_suppress_silhouette_and_abort_is_recoverable() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut frame = runtime().frame().clone();
    frame.objects[1].style.fill = Some(Color::rgba(1.0, 0.0, 0.0, 0.0));
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(&frame);
    renderer.upload(&device, &queue, &prepared);
    assert_eq!(
        prepared.observe_object(1).unwrap().submission_membership,
        Some(false)
    );
    let request = AnalyticGlowRequest {
        definition: GlowUpdate::default()
            .source(GlowSource::Silhouette)
            .apply_to(glow(1.0).definition)
            .unwrap(),
        ..glow(1.0)
    };
    let mut abandoned = device.create_command_encoder(&Default::default());
    let first = renderer
        .prepare_analytic_glow(&device, &queue, &mut abandoned, &prepared, request)
        .unwrap();
    assert_eq!(first.source_passes, 2);
    drop(abandoned);
    renderer.invalidate_analytic_glows();
    let mut encoder = device.create_command_encoder(&Default::default());
    let recovery = renderer
        .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, request)
        .unwrap();
    assert_eq!(
        (
            recovery.source_passes,
            recovery.filter.blur_passes,
            recovery.filter.composite_passes
        ),
        (2, 2, 1)
    );
    assert_eq!(
        recovery.source_texture_allocations + recovery.filter.texture_allocations,
        0
    );
    let output = CaptureImage::new(&device, wgpu::TextureFormat::Rgba8Unorm, [80, 60]);
    assert_eq!(
        renderer
            .encode(
                &mut encoder,
                &output.view,
                &prepared,
                wgpu::Color::TRANSPARENT
            )
            .draw_calls,
        3
    );
    queue.submit([encoder.finish()]);
}

#[cfg(feature = "ci-noop")]
#[test]
fn repacked_source_retires_old_slot_and_does_not_double_count_memory() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut frame = runtime().frame().clone();
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(&frame);
    renderer.upload(&device, &queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, glow(1.0))
        .unwrap();
    queue.submit([encoder.finish()]);
    let cost = renderer.analytic_glow_texture_bytes();
    frame.objects.swap(0, 1);
    let prepared = preparer.prepare(&frame);
    renderer.upload(&device, &queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_analytic_glow(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            AnalyticGlowRequest {
                object_index: 0,
                texture_budget_bytes: cost,
                ..glow(1.0)
            },
        )
        .unwrap();
    assert_eq!(renderer.analytic_glow_texture_bytes(), cost);
    assert_eq!(renderer.analytic_glows.as_ref().unwrap().scopes.len(), 1);
    assert_eq!(
        renderer.analytic_glows.as_ref().unwrap().owners[&ObjectId::new(2)],
        (0, 0)
    );
    let output = CaptureImage::new(&device, wgpu::TextureFormat::Rgba8Unorm, [80, 60]);
    assert_eq!(
        renderer
            .encode(
                &mut encoder,
                &output.view,
                &prepared,
                wgpu::Color::TRANSPARENT
            )
            .draw_calls,
        2
    );
    queue.submit([encoder.finish()]);
    renderer.remove_analytic_glow(ObjectId::new(2));
    assert_eq!(renderer.analytic_glow_texture_bytes(), 0);
}

#[cfg(feature = "ci-noop")]
#[test]
fn fractional_translation_reuses_warmed_capture_capacity() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut frame = runtime().frame().clone();
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(&frame);
    renderer.upload(&device, &queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, glow(1.0))
        .unwrap();
    queue.submit([encoder.finish()]);
    let bytes = renderer.analytic_glow_texture_bytes();
    for step in 1..=100 {
        frame.objects[1].transform.translation.x = step as f32 * 0.003;
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![1]));
        renderer.upload(&device, &queue, &prepared);
        let mut encoder = device.create_command_encoder(&Default::default());
        let stats = renderer
            .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, glow(1.0))
            .unwrap();
        assert_eq!(
            stats.source_texture_allocations + stats.filter.texture_allocations,
            0
        );
        assert_eq!(stats.filter.pipeline_compiles, 0);
        assert_eq!(renderer.analytic_glow_texture_bytes(), bytes);
        queue.submit([encoder.finish()]);
    }
}
