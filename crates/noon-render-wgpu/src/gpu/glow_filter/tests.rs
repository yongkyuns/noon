use super::*;
use noon_core::{Color, GlowUpdate, Pixels};

fn parameters(sigma: f64) -> GlowParameters {
    GlowParameters {
        definition: Glow::new(GlowUpdate::default().radius(Pixels(sigma))).unwrap(),
        output_height: 1080,
        world_view_height: 9.0,
        scope_opacity: 1.0,
        scratch_budget_bytes: 32 * 1024 * 1024,
    }
}

#[test]
fn uniform_layout_and_complete_finite_kernel_are_normalized() {
    assert_eq!(size_of::<GlowUniform>(), 1072);
    assert_eq!(std::mem::offset_of!(GlowUniform, tint), 16);
    assert_eq!(std::mem::offset_of!(GlowUniform, control), 32);
    assert_eq!(std::mem::offset_of!(GlowUniform, weights), 48);
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
    params.definition = Glow::new(GlowUpdate::default().radius(0.5)).unwrap();
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
        Err(GlowPrepareError::RadiusExceedsProfile)
    );
    for opacity in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
        let mut params = parameters(3.0);
        params.scope_opacity = opacity;
        assert_eq!(
            GlowUniform::prepare(params, [17, 13], None),
            Err(GlowPrepareError::InvalidScopeOpacity)
        );
    }
    let mut params = parameters(3.0);
    params.output_height = 0;
    assert_eq!(
        GlowUniform::prepare(params, [17, 13], None),
        Err(GlowPrepareError::Parameter(GlowParameterError::InvalidView))
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
) -> GlowRasterStats {
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
        GlowUpdate::default().intensity(0.0),
        GlowUpdate::default().radius(0.0),
        GlowUpdate::default().color(Color::rgba(1.0, 1.0, 1.0, 0.0)),
    ] {
        let mut params = parameters(3.0);
        params.definition = update.apply_to(params.definition).unwrap();
        assert_eq!(
            filter
                .prepare(&device, &queue, &mut scope, capture, params)
                .unwrap(),
            GlowRasterStats::default()
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
    let clean = GlowRasterStats {
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
        params.definition = GlowUpdate::default()
            .intensity(intensity)
            .apply_to(params.definition)
            .unwrap();
        let edit = filter
            .prepare(&device, &queue, &mut scope, capture, params)
            .unwrap();
        assert_eq!(
            edit,
            GlowRasterStats {
                bytes_uploaded: 32,
                ..clean
            }
        );
        let draw = encode_submit(&filter, &mut scope, &device, &queue);
        assert_eq!(
            draw,
            GlowRasterStats {
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
        Err(GlowPrepareError::ScratchBudgetExceeded)
    );
    bad = params;
    bad.definition = GlowUpdate::default()
        .source(GlowSource::Silhouette)
        .apply_to(params.definition)
        .unwrap();
    assert_eq!(
        filter.prepare(&device, &queue, &mut scope, capture, bad),
        Err(GlowPrepareError::MissingSilhouette)
    );
    assert_eq!(scope.output(), Some(&output));
    assert_eq!(scope.uniform, old_uniform);
    assert!(!scope.blur_dirty && !scope.output_dirty);
    let mut changed = params;
    changed.definition = GlowUpdate::default()
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
    let bounds = GlowPixelBounds {
        min: [-8.0, 20.0],
        max: [-4.0, 24.0],
    };
    let tile = GlowCaptureTile::prepare(bounds, [100, 80], parameters(2.0), 4096, 1_000_000)
        .unwrap()
        .expect("offscreen geometry contributes visible halo");
    assert_eq!(tile.support_radius, 6);
    assert_eq!(tile.origin, [-15, 13]);
    assert_eq!(tile.size, [18, 18]);
    assert_eq!(tile.viewport_origin, [0, 13]);
    assert_eq!(tile.local_origin, [15, 0]);
    assert_eq!(tile.visible_size, [3, 18]);
    assert_eq!(tile.capture_and_scratch_bytes, 18 * 18 * 16);
    let far = GlowPixelBounds {
        min: [-80.0, 20.0],
        max: [-70.0, 24.0],
    };
    assert_eq!(
        GlowCaptureTile::prepare(far, [100, 80], parameters(2.0), 4096, 1_000_000),
        Ok(None)
    );
}

#[test]
fn capture_tile_rejects_invalid_bounds_projection_and_memory() {
    let bounds = GlowPixelBounds {
        min: [5.0, 5.0],
        max: [10.0, 10.0],
    };
    let params = parameters(2.0);
    assert_eq!(
        GlowCaptureTile::prepare(bounds, [0, 80], params, 4096, 1_000_000),
        Err(GlowPrepareError::Parameter(GlowParameterError::InvalidView))
    );
    for invalid in [
        GlowPixelBounds {
            min: [f64::NAN, 0.0],
            max: [10.0, 10.0],
        },
        GlowPixelBounds {
            min: [10.0, 0.0],
            max: [10.0, 10.0],
        },
        GlowPixelBounds {
            min: [0.0, 0.0],
            max: [f64::INFINITY, 10.0],
        },
    ] {
        assert_eq!(
            GlowCaptureTile::prepare(invalid, [100, 80], params, 4096, 1_000_000),
            Err(GlowPrepareError::InvalidSourceBounds)
        );
    }
    assert_eq!(
        GlowCaptureTile::prepare(
            GlowPixelBounds {
                min: [3.0e15, 5.0],
                max: [3.0e15 + 2.0, 10.0]
            },
            [100, 80],
            params,
            4096,
            1_000_000,
        ),
        Err(GlowPrepareError::CaptureCoordinatesOutOfRange)
    );
    assert_eq!(
        GlowCaptureTile::prepare(bounds, [100, 80], params, 4, 1_000_000),
        Err(GlowPrepareError::ExtentExceedsDevice)
    );
    assert_eq!(
        GlowCaptureTile::prepare(bounds, [100, 80], params, 4096, 1),
        Err(GlowPrepareError::ScratchBudgetExceeded)
    );
}

#[test]
fn capture_tile_accounts_for_silhouette_and_neutral_allocates_nothing() {
    let bounds = GlowPixelBounds {
        min: [1.0, 1.0],
        max: [3.0, 3.0],
    };
    let painted = GlowCaptureTile::prepare(bounds, [100, 80], parameters(2.0), 4096, 1_000_000)
        .unwrap()
        .unwrap();
    let mut silhouette = parameters(2.0);
    silhouette.definition = GlowUpdate::default()
        .source(GlowSource::Silhouette)
        .apply_to(silhouette.definition)
        .unwrap();
    let other = GlowCaptureTile::prepare(bounds, [100, 80], silhouette, 4096, 1_000_000)
        .unwrap()
        .unwrap();
    assert_eq!(other.size, painted.size);
    assert_eq!(
        other.capture_and_scratch_bytes * 4,
        painted.capture_and_scratch_bytes * 5
    );
    silhouette.definition = GlowUpdate::default()
        .intensity(0.0)
        .apply_to(silhouette.definition)
        .unwrap();
    assert_eq!(
        GlowCaptureTile::prepare(bounds, [100, 80], silhouette, 4096, 0),
        Ok(None)
    );
}

#[test]
fn analytic_projection_uses_output_pixels_for_rotated_scaled_shapes() {
    use noon_core::{GeometryRef, Transform2D, Vec2};
    let camera = super::super::Camera2D::new(Vec2::ZERO, Vec2::new(20.0, 10.0)).unwrap();
    let circle = GlowPixelBounds::projected_analytic(
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
    let ellipse =
        GlowPixelBounds::projected_analytic(&GeometryRef::circle(1.0), rotated, camera, [200, 100])
            .unwrap();
    assert!((ellipse.min[0] - 95.0).abs() < 1e-5);
    assert!((ellipse.min[1] - 30.0).abs() < 1e-5);
    let rectangle = GlowPixelBounds::projected_analytic(
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
        GlowPixelBounds::projected_analytic(
            &GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            camera,
            [100, 100],
        ),
        Err(GlowPrepareError::InvalidProjection)
    );
    assert_eq!(
        GlowPixelBounds::projected_analytic(
            &GeometryRef::circle(1.0),
            Transform2D {
                scale: Vec2::ZERO,
                ..Transform2D::IDENTITY
            },
            camera,
            [200, 100],
        ),
        Err(GlowPrepareError::InvalidProjection)
    );
    assert_eq!(
        GlowPixelBounds::projected_analytic(
            &GeometryRef::circle(0.0),
            Transform2D::IDENTITY,
            camera,
            [200, 100],
        ),
        Err(GlowPrepareError::InvalidSourceBounds)
    );
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
mod analytic_scene;
