use super::*;

fn parameters(sigma_pixels: f64) -> GaussianBlurParameters {
    GaussianBlurParameters {
        sigma_pixels,
        scope_opacity: 1.0,
        scratch_budget_bytes: 16 * 1024 * 1024,
    }
}

fn source(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("RGBA Gaussian test source"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

#[test]
fn kernel_layout_mass_and_subnormal_radius_are_finite() {
    assert_eq!(size_of::<BlurUniform>(), 1056);
    assert_eq!(std::mem::offset_of!(BlurUniform, control), 16);
    assert_eq!(std::mem::offset_of!(BlurUniform, weights), 32);
    for sigma in [f64::from_bits(1), f64::MIN_POSITIVE, 0.01, 0.8, 3.25, 64.0] {
        let kernel = BlurUniform::new(parameters(sigma), [33, 29], None);
        assert_eq!(kernel.radius, (3.0 * sigma).ceil() as u32);
        let weights: Vec<_> = kernel.weights.into_iter().flatten().collect();
        let mass = f64::from(weights[0])
            + 2.0
                * weights[1..=kernel.radius as usize]
                    .iter()
                    .map(|v| f64::from(*v))
                    .sum::<f64>();
        assert!((mass - 1.0).abs() < 2e-7);
        assert!(weights.iter().all(|v| v.is_finite() && *v >= 0.0));
        assert!(weights[kernel.radius as usize + 1..]
            .iter()
            .all(|v| *v == 0.0));
    }
}

#[test]
fn retained_resources_reuse_and_invalid_preparation_preserves_coherent_scope() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let source = source(&device, [33, 29]);
    let capture = GaussianBlurCapture {
        source: &source,
        revision: 1,
    };
    let mut filter = GaussianBlurFilter::default();
    let mut scope = GaussianBlurScope::default();
    let neutral = filter
        .prepare(&device, &queue, &mut scope, capture, parameters(0.0))
        .unwrap();
    assert_eq!(neutral, GaussianBlurStats::default());
    assert!(filter.programs.is_none());
    assert!(scope.output().is_none());
    let initial = filter
        .prepare(&device, &queue, &mut scope, capture, parameters(3.25))
        .unwrap();
    assert_eq!(initial.pipeline_compiles, 3);
    assert_eq!(initial.texture_allocations, 7);
    assert_eq!(initial.scratch_bytes, 33 * 29 * 28);
    let output = scope.output().unwrap().clone();
    let mut encoder = device.create_command_encoder(&Default::default());
    let encoded = filter.encode(&mut encoder, &mut scope);
    assert_eq!(encoded.blur_passes, 2);
    assert_eq!(encoded.output_passes, 1);
    queue.submit([encoder.finish()]);
    let same = filter
        .prepare(&device, &queue, &mut scope, capture, parameters(3.25))
        .unwrap();
    assert_eq!(same.bytes_uploaded, 0);
    assert_eq!(same.texture_allocations, 0);
    let mut encoder = device.create_command_encoder(&Default::default());
    assert_eq!(filter.encode(&mut encoder, &mut scope).blur_passes, 0);
    for sigma in [f64::NAN, f64::INFINITY, -0.1, 64.001] {
        assert_eq!(
            filter.prepare(&device, &queue, &mut scope, capture, parameters(sigma)),
            Err(GaussianBlurPrepareError::InvalidRadius)
        );
        assert_eq!(scope.output(), Some(&output));
    }
    for opacity in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        let mut invalid = parameters(3.25);
        invalid.scope_opacity = opacity;
        assert_eq!(
            filter.prepare(&device, &queue, &mut scope, capture, invalid),
            Err(GaussianBlurPrepareError::InvalidOpacity)
        );
        assert_eq!(scope.output(), Some(&output));
    }
    let mut invalid = parameters(3.25);
    invalid.scratch_budget_bytes = initial.scratch_bytes - 1;
    assert_eq!(
        filter.prepare(&device, &queue, &mut scope, capture, invalid),
        Err(GaussianBlurPrepareError::ScratchBudgetExceeded)
    );
    assert_eq!(scope.output(), Some(&output));
    let mut faded = parameters(3.25);
    faded.scope_opacity = 0.37;
    let stats = filter
        .prepare(&device, &queue, &mut scope, capture, faded)
        .unwrap();
    assert_eq!(stats.bytes_uploaded, 16);
    assert_eq!(stats.texture_allocations, 0);
    let encoded = filter.encode(&mut encoder, &mut scope);
    assert_eq!(encoded.blur_passes, 0);
    assert_eq!(encoded.output_passes, 1);
    scope.invalidate();
    let encoded = filter.encode(&mut encoder, &mut scope);
    assert_eq!(encoded.blur_passes, 2);
    assert_eq!(encoded.output_passes, 1);
    queue.submit([encoder.finish()]);
    let mut neutral = parameters(0.0);
    neutral.scratch_budget_bytes = 0;
    assert_eq!(
        filter
            .prepare(&device, &queue, &mut scope, capture, neutral)
            .unwrap()
            .scratch_bytes,
        0
    );
    assert!(scope.output().is_none());
    assert!(scope.resources.is_none());
}

