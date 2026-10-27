use super::*;
use noon_core::{Blur, Color, EffectOffset, EffectUpdate, Glow, Pixels, Shadow};

mod pixels;
mod reference;

fn parameters() -> EffectStackParameters {
    EffectStackParameters {
        output_height: 240,
        world_view_height: 8.0,
        scope_opacity: 1.0,
        scratch_budget_bytes: 16 * 1024 * 1024,
        source_bounds: None,
    }
}
fn blur(sigma: f64) -> EffectDefinition {
    Blur::new(EffectUpdate::default().radius(Pixels(sigma)))
        .unwrap()
        .into()
}
fn dissolve(seed: u32, softness: f64, progress: f64) -> EffectDefinition {
    noon_core::NoiseDissolve::new(seed, softness)
        .unwrap()
        .with_progress(progress)
        .unwrap()
        .into()
}
fn glow(intensity: f64, source: GlowSource) -> EffectDefinition {
    Glow::new(
        EffectUpdate::default()
            .radius(Pixels(2.0))
            .color(Color::rgba(0.7, 0.2, 0.9, 0.6))
            .intensity(intensity)
            .source(source),
    )
    .unwrap()
    .into()
}
fn shadow(red: f32) -> EffectDefinition {
    Shadow::new(
        EffectUpdate::default()
            .radius(Pixels(1.0))
            .color(Color::rgba(red, 0.3, 0.7, 0.5))
            .offset(EffectOffset::Pixels([2.25, -1.5])),
    )
    .unwrap()
    .into()
}
fn texture(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ordered filter qualification source"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
fn render(
    filter: &mut EffectStackFilter,
    scope: &mut EffectStackScope,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    capture: GlowCapture<'_>,
    effects: &[EffectDefinition],
    parameters: EffectStackParameters,
) -> Result<EffectRasterStats, EffectPrepareError> {
    let mut encoder = device.create_command_encoder(&Default::default());
    let result = filter.prepare_encode(
        device,
        queue,
        &mut encoder,
        scope,
        capture,
        effects,
        parameters,
    );
    if result.is_ok() {
        queue.submit([encoder.finish()]);
    }
    result
}

#[test]
fn trailing_mask_destination_reuse_stays_charged_and_budget_failures_are_atomic() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let image = texture(&device, [33, 29]);
    let capture = GlowCapture {
        source: &image,
        silhouette: None,
        revision: 1,
    };
    let mut filter = EffectStackFilter::default();
    let mut scope = EffectStackScope::default();
    let effects = [blur(2.0), dissolve(7, 0.2, 0.5)];
    let first = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        parameters(),
    )
    .unwrap();
    let mut budget = parameters();
    budget.scratch_budget_bytes = first.scratch_bytes;
    let mask_output = scope.output().unwrap().clone();
    for seed in 0..16 {
        render(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            capture,
            &effects[..1],
            budget,
        )
        .unwrap();
        assert_eq!(
            scope.scratch_bytes(),
            first.scratch_bytes,
            "warm destination remains budgeted"
        );
        assert_eq!(
            scope.retained_profile_bytes(&effects[..1]),
            first.scratch_bytes
        );
        let output = scope.output().unwrap().clone();
        let revision = scope.output_revision();
        assert!(matches!(
            render(
                &mut filter,
                &mut scope,
                &device,
                &queue,
                GlowCapture {
                    source: &mask_output,
                    ..capture
                },
                &effects,
                budget
            ),
            Err(EffectPrepareError::UnsupportedCapture)
        ));
        assert!(matches!(
            render(
                &mut filter,
                &mut scope,
                &device,
                &queue,
                capture,
                &effects[..1],
                EffectStackParameters {
                    scratch_budget_bytes: first.scratch_bytes - 1,
                    ..budget
                }
            ),
            Err(EffectPrepareError::ScratchBudgetExceeded)
        ));
        assert_eq!(scope.output(), Some(&output));
        assert_eq!(scope.output_revision(), revision);
        assert_eq!(scope.scratch_bytes(), first.scratch_bytes);
        scope.deactivate_profile(&effects[..1]);
        assert_eq!(scope.scratch_bytes(), first.scratch_bytes);
        let neutral = [blur(2.0), dissolve(seed, 0.4, 0.0)];
        let work = render(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            capture,
            &neutral,
            budget,
        )
        .unwrap();
        assert_eq!(work.texture_allocations, 0);
        assert_eq!(scope.scratch_bytes(), first.scratch_bytes);
        let work = render(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            capture,
            &[blur(2.0), dissolve(seed, 0.4, 0.5)],
            budget,
        )
        .unwrap();
        assert_eq!(work.texture_allocations, 0);
        assert_eq!(scope.scratch_bytes(), first.scratch_bytes);
    }
    render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &[],
        budget,
    )
    .unwrap();
    assert_eq!(
        scope.scratch_bytes(),
        0,
        "retired capture releases active and warm resources"
    );
}

