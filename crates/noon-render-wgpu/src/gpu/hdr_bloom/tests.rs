use super::*;
use half::f16;

fn noop_filter(device: &wgpu::Device) -> HdrBloomFilter {
    HdrBloomFilter {
        device: device.clone(),
        float_targets: true,
        owner: Arc::new(()),
        programs: None,
    }
}
fn parameters(sigma: f64) -> HdrBloomParameters {
    HdrBloomParameters {
        sigma_pixels: sigma,
        threshold: 1.0,
        intensity: 1.5,
        scratch_budget_bytes: 16 * 1024 * 1024,
    }
}
fn source(device: &wgpu::Device, size: [u32; 2], format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("HDR bloom test input"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
fn encode(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    filter: &HdrBloomFilter,
    scope: &mut HdrBloomScope,
) -> HdrBloomStats {
    let mut encoder = device.create_command_encoder(&Default::default());
    let stats = filter.encode(&mut encoder, scope);
    queue.submit([encoder.finish()]);
    stats
}
#[test]
fn finite_kernel_normalization_and_layout() {
    assert_eq!(size_of::<BloomUniform>(), 1056);
    assert_eq!(std::mem::offset_of!(BloomUniform, control), 16);
    assert_eq!(std::mem::offset_of!(BloomUniform, weights), 32);
    for sigma in [
        0.0,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        0.01,
        0.65,
        2.25,
        64.0,
    ] {
        let uniform = BloomUniform::new(parameters(sigma), [17, 13], None);
        let weights: Vec<_> = uniform.weights.into_iter().flatten().collect();
        let radius = uniform.radius as usize;
        let mass = f64::from(weights[0])
            + 2.0
                * weights[1..=radius]
                    .iter()
                    .copied()
                    .map(f64::from)
                    .sum::<f64>();
        assert!((mass - 1.0).abs() < 2e-7);
        assert!(weights.iter().all(|x| x.is_finite() && *x >= 0.0));
        assert!(weights[radius + 1..].iter().all(|x| *x == 0.0));
    }
}
#[test]
fn retained_dirty_closure_atomic_rejection_and_retirement() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let texture = source(&device, [17, 13], HDR);
    let capture = HdrBloomCapture {
        source: &texture,
        revision: 1,
    };
    let mut unsupported = noop_filter(&device);
    unsupported.float_targets = false;
    let mut empty = HdrBloomScope::default();
    assert_eq!(
        unsupported.prepare(&device, &queue, &mut empty, capture, parameters(2.25)),
        Err(HdrBloomPrepareError::UnsupportedFloatAttachment)
    );
    assert!(unsupported.programs.is_none());
    assert_eq!(empty.scratch_bytes(), 0);
    let mut filter = noop_filter(&device);
    let mut scope = HdrBloomScope::default();
    assert!(filter.programs.is_none());
    assert_eq!(scope.scratch_bytes(), 0);
    let p = parameters(2.25);
    let stats = filter
        .prepare(&device, &queue, &mut scope, capture, p)
        .unwrap();
    assert_eq!(stats.pipeline_compiles, 5);
    assert_eq!(stats.texture_allocations, 5);
    assert_eq!(stats.bind_group_creations, 5);
    assert_eq!(stats.bytes_uploaded, 1056);
    assert_eq!(stats.scratch_bytes, 17 * 13 * 36);
    assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [1; 5]);
    let linear = scope.linear_output().unwrap().clone();
    let display = scope.display_output().unwrap().clone();
    let stats = filter
        .prepare(&device, &queue, &mut scope, capture, p)
        .unwrap();
    assert_eq!(
        stats,
        HdrBloomStats {
            scratch_bytes: 17 * 13 * 36,
            ..Default::default()
        }
    );
    assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [0; 5]);
    for value in [f64::NAN, f64::INFINITY, -0.1, 64.001] {
        assert_eq!(
            filter.prepare(
                &device,
                &queue,
                &mut scope,
                capture,
                HdrBloomParameters {
                    sigma_pixels: value,
                    ..p
                }
            ),
            Err(HdrBloomPrepareError::InvalidRadius)
        );
    }
    for value in [f32::NAN, f32::INFINITY, -0.1, 64.001] {
        assert_eq!(
            filter.prepare(
                &device,
                &queue,
                &mut scope,
                capture,
                HdrBloomParameters {
                    threshold: value,
                    ..p
                }
            ),
            Err(HdrBloomPrepareError::InvalidThreshold)
        );
    }
    for value in [f32::NAN, f32::INFINITY, -0.1, 8.001] {
        assert_eq!(
            filter.prepare(
                &device,
                &queue,
                &mut scope,
                capture,
                HdrBloomParameters {
                    intensity: value,
                    ..p
                }
            ),
            Err(HdrBloomPrepareError::InvalidIntensity)
        );
    }
    assert_eq!(
        filter.prepare(
            &device,
            &queue,
            &mut scope,
            capture,
            HdrBloomParameters {
                scratch_budget_bytes: 17 * 13 * 36 - 1,
                ..p
            }
        ),
        Err(HdrBloomPrepareError::ScratchBudgetExceeded)
    );
    let ldr = source(&device, [17, 13], DISPLAY);
    assert_eq!(
        filter.prepare(
            &device,
            &queue,
            &mut scope,
            HdrBloomCapture {
                source: &ldr,
                revision: 1
            },
            p
        ),
        Err(HdrBloomPrepareError::UnsupportedCapture)
    );
    let mut foreign = noop_filter(&device);
    assert_eq!(
        foreign.prepare(&device, &queue, &mut scope, capture, p),
        Err(HdrBloomPrepareError::WrongOwner)
    );
    assert!(foreign.programs.is_none());
    assert_eq!(scope.linear_output(), Some(&linear));
    assert_eq!(scope.display_output(), Some(&display));
    assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [0; 5]);
    // An intensity-only edit updates one small uniform and the final two passes.
    let quiet = HdrBloomParameters {
        intensity: 0.0,
        ..p
    };
    let stats = filter
        .prepare(&device, &queue, &mut scope, capture, quiet)
        .unwrap();
    assert_eq!(stats.bytes_uploaded, 16);
    assert_eq!(stats.texture_allocations, 0);
    assert_eq!(stats.kernel_builds, 0);
    assert_eq!(
        encode(&device, &queue, &filter, &mut scope).passes,
        [0, 0, 0, 1, 1]
    );
    // Radius/threshold edits while disabled defer work until bloom is enabled.
    let quiet = HdrBloomParameters {
        sigma_pixels: 0.65,
        threshold: 0.5,
        ..quiet
    };
    filter
        .prepare(&device, &queue, &mut scope, capture, quiet)
        .unwrap();
    assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [0; 5]);
    let audible = HdrBloomParameters {
        intensity: 1.0,
        ..quiet
    };
    filter
        .prepare(&device, &queue, &mut scope, capture, audible)
        .unwrap();
    assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [1; 5]);
    filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            capture,
            HdrBloomParameters {
                sigma_pixels: 3.0,
                ..audible
            },
        )
        .unwrap();
    assert_eq!(
        encode(&device, &queue, &filter, &mut scope).passes,
        [0, 1, 1, 1, 1]
    );
    let stats = filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            capture,
            HdrBloomParameters {
                sigma_pixels: 3.0,
                threshold: 2.0,
                ..audible
            },
        )
        .unwrap();
    assert_eq!(stats.bytes_uploaded, 16);
    assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [1; 5]);
    // Changed source identity rebinds only its two consumers and reuses images.
    let replacement = source(&device, [17, 13], HDR);
    let stats = filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            HdrBloomCapture {
                source: &replacement,
                revision: 1,
            },
            p,
        )
        .unwrap();
    assert_eq!(stats.texture_allocations, 0);
    assert_eq!(stats.bind_group_creations, 2);
    assert_eq!(scope.linear_output(), Some(&linear));
    assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [1; 5]);
    filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            HdrBloomCapture {
                source: &replacement,
                revision: 2,
            },
            p,
        )
        .unwrap();
    assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [1; 5]);
    scope.invalidate();
    assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [1; 5]);
    let resized = source(&device, [7, 5], HDR);
    let stats = filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            HdrBloomCapture {
                source: &resized,
                revision: 1,
            },
            p,
        )
        .unwrap();
    assert_eq!(stats.texture_allocations, 5);
    assert_eq!(stats.pipeline_compiles, 0);
    assert_eq!(stats.scratch_bytes, 7 * 5 * 36);
    scope.clear();
    assert!(scope.linear_output().is_none());
    assert!(scope.display_output().is_none());
    assert_eq!(scope.scratch_bytes(), 0);
    let stats = foreign
        .prepare(&device, &queue, &mut scope, capture, p)
        .unwrap();
    assert_eq!(stats.pipeline_compiles, 5);
}