fn upload(queue: &wgpu::Queue, texture: &wgpu::Texture, bytes: &[u8]) {
    queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width() * 4),
            rows_per_image: Some(texture.height()),
        },
        texture.size(),
    );
}

fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let row = texture.width() * 4;
    let stride = row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("RGBA Gaussian readback"),
        size: u64::from(stride * texture.height()),
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
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        tx.send(result).unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = buffer.get_mapped_range(..).unwrap();
    let output = bytes
        .chunks_exact(stride as usize)
        .flat_map(|row_bytes| row_bytes[..row as usize].iter().copied())
        .collect();
    drop(bytes);
    buffer.unmap();
    output
}

// Independent direct 2-D convolution, with complete square support normalized
// once. No production uniform, separable kernel or intermediate supplies it.
fn reference(source: &[u8], size: [u32; 2], sigma: f64) -> Vec<[f64; 4]> {
    let radius = (3.0 * sigma).ceil() as i32;
    let weight = |x: i32, y: i32| (-f64::from(x * x + y * y) / (2.0 * sigma * sigma)).exp();
    let mass = (-radius..=radius)
        .flat_map(|y| (-radius..=radius).map(move |x| weight(x, y)))
        .sum::<f64>();
    let mut output = vec![[0.0; 4]; (size[0] * size[1]) as usize];
    for (at, pixel) in source.as_chunks::<4>().0.iter().enumerate() {
        let (x, y) = (
            (at % size[0] as usize) as i32,
            (at / size[0] as usize) as i32,
        );
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let (xx, yy) = (x + dx, y + dy);
                if xx < 0 || yy < 0 || xx >= size[0] as i32 || yy >= size[1] as i32 {
                    continue;
                }
                let target = &mut output[(yy as u32 * size[0] + xx as u32) as usize];
                for channel in 0..4 {
                    target[channel] += f64::from(pixel[channel]) / 255.0 * weight(dx, dy) / mass;
                }
            }
        }
    }
    output
}