#[test]
fn neutral_stack_is_allocation_free_and_filter_dirty_work_follows_the_suffix() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let image = texture(&device, [33, 29]);
    let capture = GlowCapture {
        source: &image,
        silhouette: None,
        revision: 1,
    };
    let mut filter = EffectStackFilter::default();
    let mut scope = EffectStackScope::default();
    let neutral = [blur(0.0), glow(0.0, GlowSource::Painted)];
    assert_eq!(
        render(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            capture,
            &neutral,
            parameters()
        )
        .unwrap(),
        EffectRasterStats::default()
    );
    assert!(scope.passes.is_empty());
    assert!(scope.output().is_none());
    let mut effects = [blur(3.0), glow(1.0, GlowSource::Painted), shadow(0.1)];
    let first = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        parameters(),
    )
    .unwrap();
    assert_eq!((first.blur_passes, first.composite_passes), (6, 4));
    assert_eq!(
        first.pipeline_compiles, 7,
        "Gaussian programs shared by Blur and Shadow"
    );
    assert_eq!(first.texture_allocations, 18);
    assert_eq!(first.scratch_bytes, 33 * 29 * 72);
    let handles: Vec<_> = scope
        .passes
        .iter()
        .map(|pass| pass.filter.output().unwrap().clone())
        .collect();
    let revision = scope.output_revision();
    let clean = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        parameters(),
    )
    .unwrap();
    assert_eq!(
        clean,
        EffectRasterStats {
            scratch_bytes: first.scratch_bytes,
            ..Default::default()
        }
    );
    assert_eq!(scope.output_revision(), revision);
    effects[2] = shadow(0.8);
    let edit = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        parameters(),
    )
    .unwrap();
    assert_eq!(
        (edit.bytes_uploaded, edit.blur_passes, edit.composite_passes),
        (32, 0, 1)
    );
    assert_eq!(
        edit.texture_allocations + edit.pipeline_compiles + edit.kernel_builds,
        0
    );
    effects[1] = glow(1.5, GlowSource::Painted);
    let edit = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        parameters(),
    )
    .unwrap();
    assert_eq!((edit.blur_passes, edit.composite_passes), (2, 3));
    assert_eq!(
        edit.texture_allocations + edit.pipeline_compiles + edit.kernel_builds,
        0
    );
    effects[0] = blur(4.0);
    let edit = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        parameters(),
    )
    .unwrap();
    assert_eq!(
        (edit.blur_passes, edit.composite_passes, edit.kernel_builds),
        (6, 4, 1)
    );
    assert_eq!(edit.texture_allocations + edit.pipeline_compiles, 0);
    for (pass, handle) in scope.passes.iter().zip(handles) {
        assert_eq!(pass.filter.output(), Some(&handle));
    }
    let faded = EffectStackParameters {
        scope_opacity: 0.37,
        ..parameters()
    };
    let edit = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        faded,
    )
    .unwrap();
    assert_eq!(
        (edit.bytes_uploaded, edit.blur_passes, edit.composite_passes),
        (32, 0, 1)
    );
    effects[1] = glow(0.0, GlowSource::Painted);
    let neutral = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        faded,
    )
    .unwrap();
    assert_eq!(
        neutral.scratch_bytes, first.scratch_bytes,
        "warm neutral resources remain charged"
    );
    assert_eq!((neutral.blur_passes, neutral.composite_passes), (2, 2));
    let zero = EffectStackParameters {
        scope_opacity: 0.0,
        ..parameters()
    };
    let neutral = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        zero,
    )
    .unwrap();
    assert_eq!(
        (
            neutral.bytes_uploaded,
            neutral.blur_passes,
            neutral.composite_passes
        ),
        (0, 0, 0)
    );
    assert!(scope.output().is_none());
    scope.invalidate();
    let recovery = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        faded,
    )
    .unwrap();
    assert_eq!((recovery.blur_passes, recovery.composite_passes), (4, 3));
    assert_eq!(recovery.texture_allocations, 0);
    for _ in 0..20 {
        render(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            capture,
            &effects,
            parameters(),
        )
        .unwrap();
        assert_eq!(scope.scratch_bytes(), first.scratch_bytes);
    }
    render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects[..1],
        parameters(),
    )
    .unwrap();
    assert_eq!(scope.scratch_bytes(), 33 * 29 * 28);
    render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &[],
        parameters(),
    )
    .unwrap();
    assert_eq!(scope.scratch_bytes(), 0);
    assert!(scope.output().is_none());
}

