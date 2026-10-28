//! Working-color qualification for real authored image objects and image paints.
//! The reference reconstructs immutable straight sRGB texels independently,
//! decodes each tap, associates it, then performs Keys reconstruction/source-over.
use super::{decode, read, reference, srgb_decode, SIZE};
use noon::{ImageMobjectOptions, ImagePaintOptions, RasterImageSampling, Scene};
use noon_core::{SemanticMutationTransaction, Vec2, ViewColorProfile};
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, HdrBloomCapture, HdrBloomFilter,
    HdrBloomParameters, HdrBloomScope, RetainedFramePreparer,
};

const TEXELS: [u8; 16] = [
    128, 32, 224, 128, 240, 120, 16, 64, 16, 240, 128, 0, 80, 160, 224, 255,
];

// Keys' cubic with a=-1, in factored form, independent of the shader polynomial.
fn weight(distance: f64) -> f64 {
    let t = distance.abs();
    if t <= 1.0 {
        1.0 - t * t * (2.0 - t)
    } else if t < 2.0 {
        (2.0 - t).powi(2) * (1.0 - t)
    } else {
        0.0
    }
}
fn tap(x: i32, y: i32, opacity: f64) -> [f64; 4] {
    let at = (y.clamp(0, 1) * 2 + x.clamp(0, 1)) as usize * 4;
    let alpha = f64::from(TEXELS[at + 3]) / 255.0 * opacity;
    [
        srgb_decode(f64::from(TEXELS[at]) / 255.0) * alpha,
        srgb_decode(f64::from(TEXELS[at + 1]) / 255.0) * alpha,
        srgb_decode(f64::from(TEXELS[at + 2]) / 255.0) * alpha,
        alpha,
    ]
}
fn reconstruct(x: u32, y: u32, kind: usize) -> [f64; 4] {
    let opacity = 1.0;
    let u = (f64::from(x) + 0.5) / f64::from(SIZE) * 2.0;
    let v = (f64::from(y) + 0.5) / f64::from(SIZE) * 2.0;
    if kind == 0 {
        return tap(u.floor() as i32, v.floor() as i32, opacity);
    }
    let px = u - 0.5;
    let py = v - 0.5;
    let mut color = [0.0; 4];
    let radius = if kind == 1 { 0..=1 } else { -1..=2 };
    for dy in radius.clone() {
        for dx in radius.clone() {
            let xx = px.floor() as i32 + dx;
            let yy = py.floor() as i32 + dy;
            let w = if kind == 1 {
                (1.0 - (px - f64::from(xx)).abs()) * (1.0 - (py - f64::from(yy)).abs())
            } else {
                weight(px - f64::from(xx)) * weight(py - f64::from(yy))
            };
            let p = tap(xx, yy, opacity);
            for c in 0..4 {
                color[c] += p[c] * w;
            }
        }
    }
    if kind == 2 {
        color[3] = color[3].clamp(0.0, 1.0);
        for c in 0..3 {
            color[c] = color[c].clamp(0.0, color[3]);
        }
    }
    color
}
// Final object opacity applies once, after intrinsic reconstruction/clamping.
fn sample(x: u32, y: u32, kind: usize, opacity: f64) -> [f64; 4] {
    reconstruct(x, y, kind).map(|value| value * opacity)
}

fn over(source: [f64; 4], below: [f64; 4]) -> [f64; 4] {
    std::array::from_fn(|c| source[c] + below[c] * (1.0 - source[3]))
}
fn expected(kind: usize, reverse: bool, clear: [f64; 4], opacity: f64) -> Vec<[f64; 4]> {
    let base = [
        srgb_decode(0.25) * 0.5,
        srgb_decode(0.5) * 0.5,
        srgb_decode(0.75) * 0.5,
        0.5,
    ];
    (0..SIZE * SIZE)
        .map(|i| {
            let image = sample(i % SIZE, i / SIZE, kind, opacity);
            if reverse {
                over(base, over(image, clear))
            } else {
                over(image, over(base, clear))
            }
        })
        .collect()
}