#[test]
#[ignore = "requires a real native adapter; missing adapter fails qualification"]
fn rgba_gaussian_pixels_match_independent_reference_and_retained_revisions() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("RGBA Gaussian adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let size = [33, 29];
    let source = source(&device, size);
    let mut filter = GaussianBlurFilter::default();
    let mut scope = GaussianBlurScope::default();
    let mut max_operator_error: f64 = 0.0;
    let mut max_byte_error = 0;
    let mut profiles = 0;
    for kind in 0..3 {
        let mut input = vec![0; (size[0] * size[1] * 4) as usize];
        for y in 0..size[1] {
            for x in 0..size[0] {
                let pixel = match kind {
                    0 if x == size[0] / 2 && y == size[1] / 2 => [83, 19, 131, 173],
                    1 => {
                        let a = ((x * 17 + y * 29) % 256) as u8;
                        [a / 2, a / 3, a / 5, a]
                    }
                    2 if x == 0 || y == 0 => [0, 1, 0, 2],
                    _ => [0; 4],
                };
                let at = ((y * size[0] + x) * 4) as usize;
                input[at..at + 4].copy_from_slice(&pixel);
            }
        }
        upload(&queue, &source, &input);
        for sigma in [0.8, 3.25, 6.0] {
            let stats = filter
                .prepare(
                    &device,
                    &queue,
                    &mut scope,
                    GaussianBlurCapture {
                        source: &source,
                        revision: kind + 1,
                    },
                    parameters(sigma),
                )
                .unwrap();
            assert_eq!(stats.texture_allocations, if profiles == 0 { 7 } else { 0 });
            let mut encoder = device.create_command_encoder(&Default::default());
            assert_eq!(filter.encode(&mut encoder, &mut scope).blur_passes, 2);
            queue.submit([encoder.finish()]);
            let expected = reference(&input, size, sigma);
            let resources = scope.resources.as_ref().unwrap();
            let packed = resources
                .vertical
                .each_ref()
                .map(|image| read(&device, &queue, &image.texture));
            for (at, value) in expected.iter().flatten().enumerate() {
                let bits = (u32::from(packed[0][at]) << 16)
                    | (u32::from(packed[1][at]) << 8)
                    | u32::from(packed[2][at]);
                let actual = f64::from(bits) / 16777215.0;
                max_operator_error = max_operator_error.max((actual - value).abs());
                assert!(
                    (actual - value).abs() <= 1e-5,
                    "kind={kind} sigma={sigma} at={at}: {actual} != {value}"
                );
            }
            for opacity in [1.0, 0.37] {
                let mut params = parameters(sigma);
                params.scope_opacity = opacity;
                let stats = filter
                    .prepare(
                        &device,
                        &queue,
                        &mut scope,
                        GaussianBlurCapture {
                            source: &source,
                            revision: kind + 1,
                        },
                        params,
                    )
                    .unwrap();
                assert_eq!(stats.texture_allocations, 0);
                assert_eq!(stats.pipeline_compiles, 0);
                let mut encoder = device.create_command_encoder(&Default::default());
                assert_eq!(filter.encode(&mut encoder, &mut scope).blur_passes, 0);
                queue.submit([encoder.finish()]);
                let pixels = read(&device, &queue, scope.output().unwrap());
                for (at, value) in expected.iter().flatten().enumerate() {
                    let byte = (value * f64::from(opacity) * 255.0).round() as u8;
                    let difference = pixels[at].abs_diff(byte);
                    max_byte_error = max_byte_error.max(difference);
                    assert!(
                        difference <= 2,
                        "kind={kind} sigma={sigma} opacity={opacity} at={at}"
                    );
                }
            }
            profiles += 1;
        }
    }
    // At the maximum portable radius, this one-pixel crop has a small but
    // nonzero high-precision result, so a zero/missing kernel cannot pass.
    let maximum_source = self::source(&device, [1, 1]);
    let input = [83, 19, 131, 173];
    upload(&queue, &maximum_source, &input);
    let stats = filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            GaussianBlurCapture {
                source: &maximum_source,
                revision: 1,
            },
            parameters(64.0),
        )
        .unwrap();
    assert_eq!(stats.texture_allocations, 7);
    assert_eq!(stats.scratch_bytes, 28);
    let mut encoder = device.create_command_encoder(&Default::default());
    filter.encode(&mut encoder, &mut scope);
    queue.submit([encoder.finish()]);
    let expected = reference(&input, [1, 1], 64.0);
    let packed = scope
        .resources
        .as_ref()
        .unwrap()
        .vertical
        .each_ref()
        .map(|image| read(&device, &queue, &image.texture));
    for channel in 0..4 {
        let bits = (u32::from(packed[0][channel]) << 16)
            | (u32::from(packed[1][channel]) << 8)
            | u32::from(packed[2][channel]);
        let actual = f64::from(bits) / 16777215.0;
        max_operator_error = max_operator_error.max((actual - expected[0][channel]).abs());
        assert!(expected[0][channel] > 1e-6);
        assert!(
            (actual - expected[0][channel]).abs() <= 1e-6,
            "maximum-radius packed channel {channel}"
        );
    }
    profiles += 1;
    eprintln!("RGBA Gaussian: {profiles} profiles / max operator error={max_operator_error} / max RGBA byte error={max_byte_error}");
}
