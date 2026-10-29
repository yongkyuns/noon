use super::*;
use crate::{FramePreparer, GlowParameters};
use noon_compile::{CompiledObject, CompiledScene};
use noon_core::Glow;
use noon_core::{Color, EffectUpdate, Pixels, Style};
use noon_runtime::{FrameChanges, SceneInstance};

mod composed;

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

fn effect(intensity: f64) -> EffectDefinition {
    Glow::new(
        EffectUpdate::default()
            .radius(Pixels(3.0))
            .intensity(intensity),
    )
    .unwrap()
    .into()
}

fn request(definitions: &[EffectDefinition]) -> EffectCaptureRequest<'_> {
    EffectCaptureRequest {
        object_index: 1,
        definitions,
        texture_budget_bytes: 1_000_000,
    }
}

#[test]
fn tile_camera_preserves_full_output_pixel_grid() {
    let camera = Camera2D::new(Vec2::ZERO, Vec2::new(20.0, 10.0)).unwrap();
    let parameters = GlowParameters {
        definition: effect(1.0).as_glow().unwrap(),
        output_height: 100,
        world_view_height: 10.0,
        scope_opacity: 1.0,
        scratch_budget_bytes: 1_000_000,
    };
    let tile = EffectCaptureTile::prepare(
        EffectPixelBounds {
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
    let prepared = preparer.prepare(runtime.frame()).unwrap();
    renderer.upload(&device, &queue, &prepared);
    let target = CaptureImage::new(&device, wgpu::TextureFormat::Rgba8Unorm, [80, 60]);
    let mut encoder = device.create_command_encoder(&Default::default());
    let neutral = renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&[effect(0.0)]),
        )
        .unwrap();
    assert_eq!(neutral, EffectCaptureStats::default());
    assert!(renderer.effect_captures.is_none());
    let first = renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&[effect(1.0)]),
        )
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
    let source = renderer.effect_captures.as_ref().unwrap().scopes[&(0, 1)]
        .source
        .texture
        .clone();
    let prepared = preparer
        .prepare_incremental(runtime.frame(), &FrameChanges::default())
        .unwrap();
    let mut encoder = device.create_command_encoder(&Default::default());
    let clean = renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&[effect(1.0)]),
        )
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
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&[effect(0.4)]),
        )
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
        renderer.effect_captures.as_ref().unwrap().scopes[&(0, 1)]
            .source
            .texture,
        source
    );
    queue.submit([encoder.finish()]);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&[effect(0.0)]),
        )
        .unwrap();
    assert!(!renderer.has_effect_captures());
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
    renderer.remove_effect_capture(ObjectId::new(2));
    assert_eq!(renderer.effect_capture_texture_bytes(), 0);
    assert!(renderer.effect_captures.as_ref().unwrap().scopes.is_empty());
}

#[cfg(feature = "ci-noop")]
#[test]
fn truncating_a_warm_stack_rebinds_the_final_output_and_retires_neutral_suffix() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let runtime = runtime();
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(runtime.frame()).unwrap();
    renderer.upload(&device, &queue, &prepared);
    let definitions = [
        effect(1.0),
        noon_core::Shadow::new(
            EffectUpdate::default()
                .radius(Pixels(0.0))
                .offset(noon_core::EffectOffset::Pixels([0.0, 0.0])),
        )
        .unwrap()
        .into(),
    ];
    let mut encoder = device.create_command_encoder(&Default::default());
    let first = renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&definitions),
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let last_output = renderer.effect_captures.as_ref().unwrap().scopes[&(0, 1)]
        .placement_output
        .clone()
        .unwrap();
    let mut encoder = device.create_command_encoder(&Default::default());
    let truncated = renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&definitions[..1]),
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let scope = &renderer.effect_captures.as_ref().unwrap().scopes[&(0, 1)];
    assert_eq!(
        truncated.source_passes
            + truncated.source_texture_allocations
            + truncated.filter.texture_allocations,
        0
    );
    assert_ne!(scope.placement_output.as_ref(), Some(&last_output));
    assert_eq!(scope.placement_output.as_ref(), scope.filter.output());
    assert!(truncated.retained_texture_bytes < first.retained_texture_bytes);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&definitions),
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let neutral = [EffectUpdate::default()
        .intensity(0.0)
        .apply_to(definitions[0])
        .unwrap()];
    let mut encoder = device.create_command_encoder(&Default::default());
    let neutral = renderer
        .prepare_effect_capture(&device, &queue, &mut encoder, &prepared, request(&neutral))
        .unwrap();
    let scope = &renderer.effect_captures.as_ref().unwrap().scopes[&(0, 1)];
    assert!(!scope.enabled);
    assert!(scope.placement_output.is_none());
    assert_eq!(
        scope.filter.scratch_bytes(),
        u64::from(scope.tile.size[0]) * u64::from(scope.tile.size[1]) * 12
    );
    assert_eq!(
        neutral.retained_texture_bytes,
        truncated.retained_texture_bytes
    );
    queue.submit([encoder.finish()]);
}