#[test]
#[ignore = "requires real native float targets; missing adapter fails qualification"]
fn retained_image_assets_decode_per_tap_before_filtering_and_reuse_residency() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("retained linear assets"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let artifact =
        std::env::var_os("NOON_RETAINED_ASSET_ARTIFACT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &artifact {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("adapter.txt"), format!("{:?}", adapter.get_info())).unwrap();
    }
    let mut profiles = 0u64;
    let mut max_error = 0.0f64;
    let mut max_display_error = 0;
    for paint in [false, true] {
        for kind in 0..3 {
            for reverse in [false, true] {
                for clear in [[0.0; 4], [0.125, 0.0625, 0.03125, 1.0]] {
                    let mut scene = Scene::new();
                    let camera = scene.camera_frame().unwrap();
                    let mut base = scene.rectangle(8.0, 8.0).unwrap();
                    base.disable_stroke().unwrap();
                    base.set_fill(0.25, 0.5, 0.75, 0.5).unwrap();
                    let sampling = [
                        RasterImageSampling::Nearest,
                        RasterImageSampling::Linear,
                        RasterImageSampling::Bicubic,
                    ][kind];
                    let mut image = if paint {
                        let mut image = scene.rectangle(8.0, 8.0).unwrap();
                        image.disable_stroke().unwrap();
                        let mut options =
                            ImagePaintOptions::rgba8(2, 2, TEXELS.as_slice()).unwrap();
                        options.set_sampling(sampling);
                        image.set_fill_image(options, Some(1.0)).unwrap();
                        image
                    } else {
                        let mut options =
                            ImageMobjectOptions::rgba8(2, 2, TEXELS.as_slice()).unwrap();
                        options.set_height(8.0).unwrap();
                        options.set_sampling(sampling);
                        scene.image(options).unwrap()
                    };
                    image.set_object_opacity(0.5).unwrap();
                    if reverse {
                        scene.add(&image).unwrap();
                        scene.add(&base).unwrap();
                    } else {
                        scene.add(&base).unwrap();
                        scene.add(&image).unwrap();
                    }
                    let mut session = scene.execution_session().unwrap();
                    let mut tx = SemanticMutationTransaction::new();
                    tx.set_view_color_profile(camera.node_id(), ViewColorProfile::LinearHdr);
                    scene.live(&mut session).apply(tx).unwrap();
                    let mut renderer =
                        GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba16Float);
                    renderer.set_viewport(&device, &queue, SIZE, SIZE);
                    renderer.set_camera(
                        &queue,
                        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 8.0)).unwrap(),
                    );
                    let mut text = renderer.create_retained_text_state(&device, &queue);
                    let mut preparer = RetainedFramePreparer::new();
                    let metrics = TextDeviceMetrics::uniform(2.0).unwrap();
                    let publication = session.take_renderer_publication();
                    let prepared = preparer
                        .prepare_publication(&device, &publication, metrics)
                        .unwrap();
                    assert_eq!(prepared.color_profile(), ViewColorProfile::LinearHdr);
                    let uploaded = renderer.upload_retained(&device, &queue, &prepared, &mut text);
                    assert_eq!(uploaded.images.textures_uploaded, 1);
                    assert_eq!(uploaded.images.pixel_bytes_uploaded, TEXELS.len());
                    let mut encoder = device.create_command_encoder(&Default::default());
                    renderer
                        .encode_retained(
                            &mut encoder,
                            &view,
                            &prepared,
                            &text,
                            wgpu::Color {
                                r: clear[0],
                                g: clear[1],
                                b: clear[2],
                                a: clear[3],
                            },
                            None,
                        )
                        .unwrap();
                    queue.submit([encoder.finish()]);
                    let pixels = read(&device, &queue, &target);
                    let expected = expected(kind, reverse, clear, 0.5);
                    for (at, (a, e)) in decode(&pixels)
                        .iter()
                        .zip(expected.iter().flatten())
                        .enumerate()
                    {
                        let err = (a - e).abs();
                        max_error = max_error.max(err);
                        assert!(
                            a.is_finite() && err <= 0.002,
                            "asset {profiles} paint={paint} kind={kind} channel={at} {a} != {e}"
                        );
                    }
                    let mut filter = HdrBloomFilter::new(&adapter, &device);
                    let mut scope = HdrBloomScope::default();
                    let params = HdrBloomParameters {
                        sigma_pixels: 0.65,
                        threshold: 0.05,
                        intensity: 1.5,
                        scratch_budget_bytes: 1024 * 1024,
                    };
                    filter
                        .prepare(
                            &device,
                            &queue,
                            &mut scope,
                            HdrBloomCapture {
                                source: &target,
                                revision: 1,
                            },
                            params,
                        )
                        .unwrap();
                    let mut encoder = device.create_command_encoder(&Default::default());
                    filter.encode(&mut encoder, &mut scope);
                    queue.submit([encoder.finish()]);
                    let display = read(&device, &queue, scope.display_output().unwrap());
                    let (_, display_expected) = reference(&expected, [SIZE, SIZE], params);
                    let error = display
                        .iter()
                        .zip(&display_expected)
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    max_display_error = max_display_error.max(error);
                    assert!(error <= 2);
                    drop(publication);
                    let publication = session.take_renderer_publication();
                    let prepared = preparer
                        .prepare_publication(&device, &publication, metrics)
                        .unwrap();
                    assert_eq!(
                        renderer
                            .upload_retained(&device, &queue, &prepared, &mut text)
                            .bytes_uploaded(),
                        0
                    );
                    drop(publication);
                    for opacity in [0.0, 0.5] {
                        scene
                            .live(&mut session)
                            .set_object_opacity(&image, opacity)
                            .unwrap();
                        let publication = session.take_renderer_publication();
                        let prepared = preparer
                            .prepare_publication(&device, &publication, metrics)
                            .unwrap();
                        let upload =
                            renderer.upload_retained(&device, &queue, &prepared, &mut text);
                        assert_eq!(upload.images.pixel_bytes_uploaded, 0);
                        assert_eq!(upload.images.textures_uploaded, 0);
                        let mut encoder = device.create_command_encoder(&Default::default());
                        renderer
                            .encode_retained(
                                &mut encoder,
                                &view,
                                &prepared,
                                &text,
                                wgpu::Color {
                                    r: clear[0],
                                    g: clear[1],
                                    b: clear[2],
                                    a: clear[3],
                                },
                                None,
                            )
                            .unwrap();
                        queue.submit([encoder.finish()]);
                        let actual = read(&device, &queue, &target);
                        if opacity == 0.5 {
                            assert_eq!(actual, pixels, "exact asset opacity restoration");
                        }
                    }
                    if let Some(dir) = &artifact {
                        std::fs::write(dir.join(format!("asset-{profiles}.json")),format!("{{\"paint\":{paint},\"sampling\":{kind},\"reverse\":{reverse},\"clear\":{clear:?}}}")).unwrap();
                        std::fs::write(dir.join(format!("asset-{profiles}.rgba16f")), pixels)
                            .unwrap();
                        std::fs::write(
                            dir.join(format!("asset-{profiles}.expected-rgba64f")),
                            expected
                                .iter()
                                .flatten()
                                .flat_map(|v| v.to_le_bytes())
                                .collect::<Vec<_>>(),
                        )
                        .unwrap();
                        std::fs::write(dir.join(format!("asset-{profiles}.rgba")), display)
                            .unwrap();
                        std::fs::write(
                            dir.join(format!("asset-{profiles}.expected-rgba")),
                            display_expected,
                        )
                        .unwrap();
                    }
                    profiles += 1;
                }
            }
        }
    }
    let manifest=format!("{{\"profiles\":{profiles},\"float_channels\":{},\"display_channels\":{},\"linear_bound\":0.002,\"max_linear_error\":{max_error},\"display_bound\":2,\"max_display_error\":{max_display_error},\"exact_restorations\":{profiles}}}",profiles*1024,profiles*1024);
    if let Some(dir) = &artifact {
        std::fs::write(dir.join("manifest.json"), &manifest).unwrap();
    }
    eprintln!("retained assets: {manifest}");
}

mod text_scene;
