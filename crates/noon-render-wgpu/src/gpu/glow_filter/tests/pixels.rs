//! Explicit native GPU readback. Absence of a real raster adapter is a failure,
//! not a successful skip. Synthetic effective frame inputs exercise the existing
//! primitive renderer, not Scene lowering, Python binding, or physical performance.
use super::*;
use crate::{Camera2D, FramePreparer, GpuRenderer};
use noon_core::{GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, Vec2};
use noon_runtime::{FrameObjectState, FrameState};
use std::time::Duration;

pub(super) fn device() -> (wgpu::Device, wgpu::Queue) {
    let backends = match std::env::var("NOON_GLOW_BACKEND").as_deref() {
        Ok("vulkan") => wgpu::Backends::VULKAN,
        Ok("gl") => wgpu::Backends::GL,
        Ok("metal") => wgpu::Backends::METAL,
        Ok("dx12") => wgpu::Backends::DX12,
        Err(std::env::VarError::NotPresent) => wgpu::Backends::PRIMARY,
        other => panic!("unsupported NOON_GLOW_BACKEND: {other:?}"),
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("glow pixel qualification requires a raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("glow raster adapter: {:?}", adapter.get_info());
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("glow pixel qualification requires a device")
}

pub(super) fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Vec<u8> {
    let row = texture.width() * 4;
    let stride =
        row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Noon glow qualification readback"),
        size: u64::from(stride) * u64::from(texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .expect("glow readback must finish without a poll error");
    rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
    let bytes = buffer
        .slice(..)
        .get_mapped_range()
        .expect("glow readback range must be mapped");
    let result = bytes
        .chunks_exact(stride as usize)
        .flat_map(|row_bytes| row_bytes[..row as usize].iter().copied())
        .collect();
    drop(bytes);
    buffer.unmap();
    result
}

fn primitive_capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: [u32; 2],
    rectangle: bool,
    color: Color,
    translation: Vec2,
) -> Image {
    let image = captured_image(device, size);
    let geometry = if rectangle {
        GeometryRef::rectangle(0.91, 0.57)
    } else {
        GeometryRef::circle(0.43)
    };
    let frame = FrameState {
        time: 0.0,
        objects: vec![FrameObjectState {
            id: ObjectId::new(1),
            content: ObjectContentRef::Geometry(geometry),
            text_bounds: None,
            spatial: None,
            z_index: 0.0,
            transform: Transform2D {
                translation,
                scale: Vec2::new(1.2, 0.8),
                rotation: 0.23,
            },
            style: Style {
                fill: Some(color),
                stroke: None,
                ..Style::default()
            },
            appearance: 1.0,
        }],
        presences: vec![true],
        reveals: vec![1.0],
        morphs: vec![0.0],
        render_geometries: vec![None],
        render_transforms: vec![None],
        family_animations: Vec::new(),
        family_animation_plan_indices: Vec::new(),
    };
    let mut renderer = GpuRenderer::new(device, queue, FORMAT);
    renderer.set_viewport(device, queue, size[0], size[1]);
    renderer.set_camera(
        queue,
        Camera2D::new(
            Vec2::ZERO,
            Vec2::new(size[0] as f32 / 16.0, size[1] as f32 / 16.0),
        )
        .unwrap(),
    );
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(&frame);
    renderer.upload(device, queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.encode(
        &mut encoder,
        &image.view,
        &prepared,
        wgpu::Color::TRANSPARENT,
    );
    queue.submit([encoder.finish()]);
    image
}

// Independent square-support two-dimensional oracle. No production coefficients,
// separable intermediate masks, GPU uniform bytes or shader code are reused.
pub(super) fn reference(mask: &[u8], size: [u32; 2], sigma: f64) -> Vec<f64> {
    let [width, height] = size.map(|x| x as i32);
    let radius = (3.0 * sigma).ceil() as i32;
    let weight = |dx: i32, dy: i32| {
        (-(f64::from(dx).powi(2) + f64::from(dy).powi(2)) / (2.0 * sigma.powi(2))).exp()
    };
    let mut normalization = 0.0;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            normalization += weight(dx, dy);
        }
    }
    let nonzero: Vec<_> = mask
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter_map(|(index, pixel)| {
            (pixel[3] != 0).then_some((
                index as i32 % width,
                index as i32 / width,
                f64::from(pixel[3]) / 255.0,
            ))
        })
        .collect();
    let mut result = vec![0.0; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let mut value = 0.0;
            for &(sx, sy, alpha) in &nonzero {
                let (dx, dy) = (x - sx, y - sy);
                if dx.abs() <= radius && dy.abs() <= radius {
                    value += alpha * weight(dx, dy);
                }
            }
            result[(y * width + x) as usize] = value / normalization;
        }
    }
    result
}

