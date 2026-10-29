use super::*;
use crate::gpu::{
    test_read_hdr, HdrBloomCapture, HdrBloomFilter, HdrBloomParameters, HdrBloomScope,
};

fn texture(device: &wgpu::Device, size: [u32; 2], format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("encoded display qualification"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
fn rgba(mut bytes: Vec<u8>, format: wgpu::TextureFormat) -> Vec<u8> {
    if matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for pixel in bytes.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
    }
    bytes
}
fn present(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    presenter: &EncodedDisplayPresenter,
    target: &wgpu::Texture,
) -> Vec<u8> {
    let mut encoder = device.create_command_encoder(&Default::default());
    assert_eq!(presenter.encode(&mut encoder, target).unwrap().passes, 1);
    queue.submit([encoder.finish()]);
    rgba(test_read_hdr(device, queue, target), target.format())
}
#[test]
fn encoded_display_validation_reuse_replacement_and_retirement_are_bounded() {
    let (device, _queue) = wgpu::Device::noop(&Default::default());
    let size = [7, 5];
    let source = texture(&device, size, wgpu::TextureFormat::Rgba8Unorm);
    let target = texture(&device, size, wgpu::TextureFormat::Rgba8Unorm);
    assert!(matches!(
        EncodedDisplayPresenter::new(
            &device,
            wgpu::TextureFormat::Rgba16Float,
            OutputTransfer::Direct
        ),
        Err(DisplayPresentationError::UnsupportedTarget)
    ));
    let mut p =
        EncodedDisplayPresenter::new(&device, target.format(), OutputTransfer::Direct).unwrap();
    let mut encoder = device.create_command_encoder(&Default::default());
    assert_eq!(
        p.encode(&mut encoder, &target),
        Err(DisplayPresentationError::Unprepared)
    );
    assert_eq!(
        p.prepare(&texture(&device, size, wgpu::TextureFormat::Rgba16Float)),
        Err(DisplayPresentationError::UnsupportedSource)
    );
    assert!(p.program.is_none());
    assert_eq!(
        p.prepare(&source).unwrap(),
        DisplayPresentationStats {
            pipeline_compiles: 1,
            bind_group_creations: 1,
            passes: 0
        }
    );
    assert_eq!(
        p.prepare(&source).unwrap(),
        DisplayPresentationStats::default()
    );
    let retained = p.source.clone();
    assert!(p
        .prepare(&texture(&device, size, wgpu::TextureFormat::Rgba8UnormSrgb))
        .is_err());
    assert_eq!(
        p.source, retained,
        "invalid replacement preserves the published source lease"
    );
    assert_eq!(
        p.encode(&mut encoder, &texture(&device, [8, 5], target.format())),
        Err(DisplayPresentationError::ExtentMismatch)
    );
    assert_eq!(
        p.encode(&mut encoder, &source),
        Err(DisplayPresentationError::UnsupportedTarget)
    );
    let replacement = texture(&device, [9, 6], wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(p.prepare(&replacement).unwrap().pipeline_compiles, 0);
    assert_eq!(
        p.prepare(&replacement).unwrap(),
        DisplayPresentationStats::default()
    );
    p.clear();
    assert!(p.source.is_none() && p.binding.is_none());
    assert!(p.program.is_some());
    assert_eq!(p.prepare(&source).unwrap().pipeline_compiles, 0);
}

#[test]
#[ignore = "requires real render targets; missing adapter fails qualification"]
fn encoded_display_round_trips_four_formats_and_preserves_real_bloom_output() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let size = [17, 13];
    let formats = [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ];
    let expected: Vec<u8> = (0..size[0] * size[1])
        .flat_map(|i| {
            let a = [0u32, 1, 7, 64, 128, 255][i as usize % 6];
            [
                ((i * 31 % 256) * a + 127) / 255,
                ((i * 73 % 256) * a + 127) / 255,
                ((i * 137 % 256) * a + 127) / 255,
                a,
            ]
            .map(|x| x as u8)
        })
        .collect();
    let root = std::env::var_os("NOON_HDR_PRESENTATION_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(root) = &root {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(
            root.join("adapter.txt"),
            format!("{:?}", adapter.get_info()),
        )
        .unwrap();
    }
    let mut cases = Vec::new();
    let mut roundtrip_max = 0;
    for source_format in formats[..2].iter().copied() {
        let source = texture(&device, size, source_format);
        let bytes = rgba(expected.clone(), source_format);
        queue.write_texture(
            source.as_image_copy(),
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size[0] * 4),
                rows_per_image: Some(size[1]),
            },
            source.size(),
        );
        for format in formats {
            let target = texture(&device, size, format);
            let mut p =
                EncodedDisplayPresenter::new(&device, format, OutputTransfer::Direct).unwrap();
            p.prepare(&source).unwrap();
            let actual = present(&device, &queue, &p, &target);
            for (a, e) in actual
                .as_chunks::<4>()
                .0
                .iter()
                .zip(expected.as_chunks::<4>().0.iter())
            {
                assert_eq!(a[3], e[3], "alpha is never transferred");
                for c in 0..3 {
                    roundtrip_max = roundtrip_max.max(a[c].abs_diff(e[c]));
                    assert!(a[c].abs_diff(e[c]) <= u8::from(format.is_srgb()));
                }
            }
            assert_eq!(
                p.prepare(&source).unwrap(),
                DisplayPresentationStats::default()
            );
            assert_eq!(
                present(&device, &queue, &p, &target),
                actual,
                "repeated presentation is exact"
            );
            let name = format!("roundtrip-{source_format:?}-{format:?}");
            if let Some(root) = &root {
                std::fs::write(root.join(format!("{name}.rgba8")), &actual).unwrap();
            }
            cases.push(serde_json::json!({"name":name,"expected":expected,"tolerance":u8::from(format.is_srgb()),"alpha_tolerance":0}));
        }
    }

    // Independently generated half-exact radiance includes alpha zero, partial
    // coverage and >1 values. The existing independent full-2D f64 reference is
    // the specification oracle, never a production display readback.
    let input: Vec<[f64; 4]> = (0..size[0] * size[1])
        .map(|i| {
            let a = [0.0, 0.25, 0.5, 1.0][i as usize % 4];
            [a * 16.0, a * 4.0, a * 0.25, a]
        })
        .collect();
    let source = texture(&device, size, wgpu::TextureFormat::Rgba16Float);
    let bytes: Vec<u8> = input
        .iter()
        .flatten()
        .flat_map(|x| half::f16::from_f64(*x).to_le_bytes())
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
    let mut filter = HdrBloomFilter::new(&adapter, &device);
    let mut scope = HdrBloomScope::default();
    let mut bloom_max = 0;
    let mut outputs: Vec<_> = formats
        .into_iter()
        .map(|format| {
            (
                texture(&device, size, format),
                EncodedDisplayPresenter::new(&device, format, OutputTransfer::Direct).unwrap(),
                None::<Vec<u8>>,
            )
        })
        .collect();

    for (phase, (sigma, intensity)) in [(0.0, 0.0), (0.65, 1.5), (2.25, 3.0), (0.0, 0.0)]
        .into_iter()
        .enumerate()
    {
        let parameters = HdrBloomParameters {
            sigma_pixels: sigma,
            threshold: 1.0,
            intensity,
            scratch_budget_bytes: 1024 * 1024,
        };
        filter
            .prepare(
                &device,
                &queue,
                &mut scope,
                HdrBloomCapture {
                    source: &source,
                    revision: 1,
                },
                parameters,
            )
            .unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        filter.encode(&mut encoder, &mut scope);
        queue.submit([encoder.finish()]);
        let (_, reference) = crate::gpu::hdr_bloom::tests::reference(&input, size, parameters);
        for (target, p, base) in &mut outputs {
            let format = target.format();
            let stats = p.prepare(scope.display_output().unwrap()).unwrap();
            assert_eq!(stats.pipeline_compiles, usize::from(phase == 0));
            assert_eq!(stats.bind_group_creations, usize::from(phase == 0));
            let actual = present(&device, &queue, p, target);
            if phase == 0 {
                *base = Some(actual.clone());
            } else if phase == 3 {
                assert_eq!(
                    base.as_ref(),
                    Some(&actual),
                    "exact zero-energy restoration"
                );
            }
            for (a, e) in actual.iter().zip(&reference) {
                bloom_max = bloom_max.max(a.abs_diff(*e));
                assert!(
                    a.abs_diff(*e) <= 2,
                    "unchanged HDR display bound: {a} vs {e}"
                );
            }
            let name = format!("bloom-{phase}-{format:?}");
            if let Some(root) = &root {
                std::fs::write(root.join(format!("{name}.rgba8")), &actual).unwrap();
            }
            cases.push(serde_json::json!({"name":name,"expected":reference,"tolerance":2,"sigma":sigma,"intensity":intensity}));
        }
    }
    // Real wrong-transfer GPU consumers: encoding twice into sRGB, and decoding
    // into plain UNORM. Both must fail the same frozen 1-byte bound.
    let source = texture(&device, size, wgpu::TextureFormat::Rgba8Unorm);
    queue.write_texture(
        source.as_image_copy(),
        &expected,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size[0] * 4),
            rows_per_image: Some(size[1]),
        },
        source.size(),
    );
    let mut negatives = Vec::new();
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let target = texture(&device, size, format);
        let mut p = EncodedDisplayPresenter::new(&device, format, OutputTransfer::Direct).unwrap();
        p.decode_before_store = !p.decode_before_store;
        p.prepare(&source).unwrap();
        let actual = present(&device, &queue, &p, &target);
        let failed = actual
            .iter()
            .zip(&expected)
            .filter(|(a, e)| a.abs_diff(**e) > 1)
            .count();
        let maximum = actual
            .iter()
            .zip(&expected)
            .map(|(a, e)| a.abs_diff(*e))
            .max()
            .unwrap();
        assert!(
            failed > 100 && maximum > 30,
            "wrong transfer must be observably rejected"
        );
        for (a, e) in actual
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0.iter())
        {
            assert_eq!(a[3], e[3]);
        }
        let name = format!("negative-{format:?}");
        if let Some(root) = &root {
            std::fs::write(root.join(format!("{name}.rgba8")), &actual).unwrap();
        }
        negatives.push(serde_json::json!({"name":name,"failed_channels":failed,"maximum_error":maximum,"expected":expected}));
    }
    if let Some(root) = root {
        std::fs::write(
            root.join("report.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "size":size,"cases":cases,"negative_controls":negatives,
                "roundtrip_maximum_error":roundtrip_max,"bloom_maximum_error":bloom_max,
                "hdr_input":input,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    eprintln!("encoded display: 8 byte-roundtrip cases, 16 real Bloom cases; maxima={roundtrip_max}/{bloom_max}; two real GPU negatives rejected");
}
