//! Execution capture qualification for mixed and nested composed scopes.
//! Public group authoring, animation and worker transport have separate gates.
use super::*;
use crate::{text::TextDeviceMetrics, Camera2D, RetainedFramePreparer};
use noon_compile::{CompiledObject, CompiledResources, CompiledScene};
use noon_core::{
    Blur, Color, EffectUpdate, GeometryRef, ObjectId, Pixels, RasterImageResourceArena,
    RasterImageSampling, SemanticImageContent, Style, Transform2D,
};
use noon_runtime::SceneInstance;

mod cap_bounds;
mod composed_publication;
mod glyph_opacity;

const SIZE: [u32; 2] = [80, 60];
const IMAGE_SIZE: [usize; 2] = [16, 12];

pub(super) fn fixture() -> (SceneInstance, [Vec<u8>; 2]) {
    let pixels = std::array::from_fn(|image| {
        (0..IMAGE_SIZE[1])
            .flat_map(|y| {
                (0..IMAGE_SIZE[0]).flat_map(move |x| {
                    if image == 0 {
                        [192, 32 + 4 * x as u8, 16, 128 + ((x + y) % 2) as u8 * 96]
                    } else {
                        [
                            16,
                            192,
                            64 + 4 * y as u8,
                            if (x + 2 * y) % 5 == 0 { 0 } else { 192 },
                        ]
                    }
                })
            })
            .collect::<Vec<_>>()
    });
    let mut arena = RasterImageResourceArena::new();
    let mut resources = CompiledResources::default();
    let images = pixels
        .iter()
        .enumerate()
        .map(|(image, pixels)| {
            let handle = arena
                .intern_rgba8(IMAGE_SIZE[0] as u32, IMAGE_SIZE[1] as u32, pixels.clone())
                .unwrap();
            let content = resources
                .capture_image_from_arena(
                    &arena,
                    SemanticImageContent::with_sampling(handle, RasterImageSampling::Nearest),
                )
                .unwrap();
            CompiledObject::new(
                ObjectId::new(if image == 0 { 1 } else { 3 }),
                content,
                Transform2D {
                    translation: Vec2::new(if image == 0 { -0.5 } else { 0.5 }, 0.0),
                    scale: Vec2::new(0.1, 0.1),
                    ..Transform2D::IDENTITY
                },
                Style {
                    fill: Some(Color::rgba(
                        1.0,
                        1.0,
                        1.0,
                        if image == 0 { 0.5 } else { 1.0 },
                    )),
                    stroke: None,
                    stroke_width: 0.0,
                    opacity: if image == 0 { 0.25 } else { 0.75 },
                    ..Style::default()
                },
            )
        })
        .collect::<Vec<_>>();
    let middle = CompiledObject::new(
        ObjectId::new(2),
        GeometryRef::rectangle(1.4, 0.8),
        Transform2D::IDENTITY,
        Style {
            fill: Some(Color::rgba(0.0, 0.0, 1.0, 0.5)),
            stroke: None,
            opacity: 0.75,
            ..Style::default()
        },
    );
    let mut compiled =
        CompiledScene::compile_objects(vec![images[0].clone(), middle, images[1].clone()], &[])
            .unwrap();
    compiled.merge_prepared_resources(resources);
    (SceneInstance::new(compiled), pixels)
}

fn target(device: &wgpu::Device, samples: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("composed source qualification"),
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | if samples == 1 {
                wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING
            } else {
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    })
}

pub(super) fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let stride = (SIZE[0] * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(stride * SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(SIZE[1]),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        tx.send(result).unwrap()
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mapped = buffer.get_mapped_range(..).unwrap();
    let bytes = mapped
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..SIZE[0] as usize * 4].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    bytes
}

pub(super) fn over(destination: &mut [u8], source: [f64; 4]) {
    for channel in 0..4 {
        destination[channel] = (255.0 * source[channel]
            + f64::from(destination[channel]) * (1.0 - source[3]))
            .round()
            .clamp(0.0, 255.0) as u8;
    }
}

// Literal integer pixel placement and straight-byte source-over, independent of
// packed uniforms, source draw lists, GPU intermediates and shader helpers.
pub(super) fn reference_layers(images: &[Vec<u8>; 2], silhouette: bool) -> [Vec<u8>; 3] {
    let mut layers = std::array::from_fn(|_| vec![0; SIZE[0] as usize * SIZE[1] as usize * 4]);
    for image in 0..2 {
        let left = if image == 0 { 27 } else { 37 };
        let opacity = if image == 0 {
            if silhouette {
                0.25
            } else {
                0.125
            }
        } else {
            0.75
        };
        for y in 0..IMAGE_SIZE[1] {
            for x in 0..IMAGE_SIZE[0] {
                let pixel = &images[image][(y * IMAGE_SIZE[0] + x) * 4..][..4];
                let alpha = f64::from(pixel[3]) / 255.0 * opacity;
                let out = ((y + 24) * SIZE[0] as usize + x + left) * 4;
                over(
                    &mut layers[image * 2][out..out + 4],
                    [
                        // Bitmap silhouette coverage is intrinsic texel alpha.
                        // Its capture keeps intrinsic RGB; mask consumers read A.
                        f64::from(pixel[0]) / 255.0 * alpha,
                        f64::from(pixel[1]) / 255.0 * alpha,
                        f64::from(pixel[2]) / 255.0 * alpha,
                        alpha,
                    ],
                );
            }
        }
    }
    for y in 26..34 {
        for x in 33..47 {
            let out = (y * SIZE[0] as usize + x) * 4;
            over(
                &mut layers[1][out..out + 4],
                if silhouette {
                    [0.75; 4]
                } else {
                    [0.0, 0.0, 0.375, 0.375]
                },
            );
        }
    }
    layers
}

