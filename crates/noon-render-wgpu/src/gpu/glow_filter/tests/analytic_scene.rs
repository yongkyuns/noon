//! Runtime-frame -> padded capture -> ordinary painter-stream pixel qualification.
//! The independent reference uses a much larger plain capture with a different
//! camera extent, then crops only AFTER the full two-dimensional convolution.
//! This is not yet a public authored Scene/effect-channel test.
use super::pixels::{device, readback, reference};
use crate::{AnalyticGlowRequest, Camera2D, FramePreparer, GpuRenderer};
use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Color, GeometryRef, Glow, GlowSource, GlowUpdate, ObjectId, Pixels, Style, Transform2D, Vec2,
};
use noon_runtime::{FrameState, SceneInstance};

const VIEW: [u32; 2] = [80, 60];
const PAD: u32 = 24;
const SIGMA: f64 = 3.25;

fn target(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Noon glow painter qualification output"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn render_plain(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: &FrameState,
    size: [u32; 2],
) -> Vec<u8> {
    let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(device, queue, size[0], size[1]);
    renderer.set_camera(
        queue,
        Camera2D::new(
            Vec2::ZERO,
            Vec2::new(size[0] as f32 / 10.0, size[1] as f32 / 10.0),
        )
        .unwrap(),
    );
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(frame);
    renderer.upload(device, queue, &prepared);
    let texture = target(device, size);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.encode(
        &mut encoder,
        &texture.create_view(&Default::default()),
        &prepared,
        wgpu::Color::TRANSPARENT,
    );
    queue.submit([encoder.finish()]);
    readback(device, queue, &texture)
}

fn isolated(frame: &FrameState, index: usize) -> FrameState {
    let mut result = frame.clone();
    result.presences.fill(false);
    result.presences[index] = true;
    result
}

