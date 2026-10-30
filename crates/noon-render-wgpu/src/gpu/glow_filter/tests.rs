use super::*;
use noon_core::{Color, EffectUpdate, Pixels};

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn only_effect(stack: &noon_compile::CompiledEffectStack) -> noon_compile::CompiledEffect {
    assert_eq!(stack.len(), 1, "single-effect reference fixture");
    stack.iter().next().unwrap()
}

fn parameters(sigma: f64) -> GlowParameters {
    GlowParameters {
        definition: Glow::new(EffectUpdate::default().radius(Pixels(sigma))).unwrap(),
        output_height: 1080,
        world_view_height: 9.0,
        scope_opacity: 1.0,
        scratch_budget_bytes: 32 * 1024 * 1024,
    }
}

#[test]
fn ancestor_original_coverage_is_budgeted_without_an_authored_filter() {
    let bounds = EffectPixelBounds {
        min: [17.0, 25.0],
        max: [25.0, 33.0],
    };
    let parameters = super::super::EffectStackParameters {
        shader: None,
        working_format: wgpu::TextureFormat::Rgba8Unorm,
        output_height: 64,
        world_view_height: 6.4,
        scope_opacity: 1.0,
        scratch_budget_bytes: 800,
        source_bounds: None,
    };
    let prepare = |demand, budget| {
        EffectCaptureTile::prepare_stack_input(
            bounds,
            [64; 2],
            &[],
            parameters,
            1024,
            budget,
            demand,
        )
    };
    assert_eq!(prepare(CaptureDemand::default(), 800), Ok(None));
    let demand = CaptureDemand {
        defer_glow_blur: false,
        required_by_parent: true,
        original_by_parent: true,
        isolate: false,
        padding_pixels: 0,
    };
    // Eight by eight pixels plus a one-pixel coverage fringe, with one painted
    // and one original RGBA8 image. No filter scratch or invented attachment.
    let tile = prepare(demand, 800).unwrap().unwrap();
    assert_eq!(tile.size, [10; 2]);
    assert_eq!(tile.capture_and_scratch_bytes, 800);
    assert_eq!(
        prepare(demand, 799),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
}

#[test]
fn uniform_layout_and_complete_finite_kernel_are_normalized() {
    assert_eq!(size_of::<GlowUniform>(), 1088);
    assert_eq!(std::mem::offset_of!(GlowUniform, tint), 16);
    assert_eq!(std::mem::offset_of!(GlowUniform, control), 32);
    assert_eq!(std::mem::offset_of!(GlowUniform, source_bounds), 48);
    assert_eq!(std::mem::offset_of!(GlowUniform, weights), 64);
    for sigma in [f64::MIN_POSITIVE, 0.01, 0.5, 1.0, 3.25, 12.0, 64.0] {
        let (_, uniform) = GlowUniform::prepare(parameters(sigma), [17, 13], None).unwrap();
        assert_eq!(uniform.radius, (3.0 * sigma).ceil() as u32);
        let coefficients: Vec<_> = uniform.weights.into_iter().flatten().collect();
        let mass = f64::from(coefficients[0])
            + 2.0
                * coefficients[1..=uniform.radius as usize]
                    .iter()
                    .map(|&value| f64::from(value))
                    .sum::<f64>();
        assert!((mass - 1.0).abs() < 2e-7, "sigma={sigma}, mass={mass}");
        assert!(coefficients[uniform.radius as usize + 1..]
            .iter()
            .all(|&x| x == 0.0));
    }
}

#[test]
fn world_sigma_uses_full_output_not_crop_and_pixels_remain_fixed() {
    let mut params = parameters(12.0);
    params.definition = Glow::new(EffectUpdate::default().radius(0.5)).unwrap();
    let (sigma, _) = GlowUniform::prepare(params, [17, 13], None).unwrap();
    assert_eq!(sigma, 60.0);
    let mut pixels = parameters(12.0);
    pixels.output_height = 2160;
    pixels.world_view_height = 4.5;
    assert_eq!(
        GlowUniform::prepare(pixels, [17, 13], None).unwrap().0,
        12.0
    );
}

#[test]
fn bad_view_radius_and_opacity_are_rejected() {
    assert_eq!(
        GlowUniform::prepare(parameters(64.001), [17, 13], None),
        Err(EffectPrepareError::RadiusExceedsProfile)
    );
    for opacity in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
        let mut params = parameters(3.0);
        params.scope_opacity = opacity;
        assert_eq!(
            GlowUniform::prepare(params, [17, 13], None),
            Err(EffectPrepareError::InvalidScopeOpacity)
        );
    }
    let mut params = parameters(3.0);
    params.output_height = 0;
    assert_eq!(
        GlowUniform::prepare(params, [17, 13], None),
        Err(EffectPrepareError::Parameter(
            EffectParameterError::InvalidView
        ))
    );
}

