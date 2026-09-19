use super::*;
use noon_core::Vec2;

fn circle(x: f32) -> AnalyticOverlay {
    AnalyticOverlay::new(
        &GeometryRef::Circle { radius: 1.0 },
        Transform2D {
            translation: Vec2::new(x, 0.0),
            ..Transform2D::default()
        },
        Color::rgba(1.0, 1.0, 0.0, 0.35),
    )
    .unwrap()
}

#[test]
fn overlay_preparation_preserves_effective_reflections_and_nonuniform_scale() {
    let transform = Transform2D {
        translation: Vec2::new(2.0, -3.0),
        scale: Vec2::new(-2.0, 0.5),
        rotation: 0.7,
    };
    let overlay = AnalyticOverlay::new(
        &GeometryRef::Circle { radius: 1.0 },
        transform,
        Color::rgba(1.0, 1.0, 0.0, 0.35),
    )
    .unwrap();
    let OverlayInstance::Circle(instance) = overlay.instance else {
        panic!("circle")
    };
    assert_eq!(instance.transform, transform.into());
    assert_eq!(
        instance.padding[0], 1.0,
        "complete analytic fill, not Create progress"
    );
    assert_eq!(instance.style.stroke_enabled, 0);
    assert_eq!(instance.style.fill, [1.0, 1.0, 0.0, 0.35]);
    assert_eq!(instance.style.opacity, 1.0);
}

#[test]
fn overlay_preparation_rejects_unsupported_degenerate_and_nonfinite_values() {
    let transform = Transform2D::default();
    let color = Color::rgba(1.0, 1.0, 0.0, 0.35);
    assert_eq!(
        AnalyticOverlay::new(
            &GeometryRef::Line {
                start: Vec2::ZERO,
                end: Vec2::new(1.0, 0.0)
            },
            transform,
            color
        ),
        Err(OverlayPrepareError::UnsupportedGeometry)
    );
    for radius in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            AnalyticOverlay::new(&GeometryRef::Circle { radius }, transform, color),
            Err(OverlayPrepareError::InvalidGeometry)
        );
    }
    for size in [Vec2::ZERO, Vec2::new(1.0, f32::NAN), Vec2::new(-1.0, 1.0)] {
        assert_eq!(
            AnalyticOverlay::new(&GeometryRef::Rectangle { size }, transform, color),
            Err(OverlayPrepareError::InvalidGeometry)
        );
    }
    for bad in [
        Transform2D {
            scale: Vec2::ZERO,
            ..transform
        },
        Transform2D {
            rotation: f32::NAN,
            ..transform
        },
        Transform2D {
            translation: Vec2::new(f32::INFINITY, 0.0),
            ..transform
        },
    ] {
        assert_eq!(
            AnalyticOverlay::new(&GeometryRef::Circle { radius: 1.0 }, bad, color),
            Err(OverlayPrepareError::InvalidTransform)
        );
    }
    for bad in [
        Color::rgba(1.0, 1.0, 0.0, f32::NAN),
        Color::rgba(2.0, 0.0, 0.0, 1.0),
    ] {
        assert_eq!(
            AnalyticOverlay::new(&GeometryRef::Circle { radius: 1.0 }, transform, bad),
            Err(OverlayPrepareError::InvalidColor)
        );
    }
}

#[test]
fn overlay_upload_is_lazy_fixed_size_and_reuses_unchanged_instances() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut state = OverlayGpuState::default();
    assert_eq!(state.capacity_bytes(), 0);
    assert_eq!(state.update(&device, &queue, None), UploadStats::default());
    let first = state.update(&device, &queue, Some(circle(0.0)));
    assert_eq!(first.buffer_reallocations, 1);
    assert_eq!(first.bytes_uploaded, std::mem::size_of::<CircleInstance>());
    assert_eq!(
        state.update(&device, &queue, Some(circle(0.0))),
        UploadStats::default()
    );
    for i in 0..1000 {
        let shape = if i % 2 == 0 {
            circle(i as f32)
        } else {
            AnalyticOverlay::new(
                &GeometryRef::Rectangle {
                    size: Vec2::new(2.0, 1.0),
                },
                Transform2D::default(),
                Color::rgba(0.0, 1.0, 1.0, 0.5),
            )
            .unwrap()
        };
        let update = state.update(&device, &queue, Some(shape));
        assert_eq!(update.buffer_reallocations, 0);
        assert!(update.bytes_uploaded <= std::mem::size_of::<CircleInstance>());
        assert_eq!(
            state.capacity_bytes(),
            std::mem::size_of::<CircleInstance>()
        );
        assert_eq!(state.update(&device, &queue, None), UploadStats::default());
    }
    assert_eq!(
        std::mem::size_of::<CircleInstance>(),
        std::mem::size_of::<RectangleInstance>()
    );
}

#[test]
fn overlay_draw_is_explicit_and_uses_no_stable_instance_storage() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut overlay = OverlayGpuState::default();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    for shape in [Some(circle(0.0)), None] {
        overlay.update(&device, &queue, shape);
        let mut encoder = device.create_command_encoder(&Default::default());
        assert_eq!(
            renderer.encode_overlay(&mut encoder, &view, None),
            DrawStats::default()
        );
        let draws = renderer.encode_overlay(&mut encoder, &view, Some(&overlay));
        assert_eq!(draws.draw_calls, usize::from(shape.is_some()));
        assert_eq!(draws.instances_drawn, usize::from(shape.is_some()));
        queue.submit([encoder.finish()]);
    }
    assert_eq!(renderer.circle_capacity_bytes(), 0);
    assert_eq!(renderer.rectangle_capacity_bytes(), 0);
}