fn over(dst: &mut [u8], src: &[u8]) {
    for (dst, src) in dst
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(src.as_chunks::<4>().0)
    {
        let remaining = 1.0 - f64::from(src[3]) / 255.0;
        for channel in 0..4 {
            dst[channel] = (f64::from(src[channel]) + remaining * f64::from(dst[channel]))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}

fn expected(device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameState, glow: Glow) -> Vec<u8> {
    let size = [VIEW[0] + 2 * PAD, VIEW[1] + 2 * PAD];
    let mut source_frame = isolated(frame, 1);
    let opacity = source_frame.objects[1].style.opacity;
    source_frame.objects[1].style.opacity = 1.0;
    let source = render_plain(device, queue, &source_frame, size);
    let mask = if glow.source() == GlowSource::Silhouette {
        source_frame.objects[1].style.fill = Some(Color::WHITE);
        render_plain(device, queue, &source_frame, size)
    } else {
        source.clone()
    };
    let blur = reference(&mask, size, SIGMA);
    assert!(blur.iter().copied().fold(0.0_f64, f64::max) > 1e-4);
    let tint = glow.color();
    let tint = [tint.red, tint.green, tint.blue].map(f64::from);
    let mut effect = vec![0_u8; source.len()];
    for ((out, src), blurred) in effect
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(source.as_chunks::<4>().0)
        .zip(blur)
    {
        let alpha = f64::from(src[3]) / 255.0;
        let halo = (glow.intensity() * f64::from(glow.color().alpha) * blurred).min(1.0);
        for channel in 0..4 {
            let component = if channel == 3 {
                halo
            } else {
                tint[channel] * halo
            };
            let value =
                f64::from(opacity) * (f64::from(src[channel]) / 255.0 + (1.0 - alpha) * component);
            out[channel] = (255.0 * value).round().clamp(0.0, 255.0) as u8;
        }
    }
    let mut canvas = render_plain(device, queue, &isolated(frame, 0), size);
    over(&mut canvas, &effect);
    let foreground = render_plain(device, queue, &isolated(frame, 2), size);
    over(&mut canvas, &foreground);
    let mut cropped = Vec::with_capacity((VIEW[0] * VIEW[1] * 4) as usize);
    for y in PAD..PAD + VIEW[1] {
        let start = ((y * size[0] + PAD) * 4) as usize;
        cropped.extend_from_slice(&canvas[start..start + VIEW[0] as usize * 4]);
    }
    cropped
}

fn frame(rectangle: bool, offscreen: bool, transparent: bool) -> FrameState {
    let geometry = if rectangle {
        GeometryRef::rectangle(1.1, 0.6)
    } else {
        GeometryRef::circle(0.65)
    };
    let positions = [
        Vec2::new(-1.0, 0.0),
        Vec2::new(if offscreen { -4.75 } else { 0.173 }, -0.217),
        Vec2::new(if offscreen { -3.8 } else { 0.55 }, 0.25),
    ];
    let objects = vec![
        CompiledObject::new(
            ObjectId::new(1),
            GeometryRef::circle(2.4),
            Transform2D {
                translation: positions[0],
                ..Transform2D::IDENTITY
            },
            Style {
                fill: Some(Color::rgba(0.14, 0.28, 0.55, 0.7)),
                stroke: None,
                ..Style::default()
            },
        ),
        CompiledObject::new(
            ObjectId::new(2),
            geometry,
            Transform2D {
                translation: positions[1],
                rotation: if offscreen { 0.0 } else { 0.37 },
                scale: if offscreen {
                    Vec2::new(1.0, 1.0)
                } else {
                    Vec2::new(-1.2, 0.8)
                },
            },
            Style {
                fill: Some(Color::rgba(
                    0.7,
                    0.3,
                    0.1,
                    if transparent { 0.0 } else { 0.43 },
                )),
                stroke: None,
                opacity: 0.47,
                ..Style::default()
            },
        ),
        CompiledObject::new(
            ObjectId::new(3),
            GeometryRef::circle(0.3),
            Transform2D {
                translation: positions[2],
                ..Transform2D::IDENTITY
            },
            Style {
                fill: Some(Color::rgba(0.1, 0.95, 0.4, 1.0)),
                stroke: None,
                ..Style::default()
            },
        ),
    ];
    SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap())
        .frame()
        .clone()
}

#[test]
#[ignore = "requires a raster adapter; explicit canonical painter/capture qualification"]
fn retained_painter_glow_pixels() {
    let (device, queue) = device();
    for (rectangle, offscreen, transparent) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (true, true, true),
        (false, false, true),
    ] {
        let frame = frame(rectangle, offscreen, transparent);
        let mode = if transparent {
            GlowSource::Silhouette
        } else {
            GlowSource::Painted
        };
        let glow = Glow::new(
            GlowUpdate::default()
                .radius(Pixels(SIGMA))
                .intensity(2.4)
                .color(Color::rgba(0.95, 0.35, 0.8, 0.8))
                .source(mode),
        )
        .unwrap();
        let request = AnalyticGlowRequest {
            object_index: 1,
            definition: glow,
            texture_budget_bytes: 1_000_000,
        };
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, VIEW[0], VIEW[1]);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        );
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);
        renderer.upload(&device, &queue, &prepared);
        let texture = target(&device, VIEW);
        let view = texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        let stats = renderer
            .prepare_analytic_glow(&device, &queue, &mut encoder, &prepared, request)
            .unwrap();
        assert_eq!(stats.source_passes, if transparent { 2 } else { 1 });
        renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::TRANSPARENT);
        queue.submit([encoder.finish()]);
        let actual = readback(&device, &queue, &texture);
        let expected = expected(&device, &queue, &frame, glow);
        let max = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
            .max()
            .unwrap();
        let plain = render_plain(&device, &queue, &frame, VIEW);
        let halo_signal = actual
            .iter()
            .zip(&plain)
            .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
            .max()
            .unwrap();
        eprintln!("painter rectangle={rectangle} offscreen={offscreen} transparent={transparent}: max_byte_error={max} halo_signal={halo_signal}");
        assert!(
            halo_signal > 2,
            "must actually show a visible halo, including offscreen sources"
        );
        assert!(
            max <= 2,
            "painter/capture mismatch, maximum RGBA byte error {max}"
        );
        // Neutral must restore exact ordinary output, not a copied/composited approximation.
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer
            .prepare_analytic_glow(
                &device,
                &queue,
                &mut encoder,
                &prepared,
                AnalyticGlowRequest {
                    definition: GlowUpdate::default().intensity(0.0).apply_to(glow).unwrap(),
                    ..request
                },
            )
            .unwrap();
        renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::TRANSPARENT);
        queue.submit([encoder.finish()]);
        assert_eq!(
            readback(&device, &queue, &texture),
            plain,
            "neutral must be bit-identical"
        );
        renderer.remove_analytic_glow(ObjectId::new(2));
        assert_eq!(renderer.analytic_glow_texture_bytes(), 0);
    }
}
