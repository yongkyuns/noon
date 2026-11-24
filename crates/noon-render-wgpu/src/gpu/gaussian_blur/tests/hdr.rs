use super::*;
use crate::gpu::{HdrBloomCapture, HdrBloomFilter, HdrBloomParameters, HdrBloomScope};
use half::f16;

fn float_source(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("linear Gaussian oracle source"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: LINEAR_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
fn submit(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    filter: &GaussianBlurFilter,
    scope: &mut GaussianBlurScope,
) -> GaussianBlurStats {
    let mut encoder = device.create_command_encoder(&Default::default());
    let stats = filter.encode(&mut encoder, scope);
    queue.submit([encoder.finish()]);
    stats
}
// A full two-dimensional f64 convolution with a normalized square kernel and
// transparent padding. No production weights or intermediate pixels are used.
fn reference(input: &[[f64; 4]], size: [u32; 2], sigma: f64, opacity: f32) -> Vec<[f64; 4]> {
    let radius = (3.0 * sigma).ceil() as i32;
    let weight = |x: i32, y: i32| (-f64::from(x * x + y * y) / (2.0 * sigma * sigma)).exp();
    let mass: f64 = (-radius..=radius)
        .flat_map(|y| (-radius..=radius).map(move |x| weight(x, y)))
        .sum();
    (0..size[0] * size[1])
        .map(|at| {
            let x = (at % size[0]) as i32;
            let y = (at / size[0]) as i32;
            let mut result = [0.0; 4];
            for yy in (y - radius).max(0)..=(y + radius).min(size[1] as i32 - 1) {
                for xx in (x - radius).max(0)..=(x + radius).min(size[0] as i32 - 1) {
                    let source = input[(yy as u32 * size[0] + xx as u32) as usize];
                    let w = weight(xx - x, yy - y) / mass * f64::from(opacity);
                    for c in 0..4 {
                        result[c] += source[c] * w;
                    }
                }
            }
            result
        })
        .collect()
}
#[test]
fn linear_storage_cost_dirty_closure_and_atomic_budget_rejection() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let source = float_source(&device, [17, 13]);
    let capture = GaussianBlurCapture {
        source: &source,
        revision: 1,
    };
    let mut filter = GaussianBlurFilter::default();
    let mut scope = GaussianBlurScope::default();
    let p = parameters(1.25);
    let initial = filter
        .prepare(&device, &queue, &mut scope, capture, p)
        .unwrap();
    assert_eq!(initial.texture_allocations, 3);
    assert_eq!(initial.pipeline_compiles, 3);
    assert_eq!(initial.scratch_bytes, 17 * 13 * 24);
    let first = scope.output().unwrap().clone();
    let encoded = submit(&device, &queue, &filter, &mut scope);
    assert_eq!((encoded.blur_passes, encoded.output_passes), (2, 1));
    assert_eq!(
        filter
            .prepare(&device, &queue, &mut scope, capture, p)
            .unwrap()
            .bytes_uploaded,
        0
    );
    let encoded = submit(&device, &queue, &filter, &mut scope);
    assert_eq!((encoded.blur_passes, encoded.output_passes), (0, 0));
    let faded = GaussianBlurParameters {
        scope_opacity: 0.25,
        ..p
    };
    let stats = filter
        .prepare(&device, &queue, &mut scope, capture, faded)
        .unwrap();
    assert_eq!(stats.bytes_uploaded, 16);
    assert_eq!(stats.texture_allocations, 0);
    let encoded = submit(&device, &queue, &filter, &mut scope);
    assert_eq!((encoded.blur_passes, encoded.output_passes), (0, 1));
    assert_eq!(
        filter.prepare(
            &device,
            &queue,
            &mut scope,
            capture,
            GaussianBlurParameters {
                scratch_budget_bytes: initial.scratch_bytes - 1,
                ..p
            }
        ),
        Err(GaussianBlurPrepareError::ScratchBudgetExceeded)
    );
    assert_eq!(scope.output(), Some(&first));
    assert_eq!(
        filter
            .prepare(&device, &queue, &mut scope, capture, faded)
            .unwrap()
            .bytes_uploaded,
        0
    );
    let changed = GaussianBlurCapture {
        revision: 2,
        ..capture
    };
    assert_eq!(
        filter
            .prepare(&device, &queue, &mut scope, changed, faded)
            .unwrap()
            .texture_allocations,
        0
    );
    let encoded = submit(&device, &queue, &filter, &mut scope);
    assert_eq!((encoded.blur_passes, encoded.output_passes), (2, 1));
    scope.invalidate();
    let encoded = submit(&device, &queue, &filter, &mut scope);
    assert_eq!((encoded.blur_passes, encoded.output_passes), (2, 1));
    let replacement = float_source(&device, [17, 13]);
    let stats = filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            GaussianBlurCapture {
                source: &replacement,
                revision: 2,
            },
            faded,
        )
        .unwrap();
    assert_eq!(stats.texture_allocations, 0);
    assert_eq!(stats.bind_group_creations, 1);
    submit(&device, &queue, &filter, &mut scope);
    let resized = float_source(&device, [19, 11]);
    let stats = filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            GaussianBlurCapture {
                source: &resized,
                revision: 2,
            },
            faded,
        )
        .unwrap();
    assert_eq!(stats.texture_allocations, 3);
    assert_eq!(stats.pipeline_compiles, 0);
    assert_eq!(stats.scratch_bytes, 19 * 11 * 24);
    scope.clear();
    assert_eq!(scope.scratch_bytes(), 0);
    let stats = filter
        .prepare(&device, &queue, &mut scope, capture, p)
        .unwrap();
    assert_eq!(stats.texture_allocations, 3);
    assert_eq!(stats.pipeline_compiles, 0);
    // Switching the caller-owned scope between compatibility and linear storage
    // rebuilds only format-dependent resources, then reuses each program profile.
    let compat = super::source(&device, [17, 13]);
    let stats = filter
        .prepare(
            &device,
            &queue,
            &mut scope,
            GaussianBlurCapture {
                source: &compat,
                revision: 1,
            },
            p,
        )
        .unwrap();
    assert_eq!(stats.texture_allocations, 7);
    assert_eq!(stats.pipeline_compiles, 3);
    let stats = filter
        .prepare(&device, &queue, &mut scope, capture, p)
        .unwrap();
    assert_eq!(stats.texture_allocations, 3);
    assert_eq!(stats.pipeline_compiles, 0);
}
#[test]
#[ignore = "requires a real native adapter; missing adapter fails qualification"]
fn linear_gaussian_preserves_radiance_and_feeds_real_bloom() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("linear Gaussian adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let size = [17, 13];
    let source = float_source(&device, size);
    let mut filter = GaussianBlurFilter::default();
    let mut scope = GaussianBlurScope::default();
    let mut bloom = HdrBloomFilter::new(&adapter, &device);
    let mut bloom_scope = HdrBloomScope::default();
    let bp = HdrBloomParameters {
        sigma_pixels: 1.25,
        threshold: 0.5,
        intensity: 1.5,
        scratch_budget_bytes: 1_000_000,
    };
    let root = std::env::var_os("NOON_HDR_GAUSSIAN_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(root) = &root {
        std::fs::create_dir_all(root).unwrap();
    }
    let mut cases = Vec::new();
    let mut maximum = 0.0f64;
    let mut display_max = 0;
    let mut witness = 0.0f64;
    for kind in 0..4u64 {
        let input: Vec<[f64; 4]> = (0..size[0] * size[1])
            .map(|at| {
                let x = at % size[0];
                let y = at / size[0];
                let a = match kind {
                    0 if x == 8 && y == 6 => 0.75,
                    1 => f64::from((x * 3 + y * 5) % 5) / 4.0,
                    2 if x == 0 || y == 0 => 0.5,
                    3 => 0.0009765625,
                    _ => 0.0,
                };
                [16.0 * a, 4.0 * a, a, a]
            })
            .collect();
        let bytes: Vec<u8> = input
            .iter()
            .flatten()
            .flat_map(|v| f16::from_f64(*v).to_bits().to_le_bytes())
            .collect();
        queue.write_texture(
            source.as_image_copy(),
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size[0] * 8),
                rows_per_image: Some(size[1]),
            },
            source.size(),
        );
        for sigma in [0.65, 1.25, 3.0] {
            for opacity in [1.0, 0.25, 0.65] {
                let p = GaussianBlurParameters {
                    scope_opacity: opacity,
                    ..parameters(sigma)
                };
                let stats = filter
                    .prepare(
                        &device,
                        &queue,
                        &mut scope,
                        GaussianBlurCapture {
                            source: &source,
                            revision: kind + 1,
                        },
                        p,
                    )
                    .unwrap();
                assert_eq!(stats.scratch_bytes, u64::from(size[0] * size[1]) * 24);
                submit(&device, &queue, &filter, &mut scope);
                let expected = reference(&input, size, sigma, opacity);
                let raw = crate::gpu::test_read_hdr(&device, &queue, scope.output().unwrap());
                let actual = crate::gpu::test_decode_hdr(&raw);
                for (a, e) in actual.iter().zip(expected.iter().flatten()) {
                    maximum = maximum.max((a - e).abs());
                    witness = witness.max(*a);
                    assert!(
                        (a - e).abs() <= 0.006 + 0.004 * e.abs(),
                        "kind={kind} sigma={sigma} opacity={opacity}: {a} != {e}"
                    );
                }
                bloom
                    .prepare(
                        &device,
                        &queue,
                        &mut bloom_scope,
                        HdrBloomCapture {
                            source: scope.output().unwrap(),
                            revision: cases.len() as u64 + 1,
                        },
                        bp,
                    )
                    .unwrap();
                let mut encoder = device.create_command_encoder(&Default::default());
                bloom.encode(&mut encoder, &mut bloom_scope);
                queue.submit([encoder.finish()]);
                let (linear, display) =
                    crate::gpu::hdr_bloom::tests::reference(&expected, size, bp);
                let bloom_raw = crate::gpu::test_read_hdr(
                    &device,
                    &queue,
                    bloom_scope.linear_output().unwrap(),
                );
                for (a, e) in crate::gpu::test_decode_hdr(&bloom_raw)
                    .iter()
                    .zip(linear.iter().flatten())
                {
                    assert!((a - e).abs() <= 0.006 + 0.004 * e.abs(), "Bloom {a} != {e}");
                }
                let display_raw = crate::gpu::test_read_hdr(
                    &device,
                    &queue,
                    bloom_scope.display_output().unwrap(),
                );
                for (a, e) in display_raw.iter().zip(&display) {
                    display_max = display_max.max(a.abs_diff(*e));
                    assert!(a.abs_diff(*e) <= 2);
                }
                let id = cases.len();
                if let Some(root) = &root {
                    std::fs::write(root.join(format!("{id}.rgba16f")), raw).unwrap();
                    std::fs::write(root.join(format!("{id}.bloom.rgba16f")), bloom_raw).unwrap();
                    std::fs::write(root.join(format!("{id}.display.rgba8")), display_raw).unwrap();
                }
                cases.push(serde_json::json!({"id":id,"kind":kind,"sigma":sigma,"opacity":opacity,"input":input,"expected":expected,"bloom_expected":linear,"display_expected":display}));
                let clean = filter
                    .prepare(
                        &device,
                        &queue,
                        &mut scope,
                        GaussianBlurCapture {
                            source: &source,
                            revision: kind + 1,
                        },
                        p,
                    )
                    .unwrap();
                assert_eq!(clean.bytes_uploaded, 0);
                let encoded = submit(&device, &queue, &filter, &mut scope);
                assert_eq!((encoded.blur_passes, encoded.output_passes), (0, 0));
            }
        }
    }
    assert!(witness > 5.0, "pre-Bloom bright-radiance witness");
    // Compile an actual early-clamping operator, reproducing the unit clamp in
    // the old packed reconstruction. It must fail the same mathematical oracle.
    let clamped = include_str!("../../gaussian_blur_linear.wgsl")
        .replace("return color;", "return clamp(color,vec4(0.0),vec4(1.0));");
    assert_ne!(clamped, include_str!("../../gaussian_blur_linear.wgsl"));
    let mut wrong = GaussianBlurFilter::default();
    wrong.programs[1] = Some(Programs::with_shader(&device, LINEAR_FORMAT, &clamped));
    let input = vec![[12.0, 3.0, 0.75, 0.75]; (size[0] * size[1]) as usize];
    let bytes: Vec<u8> = input
        .iter()
        .flatten()
        .flat_map(|v| f16::from_f64(*v).to_bits().to_le_bytes())
        .collect();
    queue.write_texture(
        source.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size[0] * 8),
            rows_per_image: Some(size[1]),
        },
        source.size(),
    );
    let mut wrong_scope = GaussianBlurScope::default();
    wrong
        .prepare(
            &device,
            &queue,
            &mut wrong_scope,
            GaussianBlurCapture {
                source: &source,
                revision: 10,
            },
            parameters(0.65),
        )
        .unwrap();
    submit(&device, &queue, &wrong, &mut wrong_scope);
    let expected = reference(&input, size, 0.65, 1.0);
    let negative_raw = crate::gpu::test_read_hdr(&device, &queue, wrong_scope.output().unwrap());
    let negative = crate::gpu::test_decode_hdr(&negative_raw)
        .iter()
        .zip(expected.iter().flatten())
        .map(|(a, e)| (a - e).abs())
        .fold(0.0f64, f64::max);
    assert!(negative > 5.0);
    if let Some(root) = root {
        std::fs::write(root.join("early-clamp-negative.rgba16f"), negative_raw).unwrap();
        std::fs::write(root.join("early-clamp-negative-reference.json"), serde_json::to_vec(&serde_json::json!({"input": input, "expected": expected, "sigma": 0.65, "opacity": 1.0})).unwrap()).unwrap();
        std::fs::write(root.join("report.json"),serde_json::to_vec(&serde_json::json!({"size":size,"cases":cases,"maximum_error":maximum,"display_maximum":display_max,"radiance_witness":witness,"early_clamp_negative":negative})).unwrap()).unwrap();
    }
    eprintln!("linear Gaussian: {} profiles, float max={maximum}, display max={display_max}, source witness={witness}, actual early-clamp negative={negative}",cases.len());
}