#[test]
fn late_invalid_pass_preserves_the_previous_coherent_stack_and_earlier_parameters() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let image = texture(&device, [33, 29]);
    let mask = texture(&device, [32, 29]);
    let capture = GlowCapture {
        source: &image,
        silhouette: None,
        revision: 1,
    };
    let mut filter = EffectStackFilter::default();
    let mut scope = EffectStackScope::default();
    let effects = [blur(3.0), shadow(0.1)];
    render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        parameters(),
    )
    .unwrap();
    let output = scope.output().unwrap().clone();
    let revision = scope.output_revision();
    let bytes = scope.scratch_bytes();
    let mismatch = GlowCapture {
        silhouette: Some(&mask),
        ..capture
    };
    for (capture, effects, params, error) in [
        (
            capture,
            vec![blur(4.0), dissolve(7, 0.08, 0.5)],
            EffectStackParameters {
                source_bounds: Some(EffectPixelBounds {
                    min: [0.0; 2],
                    max: [0.0, 29.0],
                }),
                ..parameters()
            },
            EffectPrepareError::InvalidSourceBounds,
        ),
        (
            capture,
            vec![blur(4.0), blur(65.0)],
            parameters(),
            EffectPrepareError::RadiusExceedsProfile,
        ),
        (
            capture,
            vec![blur(4.0), glow(1.0, GlowSource::Silhouette)],
            parameters(),
            EffectPrepareError::MissingSilhouette,
        ),
        (
            mismatch,
            vec![blur(4.0), glow(1.0, GlowSource::Silhouette)],
            parameters(),
            EffectPrepareError::CaptureSizeMismatch,
        ),
        (
            capture,
            vec![blur(4.0), shadow(0.8)],
            EffectStackParameters {
                scratch_budget_bytes: bytes - 1,
                ..parameters()
            },
            EffectPrepareError::ScratchBudgetExceeded,
        ),
        (
            capture,
            vec![blur(4.0); 9],
            parameters(),
            EffectPrepareError::StackExceedsProfile,
        ),
        (
            capture,
            vec![blur(4.0)],
            EffectStackParameters {
                scope_opacity: f32::NAN,
                ..parameters()
            },
            EffectPrepareError::InvalidScopeOpacity,
        ),
        (
            capture,
            vec![
                blur(4.0),
                Shadow::new(EffectUpdate::default().offset(EffectOffset::Pixels([1e40, 0.0])))
                    .unwrap()
                    .into(),
            ],
            parameters(),
            EffectPrepareError::CaptureCoordinatesOutOfRange,
        ),
    ] {
        assert_eq!(
            render(
                &mut filter,
                &mut scope,
                &device,
                &queue,
                capture,
                &effects,
                params
            ),
            Err(error)
        );
        assert_eq!(scope.output(), Some(&output));
        assert_eq!(scope.output_revision(), revision);
        assert_eq!(scope.scratch_bytes(), bytes);
    }
    let clean = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        parameters(),
    )
    .unwrap();
    assert_eq!(
        clean,
        EffectRasterStats {
            scratch_bytes: bytes,
            ..Default::default()
        }
    );
    scope.clear();
    assert_eq!(scope.scratch_bytes(), 0);
}

#[test]
fn later_pass_capability_failure_happens_before_any_prefix_is_installed() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor {
        required_limits: wgpu::Limits {
            max_color_attachments: 1,
            max_color_attachment_bytes_per_sample: 4,
            ..Default::default()
        },
        ..Default::default()
    });
    let image = texture(&device, [33, 29]);
    let mut filter = EffectStackFilter::default();
    let mut scope = EffectStackScope::default();
    assert_eq!(
        render(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            GlowCapture {
                source: &image,
                silhouette: None,
                revision: 1
            },
            &[glow(1.0, GlowSource::Painted), blur(3.0)],
            parameters()
        ),
        Err(EffectPrepareError::Blur(
            super::super::GaussianBlurPrepareError::InsufficientColorAttachments
        ))
    );
    assert!(scope.passes.is_empty());
    assert!(scope.output().is_none());
}