fn captured_image(device: &wgpu::Device, size: [u32; 2]) -> Image {
    Image::new(device, size, "Noon glow test source")
}

fn encode_submit(
    filter: &GlowFilter,
    scope: &mut GlowScope,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> EffectRasterStats {
    let mut encoder = device.create_command_encoder(&Default::default());
    let stats = filter.encode(&mut encoder, scope);
    queue.submit([encoder.finish()]);
    stats
}

// Noop tests qualify resource accounting and command validation, never pixels.
#[cfg(feature = "ci-noop")]
#[test]
fn neutral_is_lazy_and_clean_or_intensity_edits_reuse_resources() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let source = captured_image(&device, [17, 13]);
    let capture = GlowCapture {
        source: &source.texture,
        silhouette: None,
        revision: 1,
    };
    let mut filter = GlowFilter::default();
    let mut scope = GlowScope::default();
    for update in [
        EffectUpdate::default().intensity(0.0),
        EffectUpdate::default().radius(0.0),
        EffectUpdate::default().color(Color::rgba(1.0, 1.0, 1.0, 0.0)),
    ] {
        let mut params = parameters(3.0);
        params.definition = update.apply_to(params.definition).unwrap();
        assert_eq!(
            filter
                .prepare(&device, &queue, &mut scope, capture, params)
                .unwrap(),
            EffectRasterStats::default()
        );
        assert!(scope.output().is_none());
        assert!(filter.programs.is_none());
    }
    let params = parameters(3.0);
    let first = filter
        .prepare(&device, &queue, &mut scope, capture, params)
        .unwrap();
    assert_eq!(
        (
            first.pipeline_compiles,
            first.kernel_builds,
            first.texture_allocations,
            first.buffer_allocations,
            first.bind_group_creations
        ),
        (3, 1, 3, 1, 3)
    );
    assert_eq!(first.bytes_uploaded, size_of::<GlowUniform>());
    assert_eq!(scope.scratch_bytes(), 17 * 13 * 12);
    let output = scope.output().unwrap().clone();
    assert!(scope.output_view().is_some());
    let first = encode_submit(&filter, &mut scope, &device, &queue);
    assert_eq!((first.blur_passes, first.composite_passes), (2, 1));
    let clean = EffectRasterStats {
        scratch_bytes: scope.scratch_bytes(),
        ..Default::default()
    };
    assert_eq!(
        filter
            .prepare(&device, &queue, &mut scope, capture, params)
            .unwrap(),
        clean
    );
    assert_eq!(encode_submit(&filter, &mut scope, &device, &queue), clean);
    for intensity in [0.1, 1.5, 8.0, 0.25] {
        let mut params = params;
        params.definition = EffectUpdate::default()
            .intensity(intensity)
            .apply_to(params.definition)
            .unwrap();
        let edit = filter
            .prepare(&device, &queue, &mut scope, capture, params)
            .unwrap();
        assert_eq!(
            edit,
            EffectRasterStats {
                bytes_uploaded: 32,
                ..clean
            }
        );
        let draw = encode_submit(&filter, &mut scope, &device, &queue);
        assert_eq!(
            draw,
            EffectRasterStats {
                composite_passes: 1,
                ..clean
            }
        );
        assert_eq!(scope.output(), Some(&output));
    }
}