#[cfg(feature = "ci-noop")]
#[test]
fn tiny_mask_domain_rejects_without_discarding_a_coherent_warm_capture() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let make_runtime = |radius| {
        SceneInstance::new(
            CompiledScene::compile_objects(
                vec![CompiledObject::new(
                    ObjectId::new(2),
                    GeometryRef::circle(radius),
                    Transform2D::IDENTITY,
                    Style {
                        fill: Some(Color::WHITE),
                        stroke: None,
                        ..Style::default()
                    },
                )],
                &[],
            )
            .unwrap(),
        )
    };
    let mut preparer = FramePreparer::new();
    let mask = noon_core::NoiseDissolve::default()
        .with_progress(0.5)
        .unwrap()
        .into();
    let normal = make_runtime(0.4);
    let prepared = preparer.prepare(normal.frame()).unwrap();
    renderer.upload(&device, &queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            EffectCaptureRequest {
                object_index: 0,
                ..request(&[mask])
            },
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let bytes = renderer.effect_capture_texture_bytes();
    assert!(bytes > 0);
    for index in 0..8 {
        let value = noon_core::NoiseDissolve::new(index + 7, 0.4)
            .unwrap()
            .with_progress(0.25 + f64::from(index) * 0.05)
            .unwrap()
            .into();
        for changed in [true, false] {
            let mut encoder = device.create_command_encoder(&Default::default());
            let stats = renderer
                .prepare_effect_capture(
                    &device,
                    &queue,
                    &mut encoder,
                    &prepared,
                    EffectCaptureRequest {
                        object_index: 0,
                        ..request(&[value])
                    },
                )
                .unwrap();
            queue.submit([encoder.finish()]);
            assert_eq!(
                stats.source_passes
                    + stats.source_bytes_uploaded
                    + stats.source_texture_allocations,
                0
            );
            assert_eq!(
                stats.filter.texture_allocations
                    + stats.filter.pipeline_compiles
                    + stats.filter.kernel_builds,
                0
            );
            assert_eq!(stats.filter.bytes_uploaded, if changed { 48 } else { 0 });
            assert_eq!(stats.filter.composite_passes, usize::from(changed));
            assert_eq!(renderer.effect_capture_texture_bytes(), bytes);
        }
    }
    let output = renderer.effect_captures.as_ref().unwrap().scopes[&(0, 0)]
        .filter
        .output()
        .unwrap()
        .clone();
    let revision = renderer.effect_captures.as_ref().unwrap().scopes[&(0, 0)].output_revision;
    let tiny = make_runtime(1e-6);
    let prepared = preparer.prepare(tiny.frame()).unwrap();
    renderer.upload(&device, &queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    assert_eq!(
        renderer.prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            EffectCaptureRequest {
                object_index: 0,
                ..request(&[mask])
            }
        ),
        Err(EffectPrepareError::CaptureCoordinatesOutOfRange)
    );
    assert_eq!(renderer.effect_capture_texture_bytes(), bytes);
    let scope = &renderer.effect_captures.as_ref().unwrap().scopes[&(0, 0)];
    assert_eq!(scope.filter.output(), Some(&output));
    assert_eq!(scope.output_revision, revision);
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
    let prepared = preparer.prepare(runtime.frame()).unwrap();
    renderer.upload(&device, &queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    assert_eq!(
        renderer.prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            EffectCaptureRequest {
                texture_budget_bytes: 1,
                ..request(&[effect(1.0)])
            }
        ),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
    assert!(renderer.effect_captures.is_none());
    renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&[effect(1.0)]),
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let bytes = renderer.effect_capture_texture_bytes();
    let before = renderer.effect_captures.as_ref().unwrap().scopes[&(0, 1)]
        .filter
        .output()
        .unwrap()
        .clone();
    let mut encoder = device.create_command_encoder(&Default::default());
    assert_eq!(
        renderer.prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            EffectCaptureRequest {
                object_index: 0,
                texture_budget_bytes: bytes,
                ..request(&[effect(1.0)])
            }
        ),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
    assert_eq!(renderer.effect_capture_texture_bytes(), bytes);
    assert_eq!(renderer.effect_captures.as_ref().unwrap().scopes.len(), 1);
    assert_eq!(
        renderer.effect_captures.as_ref().unwrap().scopes[&(0, 1)]
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
    let prepared = preparer.prepare(&frame).unwrap();
    renderer.upload(&device, &queue, &prepared);
    assert_eq!(
        prepared.observe_object(1).unwrap().submission_membership,
        Some(false)
    );
    let request = EffectCaptureRequest {
        definitions: &[EffectUpdate::default()
            .source(GlowSource::Silhouette)
            .apply_to(effect(1.0))
            .unwrap()],
        ..request(&[])
    };
    let mut abandoned = device.create_command_encoder(&Default::default());
    let first = renderer
        .prepare_effect_capture(&device, &queue, &mut abandoned, &prepared, request)
        .unwrap();
    assert_eq!(first.source_passes, 2);
    drop(abandoned);
    renderer.invalidate_effect_captures();
    let mut encoder = device.create_command_encoder(&Default::default());
    let recovery = renderer
        .prepare_effect_capture(&device, &queue, &mut encoder, &prepared, request)
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
    let prepared = preparer.prepare(&frame).unwrap();
    renderer.upload(&device, &queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&[effect(1.0)]),
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let cost = renderer.effect_capture_texture_bytes();
    frame.objects.swap(0, 1);
    let prepared = preparer.prepare(&frame).unwrap();
    renderer.upload(&device, &queue, &prepared);
    // A neutral request after repacking must disable the source's old cache
    // identity before ordinary batching, even before an active reprepare moves it.
    let mut neutral_encoder = device.create_command_encoder(&Default::default());
    let neutral = renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut neutral_encoder,
            &prepared,
            EffectCaptureRequest {
                object_index: 0,
                texture_budget_bytes: cost,
                ..request(&[effect(0.0)])
            },
        )
        .unwrap();
    assert!(!renderer.has_effect_captures());
    assert_eq!(
        neutral.source_passes + neutral.filter.blur_passes + neutral.filter.composite_passes,
        0
    );
    let neutral_output = CaptureImage::new(&device, wgpu::TextureFormat::Rgba8Unorm, [80, 60]);
    assert_eq!(
        renderer
            .encode(
                &mut neutral_encoder,
                &neutral_output.view,
                &prepared,
                wgpu::Color::TRANSPARENT
            )
            .draw_calls,
        1
    );
    queue.submit([neutral_encoder.finish()]);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            EffectCaptureRequest {
                object_index: 0,
                texture_budget_bytes: cost,
                ..request(&[effect(1.0)])
            },
        )
        .unwrap();
    assert_eq!(renderer.effect_capture_texture_bytes(), cost);
    assert_eq!(renderer.effect_captures.as_ref().unwrap().scopes.len(), 1);
    assert_eq!(
        renderer.effect_captures.as_ref().unwrap().owners[&ObjectId::new(2)],
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
    renderer.remove_effect_capture(ObjectId::new(2));
    assert_eq!(renderer.effect_capture_texture_bytes(), 0);
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
    let prepared = preparer.prepare(&frame).unwrap();
    renderer.upload(&device, &queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            request(&[effect(1.0)]),
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let bytes = renderer.effect_capture_texture_bytes();
    for step in 1..=100 {
        frame.objects[1].transform.translation.x = step as f32 * 0.003;
        let prepared = preparer
            .prepare_incremental(&frame, &FrameChanges::objects(vec![1]))
            .unwrap();
        renderer.upload(&device, &queue, &prepared);
        let mut encoder = device.create_command_encoder(&Default::default());
        let stats = renderer
            .prepare_effect_capture(
                &device,
                &queue,
                &mut encoder,
                &prepared,
                request(&[effect(1.0)]),
            )
            .unwrap();
        assert_eq!(
            stats.source_texture_allocations + stats.filter.texture_allocations,
            0
        );
        assert_eq!(stats.filter.pipeline_compiles, 0);
        assert_eq!(renderer.effect_capture_texture_bytes(), bytes);
        queue.submit([encoder.finish()]);
    }
}

#[cfg(feature = "ci-noop")]
#[test]
fn publication_bridge_rejects_mismatched_packed_rows_before_resource_mutation() {
    use std::sync::Arc;
    let object = || {
        let mut object = CompiledObject::new(
            ObjectId::new(91),
            GeometryRef::circle(0.4),
            Transform2D::IDENTITY,
            Style {
                fill: Some(Color::WHITE),
                stroke: None,
                ..Style::default()
            },
        );
        object.effects = Some(Arc::new(
            noon_compile::CompiledEffectStack::new([noon_compile::CompiledEffect {
                attachment: noon_core::SemanticNodeId::new(6, 2),
                definition: effect(1.0),
            }])
            .unwrap(),
        ));
        object
    };
    let runtime_for =
        |object| SceneInstance::new(CompiledScene::compile_objects(vec![object], &[]).unwrap());
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut runtime = runtime_for(object());
    let publication = runtime.take_renderer_publication();
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(publication.frame()).unwrap();
    renderer.upload(&device, &queue, &prepared);
    let request = PublishedEffectCaptureRequest {
        object_index: 0,
        texture_budget_bytes: 1_000_000,
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer
        .prepare_published_effect_capture(
            &device,
            &queue,
            &mut encoder,
            &prepared,
            &publication,
            request,
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let bytes = renderer.effect_capture_texture_bytes();
    let output = renderer.effect_captures.as_ref().unwrap().scopes[&(0, 0)]
        .filter
        .output()
        .unwrap()
        .clone();
    for change in 0..4 {
        let mut changed = object();
        match change {
            0 => changed.base_transform.translation.x = 0.25,
            1 => changed.base_style.fill = Some(Color::RED),
            2 => changed.content = noon_core::ObjectContentRef::Geometry(GeometryRef::circle(0.6)),
            _ => changed.id = ObjectId::new(92),
        }
        let mut other = runtime_for(changed);
        let other_publication = other.take_renderer_publication();
        let mut encoder = device.create_command_encoder(&Default::default());
        assert_eq!(
            renderer.prepare_published_effect_capture(
                &device,
                &queue,
                &mut encoder,
                &prepared,
                &other_publication,
                request
            ),
            Err(EffectPrepareError::PublicationMismatch)
        );
        assert_eq!(renderer.effect_capture_texture_bytes(), bytes);
        assert_eq!(
            renderer.effect_captures.as_ref().unwrap().scopes[&(0, 0)]
                .filter
                .output(),
            Some(&output)
        );
    }
}

#[cfg(feature = "ci-noop")]
#[test]
fn logical_viewport_validation_is_atomic_and_physical_viewport_restores_default() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer
        .set_viewport_with_logical_size(&device, &queue, 702, 394, Vec2::new(704.0, 396.0))
        .unwrap();
    assert_eq!(renderer.logical_viewport_size, Vec2::new(704.0, 396.0));
    for invalid in [Vec2::ZERO, Vec2::new(f32::NAN, 10.0), Vec2::new(10.0, -1.0)] {
        assert!(renderer
            .set_viewport_with_logical_size(&device, &queue, 20, 20, invalid)
            .is_err());
        assert_eq!(renderer.viewport_size(), [702, 394]);
        assert_eq!(renderer.logical_viewport_size, Vec2::new(704.0, 396.0));
    }
    renderer.set_viewport(&device, &queue, 702, 394);
    assert_eq!(renderer.logical_viewport_size, Vec2::new(702.0, 394.0));
    assert!(renderer.effect_captures.is_none());
}