fn compare_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scope: &GlowScope,
    source: &[u8],
    mask: &[u8],
    size: [u32; 2],
    params: GlowParameters,
) {
    let sigma = params
        .definition
        .radius()
        .to_output_pixels(params.output_height, params.world_view_height)
        .unwrap();
    let length = size[0] as usize * size[1] as usize * 4;
    assert_eq!(source.len(), length);
    assert_eq!(mask.len(), length);
    assert!(
        mask.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0),
        "source capture must not make the Gaussian test vacuous"
    );
    let expected = reference(mask, size, sigma);
    assert!(
        expected.iter().any(|&value| value > 1e-5),
        "reference signal must exceed the comparison tolerance"
    );
    let resources = scope.resources.as_ref().unwrap();
    let packed = readback(device, queue, &resources.vertical.texture);
    let actual = readback(device, queue, scope.output().unwrap());
    let color = params.definition.color();
    let tint = [color.red, color.green, color.blue, color.alpha].map(f64::from);
    let mut max_mask_error = 0.0_f64;
    let mut max_byte_error = 0_i32;
    for (index, &blur) in expected.iter().enumerate() {
        let pixel = &packed[index * 4..index * 4 + 4];
        let bits = (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 8) | u32::from(pixel[2]);
        let measured = f64::from(bits) / 16_777_215.0;
        max_mask_error = max_mask_error.max((measured - blur).abs());
        let alpha = f64::from(source[index * 4 + 3]) / 255.0;
        let halo = (params.definition.intensity() * tint[3] * blur).min(1.0);
        for channel in 0..4 {
            let source_component = if alpha == 0.0 {
                0.0
            } else {
                f64::from(source[index * 4 + channel]) / 255.0
            };
            let halo_component = if channel == 3 {
                halo
            } else {
                tint[channel] * halo
            };
            let value = f64::from(params.scope_opacity)
                * (source_component + (1.0 - alpha) * halo_component);
            let byte = (255.0 * value.clamp(0.0, 1.0)).round() as i32;
            max_byte_error =
                max_byte_error.max((i32::from(actual[index * 4 + channel]) - byte).abs());
        }
    }
    eprintln!("glow sigma={sigma} size={size:?} source={:?} mask_error={max_mask_error:e} byte_error={max_byte_error}", params.definition.source());
    assert!(
        max_mask_error <= 1e-5,
        "Gaussian mask exceeds frozen tolerance: {max_mask_error}"
    );
    assert!(
        max_byte_error <= 2,
        "encoded source-over-halo exceeds frozen tolerance: {max_byte_error}"
    );
}

#[test]
#[ignore = "requires a native raster adapter; run explicitly, never qualifies via noop"]
fn native_gaussian_pixels_and_retained_updates() {
    let (device, queue) = device();
    let mut filter = GlowFilter::default();
    // Includes fractional transforms, nonuniform object scale, capture edges,
    // low alpha amplified by intensity, transparent silhouette and sigma limit.
    for (rectangle, alpha, mode, sigma, size, translation) in [
        (
            false,
            0.43,
            GlowSource::Painted,
            3.25,
            [65, 49],
            Vec2::new(0.173, -0.317),
        ),
        (
            true,
            0.43,
            GlowSource::Painted,
            3.25,
            [65, 49],
            Vec2::new(-1.9, 0.127),
        ),
        (
            false,
            0.0,
            GlowSource::Silhouette,
            5.0,
            [65, 49],
            Vec2::new(0.173, -0.317),
        ),
        (
            true,
            1.0 / 255.0,
            GlowSource::Painted,
            12.0,
            [65, 49],
            Vec2::new(0.0, 0.0),
        ),
        (
            true,
            0.5,
            GlowSource::Painted,
            64.0,
            [19, 15],
            Vec2::new(0.0, 0.0),
        ),
        (
            false,
            0.5,
            GlowSource::Painted,
            0.25,
            [19, 15],
            Vec2::new(0.0, 0.0),
        ),
    ] {
        let source = primitive_capture(
            &device,
            &queue,
            size,
            rectangle,
            Color::rgba(0.31, 0.57, 0.83, alpha),
            translation,
        );
        let silhouette =
            primitive_capture(&device, &queue, size, rectangle, Color::WHITE, translation);
        let source_pixels = readback(&device, &queue, &source.texture);
        let source_has_alpha = source_pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] != 0);
        assert_eq!(
            source_has_alpha,
            alpha > 0.0,
            "the ordinary renderer must supply the intended painted source"
        );
        let mask_pixels = if mode == GlowSource::Painted {
            source_pixels.clone()
        } else {
            readback(&device, &queue, &silhouette.texture)
        };
        let mut scope = GlowScope::default();
        let mut params = parameters(sigma);
        params.definition = GlowUpdate::default()
            .source(mode)
            .intensity(8.0)
            .color(Color::rgba(0.8, 0.2, 0.7, 0.65))
            .apply_to(params.definition)
            .unwrap();
        params.scope_opacity = 0.47;
        let capture = GlowCapture {
            source: &source.texture,
            silhouette: Some(&silhouette.texture),
            revision: 1,
        };
        filter
            .prepare(&device, &queue, &mut scope, capture, params)
            .unwrap();
        let first = encode_submit(&filter, &mut scope, &device, &queue);
        assert_eq!((first.blur_passes, first.composite_passes), (2, 1));
        compare_pixels(
            &device,
            &queue,
            &scope,
            &source_pixels,
            &mask_pixels,
            size,
            params,
        );
        let output = scope.output().unwrap().clone();
        params.definition = GlowUpdate::default()
            .intensity(0.4)
            .apply_to(params.definition)
            .unwrap();
        let edit = filter
            .prepare(&device, &queue, &mut scope, capture, params)
            .unwrap();
        assert_eq!(
            (
                edit.texture_allocations,
                edit.pipeline_compiles,
                edit.kernel_builds,
                edit.bytes_uploaded
            ),
            (0, 0, 0, 32)
        );
        let edit = encode_submit(&filter, &mut scope, &device, &queue);
        assert_eq!((edit.blur_passes, edit.composite_passes), (0, 1));
        assert_eq!(scope.output(), Some(&output));
        compare_pixels(
            &device,
            &queue,
            &scope,
            &source_pixels,
            &mask_pixels,
            size,
            params,
        );
    }
}