#[cfg(feature = "ci-noop")]
#[test]
fn failure_preserves_previous_scope_and_radius_revision_resize_invalidate_locally() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let source = captured_image(&device, [17, 13]);
    let capture = GlowCapture {
        source: &source.texture,
        silhouette: None,
        revision: 1,
    };
    let mut filter = GlowFilter::default();
    let mut scope = GlowScope::default();
    let params = parameters(3.0);
    filter
        .prepare(&device, &queue, &mut scope, capture, params)
        .unwrap();
    encode_submit(&filter, &mut scope, &device, &queue);
    let output = scope.output().unwrap().clone();
    assert!(scope.output_view().is_some());
    let old_uniform = scope.uniform;
    let mut bad = params;
    bad.scratch_budget_bytes = 17 * 13 * 12 - 1;
    assert_eq!(
        filter.prepare(&device, &queue, &mut scope, capture, bad),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
    bad = params;
    bad.definition = EffectUpdate::default()
        .source(GlowSource::Silhouette)
        .apply_to(params.definition)
        .unwrap();
    assert_eq!(
        filter.prepare(&device, &queue, &mut scope, capture, bad),
        Err(EffectPrepareError::MissingSilhouette)
    );
    assert_eq!(scope.output(), Some(&output));
    assert_eq!(scope.uniform, old_uniform);
    assert!(!scope.blur_dirty && !scope.output_dirty);
    let mut changed = params;
    changed.definition = EffectUpdate::default()
        .radius(Pixels(5.0))
        .apply_to(params.definition)
        .unwrap();
    let edit = filter
        .prepare(&device, &queue, &mut scope, capture, changed)
        .unwrap();
    assert_eq!(
        (
            edit.kernel_builds,
            edit.texture_allocations,
            edit.pipeline_compiles
        ),
        (1, 0, 0)
    );
    assert_eq!(edit.bytes_uploaded, size_of::<GlowUniform>());
    assert_eq!(
        encode_submit(&filter, &mut scope, &device, &queue).blur_passes,
        2
    );
    let edit = filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            GlowCapture {
                revision: 2,
                ..capture
            },
            changed,
        )
        .unwrap();
    assert_eq!(
        edit.kernel_builds + edit.texture_allocations + edit.bytes_uploaded,
        0
    );
    assert_eq!(
        encode_submit(&filter, &mut scope, &device, &queue).blur_passes,
        2
    );
    scope.invalidate();
    assert_eq!(
        encode_submit(&filter, &mut scope, &device, &queue).blur_passes,
        2
    );
    let resized = captured_image(&device, [19, 15]);
    let edit = filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            GlowCapture {
                source: &resized.texture,
                silhouette: None,
                revision: 3,
            },
            changed,
        )
        .unwrap();
    assert_eq!(
        (
            edit.pipeline_compiles,
            edit.texture_allocations,
            edit.kernel_builds
        ),
        (0, 3, 0)
    );
    scope.clear();
    assert!(scope.output().is_none());
    assert_eq!(scope.scratch_bytes(), 0);
}

#[cfg(feature = "ci-noop")]
#[test]
fn separate_scopes_share_programs_but_not_textures_or_dirty_state() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let source = captured_image(&device, [17, 13]);
    let capture = GlowCapture {
        source: &source.texture,
        silhouette: None,
        revision: 1,
    };
    let mut filter = GlowFilter::default();
    let mut a = GlowScope::default();
    let mut b = GlowScope::default();
    filter
        .prepare(&device, &queue, &mut a, capture, parameters(3.0))
        .unwrap();
    encode_submit(&filter, &mut a, &device, &queue);
    let stats = filter
        .prepare(&device, &queue, &mut b, capture, parameters(3.0))
        .unwrap();
    assert_eq!(stats.pipeline_compiles, 0);
    assert_ne!(a.output(), b.output());
    assert!(!a.blur_dirty && !a.output_dirty);
    assert!(b.blur_dirty && b.output_dirty);
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
mod pixels;