fn upload(queue: &wgpu::Queue, texture: &wgpu::Texture, values: &[[f64; 4]]) {
    let bytes: Vec<u8> = values
        .iter()
        .flatten()
        .flat_map(|v| f16::from_f64(*v).to_le_bytes())
        .collect();
    queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width() * 8),
            rows_per_image: Some(texture.height()),
        },
        texture.size(),
    );
}
fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_pixel = if texture.format() == HDR { 8 } else { 4 };
    let row = texture.width() * bytes_per_pixel;
    let stride = row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("HDR readback"),
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
        tx.send(result).unwrap()
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.get_mapped_range(..).unwrap();
    let bytes = mapped
        .chunks_exact(stride as usize)
        .flat_map(|r| r[..row as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    bytes
}
fn decode(bytes: &[u8]) -> Vec<f64> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| f16::from_le_bytes(*b).to_f64())
        .collect()
}
fn fixture(size: [u32; 2], kind: u32) -> Vec<[f64; 4]> {
    (0..size[1])
        .flat_map(|y| {
            (0..size[0]).map(move |x| match kind {
                0 if x == size[0] / 2 && y == size[1] / 2 => [16.0, 4.0, 1.0, 0.5],
                1 => {
                    let a = f64::from((x + 3 * y) % 5) / 4.0;
                    [a * 16.0, a * f64::from(x % 4), a * 0.25, a]
                }
                2 if x == 0 || y == 0 => [32.0, 8.0, 2.0, 0.5],
                3 => [4.0, 1.0, 0.25, 0.5],
                4 => [0.0, 0.0, 0.0, 0.5],
                5 => [0.0009765625, 0.000244140625, 0.0, 0.5],
                _ => [0.0; 4],
            })
        })
        .collect()
}
// Independent f64 full 2-D convolution. No production kernel, extraction
// helper, intermediate readback or separable implementation supplies this oracle.
fn reference(
    input: &[[f64; 4]],
    size: [u32; 2],
    p: HdrBloomParameters,
) -> (Vec<[f64; 4]>, Vec<u8>) {
    let radius = (3.0 * p.sigma_pixels).ceil() as i32;
    let weight = |x: i32, y: i32| {
        if p.sigma_pixels == 0.0 {
            f64::from(x == 0 && y == 0)
        } else {
            (-f64::from(x * x + y * y) / (2.0 * p.sigma_pixels * p.sigma_pixels)).exp()
        }
    };
    let mass = (-radius..=radius)
        .flat_map(|y| (-radius..=radius).map(move |x| weight(x, y)))
        .sum::<f64>();
    let extracted: Vec<_> = input
        .iter()
        .map(|source| {
            let luminance = 0.2126 * source[0] + 0.7152 * source[1] + 0.0722 * source[2];
            let amount = if luminance > f64::from(p.threshold) {
                (luminance - f64::from(p.threshold)) / luminance
            } else {
                0.0
            };
            source.map(|v| v * amount)
        })
        .collect();
    let mut linear = Vec::with_capacity(input.len());
    let mut bytes = Vec::with_capacity(input.len() * 4);
    for (at, base) in input.iter().enumerate() {
        let x = (at % size[0] as usize) as i32;
        let y = (at / size[0] as usize) as i32;
        let mut blur = [0.0; 4];
        // Restrict source enumeration to valid pixels while retaining the full
        // square kernel normalization, so edges have transparent padding.
        for yy in (y - radius).max(0)..=(y + radius).min(size[1] as i32 - 1) {
            for xx in (x - radius).max(0)..=(x + radius).min(size[0] as i32 - 1) {
                let source = extracted[(yy as u32 * size[0] + xx as u32) as usize];
                let coefficient = weight(xx - x, yy - y) / mass;
                for c in 0..4 {
                    blur[c] += source[c] * coefficient;
                }
            }
        }
        let alpha = base[3] + (1.0 - base[3]) * (blur[3] * f64::from(p.intensity)).clamp(0.0, 1.0);
        let color = [
            base[0] + blur[0] * f64::from(p.intensity),
            base[1] + blur[1] * f64::from(p.intensity),
            base[2] + blur[2] * f64::from(p.intensity),
            alpha,
        ];
        for value in color.iter().take(3) {
            let straight = if alpha == 0.0 { 0.0 } else { value / alpha };
            let mapped = straight / (1.0 + straight);
            let srgb = if mapped <= 0.0031308 {
                12.92 * mapped
            } else {
                1.055 * mapped.powf(1.0 / 2.4) - 0.055
            };
            bytes.push((srgb * alpha * 255.0).round() as u8);
        }
        bytes.push((alpha * 255.0).round() as u8);
        linear.push(color);
    }
    (linear, bytes)
}
fn byte_error(actual: &[u8], expected: &[u8]) -> u8 {
    actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap()
}
#[test]
#[ignore = "requires real native float render targets; missing adapter fails qualification"]
fn hdr_bloom_matches_independent_linear_reference_and_rejects_shader_negative_controls() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("HDR bloom adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let size = [17, 13];
    let texture = source(&device, size, HDR);
    let mut filter = HdrBloomFilter::new(&adapter, &device);
    let mut scope = HdrBloomScope::default();
    let artifact = std::env::var_os("NOON_HDR_BLOOM_ARTIFACT_DIR").map(std::path::PathBuf::from);
    if let Some(path) = &artifact {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join("adapter.txt"),
            format!(
                "{:?}\nRGBA16Float: {:?}\n",
                adapter.get_info(),
                adapter.get_texture_format_features(HDR)
            ),
        )
        .unwrap();
    }
    let mut profiles = 0;
    let mut maximum_absolute_error = 0.0f64;
    let mut maximum_byte_error = 0;
    let mut witnessed_hdr = false;
    for kind in 0..7 {
        let input = fixture(size, kind);
        upload(&queue, &texture, &input);
        for sigma in [0.0, 0.65, 2.25, 64.0] {
            for intensity in [0.0, 1.5, 8.0] {
                let p = HdrBloomParameters {
                    intensity,
                    threshold: if kind == 5 { 0.0 } else { 1.0 },
                    ..parameters(sigma)
                };
                filter
                    .prepare(
                        &device,
                        &queue,
                        &mut scope,
                        HdrBloomCapture {
                            source: &texture,
                            revision: kind as u64 + 1,
                        },
                        p,
                    )
                    .unwrap();
                encode(&device, &queue, &filter, &mut scope);
                let actual_linear_bytes = read(&device, &queue, scope.linear_output().unwrap());
                let actual_linear = decode(&actual_linear_bytes);
                let actual_display = read(&device, &queue, scope.display_output().unwrap());
                let (expected_linear, expected_display) = reference(&input, size, p);
                for (at, (actual, expected)) in actual_linear
                    .iter()
                    .zip(expected_linear.iter().flatten())
                    .enumerate()
                {
                    let error = (actual - expected).abs();
                    maximum_absolute_error = maximum_absolute_error.max(error);
                    assert!(actual.is_finite() && error <= 0.006 + 0.004 * expected.abs(), "profile={profiles} channel={at} actual={actual} expected={expected} error={error}");
                    if kind == 6 {
                        assert_eq!(*actual, 0.0, "transparent input must remain exactly empty");
                    }
                    if at % 4 < 3 && *actual > 1.0 {
                        witnessed_hdr = true;
                    }
                }
                let error = byte_error(&actual_display, &expected_display);
                maximum_byte_error = maximum_byte_error.max(error);
                assert!(error <= 2, "display profile={profiles} max error={error}");
                assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [0; 5]);
                if let Some(path) = &artifact {
                    std::fs::write(
                        path.join(format!("profile-{profiles:03}.rgba16f")),
                        actual_linear_bytes,
                    )
                    .unwrap();
                    std::fs::write(
                        path.join(format!("profile-{profiles:03}.expected-rgba64f")),
                        expected_linear
                            .iter()
                            .flatten()
                            .flat_map(|v| v.to_le_bytes())
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
                    std::fs::write(
                        path.join(format!("profile-{profiles:03}.rgba")),
                        &actual_display,
                    )
                    .unwrap();
                    std::fs::write(
                        path.join(format!("profile-{profiles:03}.expected-rgba")),
                        &expected_display,
                    )
                    .unwrap();
                    std::fs::write(path.join(format!("profile-{profiles:03}.json")), format!("{{\"kind\":{kind},\"sigma\":{sigma},\"threshold\":{},\"intensity\":{intensity}}}", p.threshold)).unwrap();
                }
                profiles += 1;
            }
        }
    }
    assert!(witnessed_hdr, "float output must retain values above one");
    // Separate noop instances can compare equal: exercise cross-device rejection
    // with actual adapter devices, before any foreign resource is touched.
    let (other_device, other_queue) =
        pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let old_output = scope.linear_output().unwrap().clone();
    assert_eq!(
        filter.prepare(
            &other_device,
            &other_queue,
            &mut scope,
            HdrBloomCapture {
                source: &texture,
                revision: 1
            },
            parameters(2.25)
        ),
        Err(HdrBloomPrepareError::WrongDevice)
    );
    assert_eq!(scope.linear_output(), Some(&old_output));

    // Actual device lifecycle: source rebinding reuses resources, an abandoned
    // encoder is retried, resizing replaces only the bounded image set, and
    // recreation after retirement reuses the filter's programs.
    let p = parameters(2.25);
    let input = fixture(size, 1);
    upload(&queue, &texture, &input);
    filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            HdrBloomCapture {
                source: &texture,
                revision: 100,
            },
            p,
        )
        .unwrap();
    encode(&device, &queue, &filter, &mut scope);
    let baseline = read(&device, &queue, scope.display_output().unwrap());
    let old_linear = scope.linear_output().unwrap().clone();
    let replacement = source(&device, size, HDR);
    upload(&queue, &replacement, &input);
    let stats = filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            HdrBloomCapture {
                source: &replacement,
                revision: 1,
            },
            p,
        )
        .unwrap();
    assert_eq!(stats.texture_allocations, 0);
    assert_eq!(stats.bind_group_creations, 2);
    assert_eq!(scope.linear_output(), Some(&old_linear));
    encode(&device, &queue, &filter, &mut scope);
    assert_eq!(
        read(&device, &queue, scope.display_output().unwrap()),
        baseline
    );
    let input = fixture(size, 2);
    upload(&queue, &replacement, &input);
    filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            HdrBloomCapture {
                source: &replacement,
                revision: 2,
            },
            p,
        )
        .unwrap();
    let mut abandoned = device.create_command_encoder(&Default::default());
    assert_eq!(filter.encode(&mut abandoned, &mut scope).passes, [1; 5]);
    drop(abandoned);
    scope.invalidate();
    assert_eq!(encode(&device, &queue, &filter, &mut scope).passes, [1; 5]);
    let (_, expected) = reference(&input, size, p);
    let actual = read(&device, &queue, scope.display_output().unwrap());
    assert!(byte_error(&actual, &expected) <= 2);
    if let Some(path) = &artifact {
        std::fs::write(path.join("lifecycle-retry.rgba"), actual).unwrap();
        std::fs::write(path.join("lifecycle-retry.expected-rgba"), expected).unwrap();
    }
    let small_size = [7, 5];
    let resized = source(&device, small_size, HDR);
    let input = fixture(small_size, 1);
    upload(&queue, &resized, &input);
    let (_, expected) = reference(&input, small_size, p);
    for retired in [false, true] {
        if retired {
            scope.clear();
            assert_eq!(scope.scratch_bytes(), 0);
            assert!(scope.linear_output().is_none());
        }
        let stats = filter
            .prepare(
                &device,
                &queue,
                &mut scope,
                HdrBloomCapture {
                    source: &resized,
                    revision: 1,
                },
                p,
            )
            .unwrap();
        assert_eq!(stats.texture_allocations, 5);
        assert_eq!(stats.pipeline_compiles, 0);
        assert_eq!(stats.scratch_bytes, 7 * 5 * 36);
        encode(&device, &queue, &filter, &mut scope);
        let actual = read(&device, &queue, scope.display_output().unwrap());
        assert!(byte_error(&actual, &expected) <= 2);
        if let Some(path) = &artifact {
            std::fs::write(
                path.join(format!("lifecycle-resized-retired-{retired}.rgba")),
                actual,
            )
            .unwrap();
            std::fs::write(
                path.join(format!("lifecycle-resized-retired-{retired}.expected-rgba")),
                &expected,
            )
            .unwrap();
        }
    }
    // Force actual shader regressions, then run the same independent oracle.
    let input = fixture(size, 1);
    upload(&queue, &texture, &input);
    let p = parameters(2.25);
    let (_, expected) = reference(&input, size, p);
    let controls = [
        (
            "early-clamp",
            SHADER.replace(
                "let base = textureLoad(primary, point, 0);",
                "let base = clamp(textureLoad(primary, point, 0), vec4<f32>(0.0), vec4<f32>(1.0));",
            ),
        ),
        (
            "double-transfer",
            SHADER.replace(
                "srgb_encode(mapped) * radiance.a",
                "srgb_encode(srgb_encode(mapped)) * radiance.a",
            ),
        ),
        (
            "double-premultiply",
            SHADER.replace(
                "srgb_encode(mapped) * radiance.a",
                "srgb_encode(mapped) * radiance.a * radiance.a",
            ),
        ),
    ];
    let mut negative_errors = Vec::new();
    for (name, shader) in controls {
        assert_ne!(shader, SHADER);
        let mut negative_filter = HdrBloomFilter::new(&adapter, &device);
        negative_filter.programs = Some(Programs::from_source(&device, &shader));
        let mut negative_scope = HdrBloomScope::default();
        negative_filter
            .prepare(
                &device,
                &queue,
                &mut negative_scope,
                HdrBloomCapture {
                    source: &texture,
                    revision: 1,
                },
                p,
            )
            .unwrap();
        encode(&device, &queue, &negative_filter, &mut negative_scope);
        let actual = read(&device, &queue, negative_scope.display_output().unwrap());
        let error = byte_error(&actual, &expected);
        assert!(
            error > 2,
            "actual shader negative control {name} escaped oracle: {error}"
        );
        if let Some(path) = &artifact {
            std::fs::write(path.join(format!("negative-{name}.rgba")), actual).unwrap();
        }
        negative_errors.push(format!("\"{name}\":{error}"));
    }
    let summary = format!("{{\"profiles\":{profiles},\"width\":17,\"height\":13,\"linear_channels\":{},\"display_channels\":{},\"linear_absolute_bound\":0.006,\"linear_relative_bound\":0.004,\"maximum_absolute_error\":{maximum_absolute_error},\"maximum_display_byte_error\":{maximum_byte_error},\"display_byte_bound\":2,\"witnessed_hdr\":{witnessed_hdr},\"real_gpu_lifecycle_cases\":4,\"shader_negative_controls\":{{{}}}}}", profiles * 17 * 13 * 4, profiles * 17 * 13 * 4, negative_errors.join(","));
    if let Some(path) = &artifact {
        std::fs::write(path.join("manifest.json"), &summary).unwrap();
    }
    eprintln!("HDR bloom qualification: {summary}");
}