pub(super) fn compose(layers: &[Vec<u8>; 3]) -> Vec<u8> {
    let mut output = vec![0; layers[0].len()];
    for layer in layers {
        for (out, pixel) in output
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(layer.as_chunks::<4>().0)
        {
            over(out, std::array::from_fn(|i| f64::from(pixel[i]) / 255.0));
        }
    }
    output
}

fn blur(source: &[u8]) -> Vec<u8> {
    blur_with_opacity(source, 0.5)
}

pub(super) fn blur_with_opacity(source: &[u8], opacity: f64) -> Vec<u8> {
    let mut output = vec![0; source.len()];
    let mut taps = Vec::new();
    let mut mass = 0.0;
    for dy in -6i32..=6 {
        for dx in -6i32..=6 {
            let weight = (-f64::from(dx * dx + dy * dy) / 8.0).exp();
            mass += weight;
            taps.push((dx, dy, weight));
        }
    }
    for y in 0..SIZE[1] as i32 {
        for x in 0..SIZE[0] as i32 {
            let mut value = [0.0; 4];
            for &(dx, dy, weight) in &taps {
                let xx = x + dx;
                let yy = y + dy;
                if xx >= 0 && yy >= 0 && xx < SIZE[0] as i32 && yy < SIZE[1] as i32 {
                    for c in 0..4 {
                        value[c] += f64::from(
                            source[(yy as usize * SIZE[0] as usize + xx as usize) * 4 + c],
                        ) * weight
                            / mass;
                    }
                }
            }
            for c in 0..4 {
                output[(y as usize * SIZE[0] as usize + x as usize) * 4 + c] =
                    (value[c] * opacity).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    output
}

pub(super) fn maximum_error(actual: &[u8], expected: &[u8]) -> u8 {
    assert_eq!(actual.len(), expected.len());
    actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap()
}

fn qualify(device: &wgpu::Device, queue: &wgpu::Queue, pixels: bool) {
    let (mut runtime, images) = fixture();
    let publication = runtime.take_renderer_publication();
    let mut preparer = RetainedFramePreparer::new();
    let metrics = TextDeviceMetrics::uniform(10.0)
        .unwrap()
        .with_world_origin_pixels(Vec2::new(40.0, 30.0))
        .unwrap();
    let prepared = preparer
        .prepare_planned_publication_visible(device, &publication, &[0, 1, 2], metrics)
        .unwrap();
    let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(device, queue, SIZE[0], SIZE[1]);
    renderer.set_camera(
        queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut text = renderer.create_retained_text_state(device, queue);
    renderer.upload_retained(device, queue, &prepared, &mut text);
    let alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
    let members = [
        (ObjectId::new(1), 1.0),
        (ObjectId::new(2), 1.0),
        (ObjectId::new(3), 1.0),
    ];
    for invalid in [
        vec![],
        vec![members[0], members[0]],
        vec![members[1], members[0]],
        vec![members[0], members[2]],
    ] {
        assert!(
            PreparedSource::prepare(&prepared, &invalid, false, alignment).is_none(),
            "aliased, reversed, empty and interleaved intervals must fail"
        );
    }
    let mut source = PreparedSource::prepare(&prepared, &members, false, alignment).unwrap();
    assert_eq!(source.draws.len(), 3);
    source.set_capture_region([0, 0], SIZE).unwrap();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: source.bytes.len().next_power_of_two() as u64,
        usage: wgpu::BufferUsages::UNIFORM
            | wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, &source.bytes);
    let mut bindings = Vec::new();
    source
        .prepare_image_bindings(&renderer, device, &buffer, &mut bindings)
        .unwrap();
    assert_eq!(bindings.len(), 2);
    assert_ne!(bindings[0].resource, bindings[1].resource);
    assert!(bindings[1].offset > bindings[0].offset);
    for binding in &bindings {
        assert_eq!(binding.offset % alignment as u64, 0);
    }
    for silhouette in [false, true] {
        let mut source =
            PreparedSource::prepare(&prepared, &members, silhouette, alignment).unwrap();
        source.set_capture_region([17, -13], SIZE).unwrap();
        for draw in &source.draws {
            if let SourceDraw::Image {
                object_index,
                bytes,
                ..
            } = draw
            {
                let image = prepared.capture_image(*object_index).unwrap();
                assert_eq!(
                    bytemuck::pod_read_unaligned::<ImageUniform>(
                        &source.bytes[bytes.start as usize..bytes.end as usize]
                    ),
                    image
                        .uniform
                        .captured(image.paint_opacity, image.object_opacity, silhouette)
                        .with_capture_origin([17, -13])
                );
            }
        }
    }
    if !pixels {
        return;
    }
    for silhouette in [false, true] {
        let source = PreparedSource::prepare(&prepared, &members, silhouette, alignment).unwrap();
        queue.write_buffer(&buffer, 0, &source.bytes);
        let output = target(device, 1);
        let multisampled = target(device, PATH_SAMPLE_COUNT);
        let text_camera = text
            .glyphs
            .capture_camera_binding(device, &renderer.camera_buffer);
        let mut encoder = device.create_command_encoder(&Default::default());
        source
            .encode(
                &renderer,
                &text,
                &prepared.geometry,
                &mut encoder,
                &output.create_view(&Default::default()),
                &multisampled.create_view(&Default::default()),
                &renderer.camera_bind_group,
                &text_camera,
                &buffer,
                &bindings,
                None,
                SIZE,
            )
            .unwrap();
        let mut filter = super::super::EffectStackFilter::default();
        let mut scope = super::super::EffectStackScope::default();
        filter
            .prepare_encode(
                device,
                queue,
                &mut encoder,
                &mut scope,
                super::super::GlowCapture {
                    source: &output,
                    silhouette: None,
                    revision: 1,
                },
                &[Blur::new(EffectUpdate::default().radius(Pixels(2.0)))
                    .unwrap()
                    .into()],
                super::super::EffectStackParameters {
                    shader: None,
                    working_format: wgpu::TextureFormat::Rgba8Unorm,
                    output_height: SIZE[1],
                    world_view_height: 6.0,
                    scope_opacity: 0.5,
                    scratch_budget_bytes: 1_000_000,
                    source_bounds: None,
                },
            )
            .unwrap();
        queue.submit([encoder.finish()]);
        let layers = reference_layers(&images, silhouette);
        let expected = compose(&layers);
        let source_error = maximum_error(&read(device, queue, &output), &expected);
        assert!(
            source_error <= 2,
            "mixed composed source max byte error {source_error}"
        );
        let expected = blur(&expected);
        let actual = read(device, queue, scope.output().unwrap());
        let filtered_error = maximum_error(&actual, &expected);
        assert!(
            filtered_error <= 2,
            "filter after composition max byte error {filtered_error}"
        );
        let per_child = compose(&layers.map(|layer| blur(&layer)));
        let witness = maximum_error(&per_child, &expected);
        assert!(
            witness > 2,
            "per-child filtering is an ineffective control: {witness}"
        );
        eprintln!("mixed composed capture silhouette={silhouette}: two different images and overlapping geometry; source max {source_error}, filtered max {filtered_error}, incorrect per-child witness {witness}");
    }
}

#[test]
fn transparent_image_paint_keeps_child_opacity_live_for_silhouette_capture() {
    let (mut runtime, _) = fixture();
    let publication = runtime.take_renderer_publication();
    let mut frame = publication.frame().clone();
    frame.objects[0].style.fill = Some(Color::rgba(1.0, 1.0, 1.0, 0.0));
    let mut images = super::super::raster_image_prepare::RasterImageFramePreparer::default();
    let prepared = images
        .stage(
            &frame,
            &noon_runtime::FrameChanges::all(),
            Some(publication.raster_image_resources()),
            publication.geometry_resources(),
            8192,
        )
        .unwrap();
    images.commit(prepared);
    let original = images.objects[&0].clone();
    let generation = images.generation;
    assert_eq!(original.object_opacity, 0.25);
    frame.objects[0].style.opacity = 0.5;
    let prepared = images
        .stage(
            &frame,
            &noon_runtime::FrameChanges::objects(vec![0]),
            Some(publication.raster_image_resources()),
            publication.geometry_resources(),
            8192,
        )
        .unwrap();
    images.commit(prepared);
    let next = &images.objects[&0];
    assert_eq!(
        original.uniform, next.uniform,
        "transparent ordinary image uniforms remain equal"
    );
    assert_eq!(next.object_opacity, 0.5);
    assert!(images.generation > generation);
    assert_ne!(
        original
            .uniform
            .captured(original.paint_opacity, original.object_opacity, true),
        next.uniform
            .captured(next.paint_opacity, next.object_opacity, true)
    );
}

#[test]
#[cfg(feature = "ci-noop")]
fn composed_capture_owns_distinct_aligned_image_bindings_and_rejects_invalid_intervals() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    qualify(&device, &queue, false);
}

#[test]
#[ignore = "requires a real native raster adapter; does not qualify semantic group authoring"]
fn mixed_composed_source_and_filter_match_independent_layer_composition() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("composed capture raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    qualify(&device, &queue, true);
}