#[test]
fn padded_capture_preserves_offscreen_halo_and_skips_truly_invisible_source() {
    let bounds = EffectPixelBounds {
        min: [-8.0, 20.0],
        max: [-4.0, 24.0],
    };
    let tile = EffectCaptureTile::prepare(bounds, [100, 80], parameters(2.0), 4096, 1_000_000)
        .unwrap()
        .expect("offscreen geometry contributes visible halo");
    assert_eq!(tile.support_radius, 6);
    assert_eq!(tile.origin, [-16, 12]);
    assert_eq!(tile.size, [19, 19]);
    assert_eq!(tile.viewport_origin, [0, 12]);
    assert_eq!(tile.local_origin, [16, 0]);
    assert_eq!(tile.visible_size, [3, 19]);
    assert_eq!(tile.capture_and_scratch_bytes, 19 * 19 * 16);
    let far = EffectPixelBounds {
        min: [-80.0, 20.0],
        max: [-70.0, 24.0],
    };
    assert_eq!(
        EffectCaptureTile::prepare(far, [100, 80], parameters(2.0), 4096, 1_000_000),
        Ok(None)
    );
}

#[test]
fn ordered_capture_uses_cumulative_support_and_one_shared_source_budget() {
    let definitions = [
        noon_core::Blur::new(EffectUpdate::default().radius(Pixels(2.0)))
            .unwrap()
            .into(),
        noon_core::Blur::new(EffectUpdate::default().radius(Pixels(3.0)))
            .unwrap()
            .into(),
    ];
    let values = super::super::EffectStackParameters {
        shader: None,
        working_format: wgpu::TextureFormat::Rgba8Unorm,
        output_height: 100,
        world_view_height: 10.0,
        scope_opacity: 0.5,
        scratch_budget_bytes: 1_000_000,
        source_bounds: None,
    };
    let bounds = EffectPixelBounds {
        min: [-16.0, 50.0],
        max: [-14.0, 52.0],
    };
    assert!(EffectCaptureTile::prepare_stack(
        bounds,
        [100, 100],
        &definitions[..1],
        values,
        4096,
        1_000_000
    )
    .unwrap()
    .is_none());
    let tile =
        EffectCaptureTile::prepare_stack(bounds, [100, 100], &definitions, values, 4096, 1_000_000)
            .unwrap()
            .unwrap();
    assert_eq!(tile.origin, [-32, 34]);
    assert_eq!(tile.size, [34, 34]);
    assert_eq!(tile.support_radius, 15);
    assert_eq!(tile.capture_and_scratch_bytes, 34 * 34 * (4 + 28 + 28));
    assert_eq!(tile.visible_size, [2, 34]);
    assert_eq!(
        EffectCaptureTile::prepare_stack(
            bounds,
            [100, 100],
            &definitions,
            values,
            4096,
            tile.capture_and_scratch_bytes - 1
        ),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
    let invalid_suffix = [
        definitions[0].clone(),
        noon_core::Blur::new(EffectUpdate::default().radius(Pixels(65.0)))
            .unwrap()
            .into(),
    ];
    assert_eq!(
        EffectCaptureTile::prepare_stack(
            bounds,
            [100, 100],
            &invalid_suffix,
            values,
            4096,
            1_000_000
        ),
        Err(EffectPrepareError::RadiusExceedsProfile)
    );
    assert_eq!(
        EffectCaptureTile::prepare_stack(
            bounds,
            [100, 100],
            &std::array::from_fn::<_, { noon_core::MAX_EFFECT_STACK_LEN + 1 }, _>(|_| definitions
                [0]
            .clone()),
            values,
            4096,
            1_000_000
        ),
        Err(EffectPrepareError::StackExceedsProfile)
    );
}

#[test]
fn silhouette_suffix_expands_original_geometry_instead_of_the_painted_prefix() {
    let a: noon_core::EffectDefinition = Glow::new(EffectUpdate::default().radius(Pixels(3.0)))
        .unwrap()
        .into();
    let b = EffectUpdate::default()
        .radius(Pixels(1.0))
        .source(GlowSource::Silhouette)
        .apply_to(a.clone())
        .unwrap();
    let bounds = EffectPixelBounds {
        min: [20.0, 20.0],
        max: [24.0, 24.0],
    };
    let values = super::super::EffectStackParameters {
        shader: None,
        working_format: wgpu::TextureFormat::Rgba8Unorm,
        output_height: 100,
        world_view_height: 10.0,
        scope_opacity: 1.0,
        scratch_budget_bytes: 1_000_000,
        source_bounds: None,
    };
    let silhouette = EffectCaptureTile::prepare_stack(
        bounds,
        [100, 100],
        &[a.clone(), b.clone()],
        values,
        4096,
        1_000_000,
    )
    .unwrap()
    .unwrap();
    assert_eq!(silhouette.origin, [10, 10]);
    assert_eq!(silhouette.size, [24, 24]);
    assert_eq!(
        silhouette.capture_and_scratch_bytes,
        24 * 24 * (4 + 4 + 12 + 12)
    );
    let painted = EffectUpdate::default()
        .source(GlowSource::Painted)
        .apply_to(b)
        .unwrap();
    let painted = EffectCaptureTile::prepare_stack(
        bounds,
        [100, 100],
        &[a, painted],
        values,
        4096,
        1_000_000,
    )
    .unwrap()
    .unwrap();
    assert_eq!(painted.origin, [6, 6]);
    assert_eq!(painted.size, [31, 31]);
}

#[test]
fn capture_tile_rejects_invalid_bounds_projection_and_memory() {
    let bounds = EffectPixelBounds {
        min: [5.0, 5.0],
        max: [10.0, 10.0],
    };
    let params = parameters(2.0);
    assert_eq!(
        EffectCaptureTile::prepare(bounds, [0, 80], params, 4096, 1_000_000),
        Err(EffectPrepareError::Parameter(
            EffectParameterError::InvalidView
        ))
    );
    for invalid in [
        EffectPixelBounds {
            min: [f64::NAN, 0.0],
            max: [10.0, 10.0],
        },
        EffectPixelBounds {
            min: [10.0, 0.0],
            max: [10.0, 10.0],
        },
        EffectPixelBounds {
            min: [0.0, 0.0],
            max: [f64::INFINITY, 10.0],
        },
    ] {
        assert_eq!(
            EffectCaptureTile::prepare(invalid, [100, 80], params, 4096, 1_000_000),
            Err(EffectPrepareError::InvalidSourceBounds)
        );
    }
    assert_eq!(
        EffectCaptureTile::prepare(
            EffectPixelBounds {
                min: [3.0e15, 5.0],
                max: [3.0e15 + 2.0, 10.0]
            },
            [100, 80],
            params,
            4096,
            1_000_000,
        ),
        Err(EffectPrepareError::CaptureCoordinatesOutOfRange)
    );
    assert_eq!(
        EffectCaptureTile::prepare(bounds, [100, 80], params, 4, 1_000_000),
        Err(EffectPrepareError::ExtentExceedsDevice)
    );
    assert_eq!(
        EffectCaptureTile::prepare(bounds, [100, 80], params, 4096, 1),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
}

#[test]
fn capture_tile_accounts_for_silhouette_and_neutral_allocates_nothing() {
    let bounds = EffectPixelBounds {
        min: [1.0, 1.0],
        max: [3.0, 3.0],
    };
    let painted = EffectCaptureTile::prepare(bounds, [100, 80], parameters(2.0), 4096, 1_000_000)
        .unwrap()
        .unwrap();
    let mut silhouette = parameters(2.0);
    silhouette.definition = EffectUpdate::default()
        .source(GlowSource::Silhouette)
        .apply_to(silhouette.definition)
        .unwrap();
    let other = EffectCaptureTile::prepare(bounds, [100, 80], silhouette, 4096, 1_000_000)
        .unwrap()
        .unwrap();
    assert_eq!(other.size, painted.size);
    assert_eq!(
        other.capture_and_scratch_bytes * 4,
        painted.capture_and_scratch_bytes * 5
    );
    silhouette.definition = EffectUpdate::default()
        .intensity(0.0)
        .apply_to(silhouette.definition)
        .unwrap();
    assert_eq!(
        EffectCaptureTile::prepare(bounds, [100, 80], silhouette, 4096, 0),
        Ok(None)
    );
}

#[test]
fn analytic_projection_uses_output_pixels_for_rotated_scaled_shapes() {
    use noon_core::{GeometryRef, Transform2D, Vec2};
    let camera = super::super::Camera2D::new(Vec2::ZERO, Vec2::new(20.0, 10.0)).unwrap();
    let circle = EffectPixelBounds::projected_analytic(
        &GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        camera,
        [200, 100],
    )
    .unwrap();
    assert_eq!(circle.min, [90.0, 40.0]);
    assert_eq!(circle.max, [110.0, 60.0]);
    let rotated = Transform2D {
        scale: Vec2::new(-2.0, 0.5),
        rotation: std::f32::consts::FRAC_PI_2,
        ..Transform2D::IDENTITY
    };
    let ellipse = EffectPixelBounds::projected_analytic(
        &GeometryRef::circle(1.0),
        rotated,
        camera,
        [200, 100],
    )
    .unwrap();
    assert!((ellipse.min[0] - 95.0).abs() < 1e-5);
    assert!((ellipse.min[1] - 30.0).abs() < 1e-5);
    let rectangle = EffectPixelBounds::projected_analytic(
        &GeometryRef::rectangle(2.0, 4.0),
        Transform2D {
            rotation: std::f32::consts::FRAC_PI_4,
            ..Transform2D::IDENTITY
        },
        camera,
        [200, 100],
    )
    .unwrap();
    let radius = 10.0 * 3.0 * (std::f64::consts::FRAC_PI_4).cos();
    assert!((rectangle.min[0] - (100.0 - radius)).abs() < 1e-5);
    assert!((rectangle.min[1] - (50.0 - radius)).abs() < 1e-5);
}

#[test]
fn analytic_projection_rejects_nonuniform_camera_and_bad_geometry() {
    use noon_core::{GeometryRef, Transform2D, Vec2};
    let camera = super::super::Camera2D::new(Vec2::ZERO, Vec2::new(20.0, 10.0)).unwrap();
    assert_eq!(
        EffectPixelBounds::projected_analytic(
            &GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            camera,
            [100, 100],
        ),
        Err(EffectPrepareError::InvalidProjection)
    );
    assert_eq!(
        EffectPixelBounds::projected_analytic(
            &GeometryRef::circle(1.0),
            Transform2D {
                scale: Vec2::ZERO,
                ..Transform2D::IDENTITY
            },
            camera,
            [200, 100],
        ),
        Err(EffectPrepareError::InvalidProjection)
    );
    assert_eq!(
        EffectPixelBounds::projected_analytic(
            &GeometryRef::circle(0.0),
            Transform2D::IDENTITY,
            camera,
            [200, 100],
        ),
        Err(EffectPrepareError::InvalidSourceBounds)
    );
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
mod analytic_scene;

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
mod animated_scene;

#[test]
fn capture_origins_preserve_derivative_quads_across_fractional_motion_and_radius_changes() {
    for left in [-21.3, -20.0, -19.7, -1.2, 0.0, 0.3, 7.8, 8.0, 9.1] {
        for sigma in [0.25, 1.1, 3.25, 4.45, 6.0] {
            let bounds = EffectPixelBounds {
                min: [left, 11.3],
                max: [left + 3.0, 14.4],
            };
            let Some(tile) =
                EffectCaptureTile::prepare(bounds, [100, 80], parameters(sigma), 4096, 1_000_000)
                    .unwrap()
            else {
                continue;
            };
            let padding = (3.0 * sigma).ceil() + 1.0;
            for axis in 0..2 {
                assert_eq!(
                    tile.origin[axis].rem_euclid(2),
                    0,
                    "local raster quad grid must match output"
                );
                assert!(f64::from(tile.origin[axis]) <= bounds.min[axis] - padding);
                assert!(
                    f64::from(tile.origin[axis]) + f64::from(tile.size[axis])
                        >= bounds.max[axis] + padding
                );
                assert_eq!(
                    i64::from(tile.origin[axis]) + i64::from(tile.local_origin[axis]),
                    i64::from(tile.viewport_origin[axis])
                );
            }
        }
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
mod retained_host;

#[test]
fn logical_projection_preserves_fractional_and_decorated_backing_pixels() {
    use noon_core::{GeometryRef, Transform2D, Vec2};
    // Same inputs used by the browser: CSS/pointer view and independently
    // rounded backing dimensions, including the shipped one-pixel border.
    for (logical, viewport) in [
        (Vec2::new(704.0, 396.0), [702, 394]),
        (Vec2::new(704.0, 396.0), [1404, 788]),
        (Vec2::new(513.25, 18477.0 / 64.0), [511, 287]),
        (Vec2::new(513.25, 18477.0 / 64.0), [639, 359]),
    ] {
        let camera =
            super::super::Camera2D::new(Vec2::ZERO, Vec2::new(8.0 * logical.x / logical.y, 8.0))
                .unwrap();
        let bounds = EffectPixelBounds::projected_analytic_in_view(
            &GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            camera,
            viewport,
            logical,
        )
        .unwrap();
        let half = [
            f64::from(viewport[0]) / f64::from(camera.world_size.x),
            f64::from(viewport[1]) / f64::from(camera.world_size.y),
        ];
        assert_eq!(
            bounds.min,
            [
                f64::from(viewport[0]) * 0.5 - half[0],
                f64::from(viewport[1]) * 0.5 - half[1]
            ]
        );
        assert_eq!(
            bounds.max,
            [
                f64::from(viewport[0]) * 0.5 + half[0],
                f64::from(viewport[1]) * 0.5 + half[1]
            ]
        );
        // The old public physical-only contract remains strict: no arbitrary
        // nonuniform logical camera has been admitted by loosening a tolerance.
        assert_eq!(
            EffectPixelBounds::projected_analytic(
                &GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                camera,
                viewport,
            ),
            Err(EffectPrepareError::InvalidProjection)
        );
    }
}

#[test]
fn logical_projection_rejects_invalid_or_nonuniform_logical_views() {
    use noon_core::{GeometryRef, Transform2D, Vec2};
    let camera = super::super::Camera2D::new(Vec2::ZERO, Vec2::new(20.0, 10.0)).unwrap();
    for logical in [
        Vec2::new(100.0, 100.0),
        Vec2::ZERO,
        Vec2::new(f32::NAN, 100.0),
        Vec2::new(200.0, f32::INFINITY),
    ] {
        assert_eq!(
            EffectPixelBounds::projected_analytic_in_view(
                &GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                camera,
                [200, 100],
                logical,
            ),
            Err(EffectPrepareError::InvalidProjection)
        );
    }
}