#[test]
fn maximum_stack_shares_programs_and_churn_stays_within_the_retained_budget() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let image = texture(&device, [17, 13]);
    let capture = GlowCapture {
        source: &image,
        silhouette: None,
        revision: 1,
    };
    let mut filter = EffectStackFilter::default();
    let mut scope = EffectStackScope::default();
    let mut effects = [blur(1.0); MAX_EFFECT_STACK_LEN];
    let mut params = EffectStackParameters {
        scratch_budget_bytes: 17 * 13 * 28 * 8,
        ..parameters()
    };
    let initial = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        params,
    )
    .unwrap();
    assert_eq!(
        (initial.pipeline_compiles, initial.texture_allocations),
        (3, 56)
    );
    assert_eq!((initial.blur_passes, initial.composite_passes), (16, 8));
    assert_eq!(scope.scratch_bytes(), params.scratch_budget_bytes);
    let prefix: Vec<_> = scope.passes[..7].iter().map(|pass| pass.revision).collect();
    effects[7] = blur(2.0);
    let edit = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &effects,
        params,
    )
    .unwrap();
    assert_eq!((edit.blur_passes, edit.composite_passes), (2, 1));
    assert_eq!(
        scope.passes[..7]
            .iter()
            .map(|pass| pass.revision)
            .collect::<Vec<_>>(),
        prefix
    );
    let unchanged = scope.output().unwrap().clone();
    params.scratch_budget_bytes -= 1;
    let zero = EffectStackParameters {
        scope_opacity: 0.0,
        ..params
    };
    assert_eq!(
        render(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            capture,
            &effects,
            zero
        ),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
    assert_eq!(
        scope.output(),
        Some(&unchanged),
        "budget rejection preserves warm output"
    );
    params.scratch_budget_bytes += 1;
    for index in 0..20 {
        effects[7] = blur(if index % 2 == 0 { 0.0 } else { 2.0 });
        let work = render(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            capture,
            &effects,
            params,
        )
        .unwrap();
        assert_eq!(work.texture_allocations + work.pipeline_compiles, 0);
        assert_eq!(work.scratch_bytes, params.scratch_budget_bytes);
    }
    let refreshed = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        GlowCapture {
            revision: 2,
            ..capture
        },
        &effects,
        params,
    )
    .unwrap();
    assert_eq!((refreshed.blur_passes, refreshed.composite_passes), (16, 8));
    assert_eq!(refreshed.texture_allocations, 0);
    scope.clear();
    assert_eq!(scope.scratch_bytes(), 0);
}

#[test]
fn blur_unit_underflow_uses_identity_and_final_opacity_belongs_to_the_active_prefix() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let image = texture(&device, [17, 13]);
    let tiny: EffectDefinition = Blur::new(EffectUpdate::default().radius(f64::from_bits(1)))
        .unwrap()
        .into();
    assert!(
        !tiny.is_neutral(),
        "the authored scene radius is strictly positive"
    );
    let params = EffectStackParameters {
        output_height: 1,
        world_view_height: f64::MAX,
        scope_opacity: 0.37,
        ..parameters()
    };
    let mut filter = EffectStackFilter::default();
    let mut scope = EffectStackScope::default();
    let capture = GlowCapture {
        source: &image,
        silhouette: None,
        revision: 1,
    };
    assert_eq!(
        render(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            capture,
            &[tiny],
            params
        )
        .unwrap(),
        EffectRasterStats::default()
    );
    assert!(scope.output().is_none());
    assert!(scope.passes.is_empty());
    assert!(super::super::EffectCaptureTile::prepare(
        super::super::EffectPixelBounds {
            min: [2.0, 2.0],
            max: [5.0, 5.0]
        },
        [17, 13],
        EffectCaptureParameters {
            definition: tiny,
            output_height: params.output_height,
            world_view_height: params.world_view_height,
            scope_opacity: params.scope_opacity
        },
        4096,
        0
    )
    .unwrap()
    .is_none());
    let first = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &[blur(1.0)],
        params,
    )
    .unwrap();
    let output = scope.output().unwrap().clone();
    let same = render(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        capture,
        &[blur(1.0), tiny],
        params,
    )
    .unwrap();
    assert_eq!(
        same,
        EffectRasterStats {
            scratch_bytes: first.scratch_bytes,
            ..Default::default()
        }
    );
    assert_eq!(scope.output(), Some(&output));
}
