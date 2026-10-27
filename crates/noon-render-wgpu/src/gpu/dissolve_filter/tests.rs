use super::*;

const SIZE: [u32; 2] = [24, 20];
fn parameters(progress: f64) -> DissolveParameters {
    DissolveParameters {
        definition: noon_core::NoiseDissolve::new(7, 0.08)
            .unwrap()
            .with_progress(progress)
            .unwrap(),
        source_bounds: EffectPixelBounds {
            min: [0.0, 0.0],
            max: SIZE.map(f64::from),
        },
        scope_opacity: 1.0,
        scratch_budget_bytes: u64::from(SIZE[0] * SIZE[1] * 4),
    }
}
fn source(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("dissolve reference source"),
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
fn prepare(
    filter: &mut DissolveFilter,
    scope: &mut DissolveScope,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    p: DissolveParameters,
) -> Result<EffectRasterStats, EffectPrepareError> {
    let mut encoder = device.create_command_encoder(&Default::default());
    let result = filter.prepare_encode(
        device,
        queue,
        &mut encoder,
        scope,
        GaussianBlurCapture {
            source: texture,
            revision: 1,
        },
        p,
    );
    if result.is_ok() {
        queue.submit([encoder.finish()]);
    }
    result
}

#[test]
fn mask_preflight_is_atomic_and_clean_preparation_reuses_resources() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let source = source(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut filter = DissolveFilter::default();
    let mut scope = DissolveScope::default();
    let mut rejected = parameters(0.5);
    rejected.scratch_budget_bytes = 0;
    assert_eq!(
        prepare(&mut filter, &mut scope, &device, &queue, &source, rejected),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
    assert!(filter.program.is_none());
    assert!(scope.output().is_none());
    let initial = prepare(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        &source,
        parameters(0.5),
    )
    .unwrap();
    assert_eq!(
        (
            initial.pipeline_compiles,
            initial.texture_allocations,
            initial.buffer_allocations,
            initial.composite_passes
        ),
        (1, 1, 1, 1)
    );
    assert_eq!(scope.scratch_bytes(), parameters(0.5).scratch_budget_bytes);
    let output = scope.output().unwrap().clone();
    let uniform = scope.uniform;
    let mut invalid = Vec::new();
    let mut p = parameters(0.5);
    p.scratch_budget_bytes -= 1;
    invalid.push((p, EffectPrepareError::ScratchBudgetExceeded));
    let mut p = parameters(0.5);
    p.scope_opacity = f32::NAN;
    invalid.push((p, EffectPrepareError::InvalidScopeOpacity));
    let mut p = parameters(0.5);
    p.source_bounds.max[0] = 0.0;
    invalid.push((p, EffectPrepareError::InvalidSourceBounds));
    let mut p = parameters(0.5);
    p.source_bounds.min[0] = f64::NAN;
    invalid.push((p, EffectPrepareError::InvalidSourceBounds));
    let mut p = parameters(0.5);
    p.source_bounds.max[0] = 1e-30;
    invalid.push((p, EffectPrepareError::CaptureCoordinatesOutOfRange));
    for (p, error) in invalid {
        assert_eq!(
            prepare(&mut filter, &mut scope, &device, &queue, &source, p),
            Err(error)
        );
        assert_eq!(scope.output(), Some(&output));
        assert_eq!(scope.uniform, uniform);
    }
    assert_eq!(
        prepare(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            &output,
            parameters(0.5)
        ),
        Err(EffectPrepareError::UnsupportedCapture)
    );
    let same = prepare(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        &source,
        parameters(0.5),
    )
    .unwrap();
    assert_eq!(
        (
            same.pipeline_compiles,
            same.texture_allocations,
            same.bytes_uploaded,
            same.composite_passes
        ),
        (0, 0, 0, 0)
    );
    for progress in [0.25, 0.75, 1.0, 0.0] {
        let work = prepare(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            &source,
            parameters(progress),
        )
        .unwrap();
        assert_eq!(
            (
                work.pipeline_compiles,
                work.texture_allocations,
                work.composite_passes
            ),
            (0, 0, 1)
        );
    }
    scope.invalidate();
    assert_eq!(
        prepare(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            &source,
            parameters(0.0)
        )
        .unwrap()
        .composite_passes,
        1
    );
    scope.clear();
    assert_eq!(scope.scratch_bytes(), 0);
    assert!(scope.output().is_none());
}

// Specification-derived scalar integer hash and barycentric field. Fixed vectors
// were calculated separately with Python arbitrary-precision integer arithmetic.
fn hash(seed: u32, x: i32, y: i32) -> u32 {
    let mut value =
        (x as u32).wrapping_mul(0x9e3779b9) ^ (y as u32).wrapping_mul(0x85ebca6b) ^ seed;
    value = (value ^ (value >> 16)).wrapping_mul(0x7feb352d);
    value = (value ^ (value >> 15)).wrapping_mul(0x846ca68b);
    value ^ (value >> 16)
}
fn noise(seed: u32, uv: [f64; 2]) -> f64 {
    let p = uv.map(|v| v * 16.0);
    let base = p.map(|v| v.floor() as i32);
    let t = std::array::from_fn::<_, 2, _>(|i| {
        let t = p[i] - p[i].floor();
        t * t * (3.0 - 2.0 * t)
    });
    let mut sum = 0.0;
    for y in 0..2 {
        for x in 0..2 {
            let weight =
                if x == 0 { 1.0 - t[0] } else { t[0] } * if y == 0 { 1.0 - t[1] } else { t[1] };
            sum += f64::from(hash(seed, base[0] + x, base[1] + y) >> 8) / 16777216.0 * weight;
        }
    }
    sum
}
pub(in crate::gpu) fn reference(input: &[u8], p: DissolveParameters, size: [u32; 2]) -> Vec<u8> {
    let progress = f64::from(p.definition.progress() as f32);
    let softness = f64::from(p.definition.softness() as f32);
    let mut result = Vec::with_capacity(input.len());
    for y in 0..size[1] {
        for x in 0..size[0] {
            let uv = std::array::from_fn::<_, 2, _>(|a| {
                ((f64::from([x, y][a]) + 0.5) - p.source_bounds.min[a])
                    / (p.source_bounds.max[a] - p.source_bounds.min[a])
            });
            let coverage = if progress == 0.0 {
                1.0
            } else if progress == 1.0 {
                0.0
            } else {
                let n = noise(p.definition.seed(), uv);
                if softness == 0.0 {
                    if n >= progress {
                        1.0
                    } else {
                        0.0
                    }
                } else {
                    let t = ((progress - n) / softness + 0.5).clamp(0.0, 1.0);
                    1.0 - (3.0 * t * t - 2.0 * t * t * t)
                }
            };
            let at = ((y * size[0] + x) * 4) as usize;
            result.extend(
                input[at..at + 4]
                    .iter()
                    .map(|&v| (f64::from(v) * coverage * f64::from(p.scope_opacity)).round() as u8),
            );
        }
    }
    result
}
#[test]
fn seeded_field_vectors_cover_zero_full_unsigned_and_negative_coordinates() {
    for (seed, x, y, expected) in [
        (0, 0, 0, 0),
        (7, 1, 2, 0xb02adea4),
        (u32::MAX, -1, 3, 0x56d60568),
        (1, -4, -9, 0xf911c8b1),
        (0x80000000, 15, 16, 0xb72e9d9f),
    ] {
        assert_eq!(hash(seed, x, y), expected);
    }
}
fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let stride = (SIZE[0] * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dissolve reference readback"),
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
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let mut result = Vec::new();
    for row in mapped.chunks(stride as usize) {
        result.extend_from_slice(&row[..(SIZE[0] * 4) as usize]);
    }
    result
}

#[test]
#[ignore = "requires a real native raster adapter; independent dissolve field and alpha oracle"]
fn dissolve_pixels_endpoints_seed_coordinates_and_warm_resources_match_reference() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("dissolve raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("dissolve adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut input = Vec::new();
    for y in 0..SIZE[1] {
        for x in 0..SIZE[0] {
            let alpha = (x * 19 + y * 29) % 256;
            input.extend(
                [
                    (x * 7 % 256) * alpha / 255,
                    (y * 11 % 256) * alpha / 255,
                    ((x + y) * 13 % 256) * alpha / 255,
                    alpha,
                ]
                .map(|v| v as u8),
            );
        }
    }
    let mut filter = DissolveFilter::default();
    let mut maximum = 0;
    let mut count = 0;
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
    ] {
        let texture = source(&device, format);
        let mut upload = input.clone();
        if format == wgpu::TextureFormat::Bgra8Unorm {
            for p in upload.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
            }
        }
        queue.write_texture(
            texture.as_image_copy(),
            &upload,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE[0] * 4),
                rows_per_image: None,
            },
            texture.size(),
        );
        let mut scope = DissolveScope::default();
        for seed in [0, 1, 7, u32::MAX] {
            for softness in [0.0, 0.08, 0.5] {
                for opacity in [1.0, 0.375] {
                    for bounds in [
                        EffectPixelBounds {
                            min: [0.0, 0.0],
                            max: SIZE.map(f64::from),
                        },
                        EffectPixelBounds {
                            min: [-7.5, 5.25],
                            max: [20.5, 31.25],
                        },
                        EffectPixelBounds {
                            min: [3.5, 2.25],
                            max: [20.5, 15.75],
                        },
                    ] {
                        for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
                            let mut p = parameters(progress);
                            p.definition = noon_core::NoiseDissolve::new(seed, softness)
                                .unwrap()
                                .with_progress(progress)
                                .unwrap();
                            p.scope_opacity = opacity;
                            p.source_bounds = bounds;
                            let first_in_scope = scope.output().is_none();
                            let work =
                                prepare(&mut filter, &mut scope, &device, &queue, &texture, p)
                                    .unwrap();
                            assert_eq!(work.texture_allocations, usize::from(first_in_scope));
                            assert_eq!(work.buffer_allocations, usize::from(first_in_scope));
                            if count > 0 {
                                assert_eq!(work.pipeline_compiles, 0);
                            }
                            let actual = read(&device, &queue, scope.output().unwrap());
                            let expected = reference(&input, p, SIZE);
                            let error = actual
                                .iter()
                                .zip(&expected)
                                .map(|(a, b)| a.abs_diff(*b))
                                .max()
                                .unwrap();
                            maximum = maximum.max(error);
                            assert!(error<=1,"dissolve seed={seed} softness={softness} p={progress} opacity={opacity} bounds={bounds:?}: max byte error {error}");
                            if progress == 0.0 && opacity == 1.0 {
                                assert_eq!(actual, input);
                            }
                            if progress == 1.0 {
                                assert!(actual.iter().all(|&v| v == 0));
                            }
                            assert_eq!(scope.scratch_bytes(), 1920);
                            count += 1;
                        }
                    }
                }
            }
        }
        prepare(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            &texture,
            parameters(0.5),
        )
        .unwrap();
        let baseline = read(&device, &queue, scope.output().unwrap());
        for _ in 0..32 {
            let same = prepare(
                &mut filter,
                &mut scope,
                &device,
                &queue,
                &texture,
                parameters(0.5),
            )
            .unwrap();
            assert_eq!(
                (
                    same.texture_allocations,
                    same.bytes_uploaded,
                    same.composite_passes
                ),
                (0, 0, 0)
            );
            assert_eq!(read(&device, &queue, scope.output().unwrap()), baseline);
        }
        let replacement = source(&device, format);
        queue.write_texture(
            replacement.as_image_copy(),
            &upload,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE[0] * 4),
                rows_per_image: None,
            },
            replacement.size(),
        );
        let rebound = prepare(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            &replacement,
            parameters(0.5),
        )
        .unwrap();
        assert_eq!(
            (
                rebound.texture_allocations,
                rebound.bind_group_creations,
                rebound.composite_passes
            ),
            (0, 1, 1)
        );
        assert_eq!(read(&device, &queue, scope.output().unwrap()), baseline);
        queue.write_texture(
            replacement.as_image_copy(),
            &vec![0; input.len()],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE[0] * 4),
                rows_per_image: None,
            },
            replacement.size(),
        );
        let mut encoder = device.create_command_encoder(&Default::default());
        let refreshed = filter
            .prepare_encode(
                &device,
                &queue,
                &mut encoder,
                &mut scope,
                GaussianBlurCapture {
                    source: &replacement,
                    revision: 2,
                },
                parameters(0.5),
            )
            .unwrap();
        assert_eq!(
            (
                refreshed.texture_allocations,
                refreshed.bytes_uploaded,
                refreshed.composite_passes
            ),
            (0, 0, 1)
        );
        queue.submit([encoder.finish()]);
        assert!(read(&device, &queue, scope.output().unwrap())
            .iter()
            .all(|&v| v == 0));
        prepare(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            &texture,
            parameters(0.5),
        )
        .unwrap();
        let mut abandoned = device.create_command_encoder(&Default::default());
        filter
            .prepare_encode(
                &device,
                &queue,
                &mut abandoned,
                &mut scope,
                GaussianBlurCapture {
                    source: &texture,
                    revision: 1,
                },
                parameters(0.75),
            )
            .unwrap();
        drop(abandoned);
        assert_eq!(read(&device, &queue, scope.output().unwrap()), baseline);
        scope.invalidate();
        let retried = prepare(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            &texture,
            parameters(0.75),
        )
        .unwrap();
        assert_eq!(
            (
                retried.texture_allocations,
                retried.bytes_uploaded,
                retried.composite_passes
            ),
            (0, 0, 1)
        );
        let expected = reference(&input, parameters(0.75), SIZE);
        assert!(read(&device, &queue, scope.output().unwrap())
            .iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(b) <= 1));
    }
    eprintln!("DISSOLVE_OPERATOR_PASS cases={count} channels={} max_error={maximum} source_endpoints_exact=true empty_endpoints_exact=true warmed_bytes=1920 source_revisions=true rebind=true abandoned_retry=true",count*input.len());
}
