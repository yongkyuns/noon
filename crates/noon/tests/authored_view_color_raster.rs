//! Actual Rust authoring -> compiler -> Runtime publication -> linear raster ->
//! Bloom -> SDR proof. This qualifies solid/gradient planar preparation only;
//! emission, text/images, retained captures and product hosts have separate gates.
use noon::{Scene, SemanticStyle};
use noon_core::{
    Color, GradientStop, LinearGradient, SemanticMutationTransaction, Vec2, VectorPath,
    ViewColorProfile,
};
use noon_render_wgpu::{
    Camera2D, FramePreparer, GpuRenderer, HdrBloomCapture, HdrBloomFilter, HdrBloomParameters,
    HdrBloomScope, RetainedFramePreparer,
};

const SIZE: u32 = 16;
const BASE: [f64; 4] = [0.5, 0.25, 0.75, 0.5];
const OVER: [f64; 4] = [0.04, 0.75, 0.5, 0.5];
const END: [f64; 4] = [0.75, 0.04, 0.25, 0.5];

fn color(c: [f64; 4]) -> Color {
    Color::rgba(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32)
}
fn scene(path: bool, paint: bool, reverse: bool) -> (Scene, noon::Mobject, noon::Mobject) {
    let mut scene = Scene::new();
    let camera = scene.camera_frame().unwrap();
    let mut base = scene.rectangle(8.0, 8.0).unwrap();
    base.set_fill(BASE[0], BASE[1], BASE[2], BASE[3]).unwrap();
    base.disable_stroke().unwrap();
    let mut overlay = if path {
        scene
            .path(
                VectorPath::new()
                    .move_to(Vec2::new(-2.0, -4.0))
                    .line_to(Vec2::new(2.0, -4.0))
                    .line_to(Vec2::new(2.0, 4.0))
                    .line_to(Vec2::new(-2.0, 4.0))
                    .close(),
                SemanticStyle::default(),
            )
            .unwrap()
    } else {
        scene.rectangle(4.0, 8.0).unwrap()
    };
    overlay.set_translation(-2.0, 0.0).unwrap();
    overlay.disable_stroke().unwrap();
    overlay
        .set_fill(OVER[0], OVER[1], OVER[2], OVER[3])
        .unwrap();
    overlay.set_object_opacity(0.5).unwrap();
    if paint {
        overlay
            .set_fill_gradient(
                LinearGradient::new(
                    [0.0, 0.5],
                    [1.0, 0.5],
                    [
                        GradientStop::new(0.0, color(OVER)).unwrap(),
                        GradientStop::new(1.0, color(END)).unwrap(),
                    ],
                )
                .unwrap(),
                Some(1.0),
            )
            .unwrap();
    }
    if reverse {
        scene.add(&overlay).unwrap();
        scene.add(&base).unwrap();
    } else {
        scene.add(&base).unwrap();
        scene.add(&overlay).unwrap();
    }
    (scene, camera, overlay)
}
fn srgb_decode(x: f64) -> f64 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}
fn expected(paint: bool, reverse: bool, clear: [f64; 4]) -> Vec<[f64; 4]> {
    expected_emission(paint, reverse, clear, 0.0)
}
fn expected_emission(paint: bool, reverse: bool, clear: [f64; 4], strength: f64) -> Vec<[f64; 4]> {
    (0..SIZE * SIZE)
        .map(|at| {
            let x = at % SIZE;
            let mut base = BASE;
            let mut over = OVER;
            for c in 0..3 {
                base[c] = srgb_decode(BASE[c]);
                over[c] = if paint {
                    let t = (f64::from(x) + 0.5) / 8.0;
                    srgb_decode(OVER[c]) * (1.0 - t) + srgb_decode(END[c]) * t
                } else {
                    srgb_decode(OVER[c])
                };
            }
            for channel in &mut over[..3] {
                *channel *= 1.0 + strength;
            }
            over[3] *= 0.5;
            let mut layers = [(base, true), (over, x < 8)];
            if reverse {
                layers.reverse();
            }
            let mut result = clear;
            for (layer, visible) in layers {
                if visible {
                    for c in 0..3 {
                        result[c] = layer[c] * layer[3] + result[c] * (1.0 - layer[3]);
                    }
                    result[3] = layer[3] + result[3] * (1.0 - layer[3]);
                }
            }
            result
        })
        .collect()
}
fn decode(bytes: &[u8]) -> Vec<f64> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| {
            let bits = u16::from_le_bytes([c[0], c[1]]);
            let exponent = (bits >> 10) & 31;
            let mantissa = f64::from(bits & 1023);
            let value = match exponent {
                0 => mantissa * 2.0f64.powi(-24),
                31 => f64::NAN,
                e => (1.0 + mantissa / 1024.0) * 2.0f64.powi(i32::from(e) - 15),
            };
            if bits & 32768 == 0 {
                value
            } else {
                -value
            }
        })
        .collect()
}
fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_pixel = if texture.format() == wgpu::TextureFormat::Rgba16Float {
        8
    } else {
        4
    };
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
fn qualify_authored_scene(retained: bool, visible: bool, emission: f64) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("authored view adapter: {:?}", adapter.get_info());
    assert!(adapter
        .get_texture_format_features(wgpu::TextureFormat::Rgba16Float)
        .flags
        .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE));
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("authored linear view"),
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
    let mut filter = HdrBloomFilter::new(&adapter, &device);
    let mut scope = HdrBloomScope::default();
    let p = HdrBloomParameters {
        sigma_pixels: 0.65,
        threshold: 0.05,
        intensity: 1.5,
        scratch_budget_bytes: 1024 * 1024,
    };
    let artifact = std::env::var_os(if emission > 0.0 {
        "NOON_EMISSION_VIEW_ARTIFACT_DIR"
    } else if visible {
        "NOON_VISIBLE_VIEW_ARTIFACT_DIR"
    } else if retained {
        "NOON_RETAINED_VIEW_ARTIFACT_DIR"
    } else {
        "NOON_AUTHORED_VIEW_ARTIFACT_DIR"
    })
    .map(std::path::PathBuf::from);
    if let Some(dir) = &artifact {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("adapter.txt"), format!("{:?}", adapter.get_info())).unwrap();
    }
    let mut profiles = 0;
    let mut max_linear_error = 0.0f64;
    let mut max_display_error = 0;
    let mut negative_error = 0.0f64;
    let mut max_source_radiance = 0.0f64;
    let mut ignored_emission_error = 0.0f64;
    for paint in [false, true] {
        for path in [false, true] {
            for reverse in [false, true] {
                for clear in [[0.0; 4], [0.125, 0.0625, 0.03125, 1.0]] {
                    let (scene, camera, mut material) = scene(path, paint, reverse);
                    material.set_emission(emission).unwrap();
                    let mut session = scene.execution_session().unwrap();
                    if emission > 0.0 {
                        let initial = session.take_renderer_publication();
                        let mut unsupported = RetainedFramePreparer::new();
                        assert!(matches!(
                            unsupported.prepare_publication(
                                &device,
                                &initial,
                                noon_render_wgpu::text::TextDeviceMetrics::uniform(2.0).unwrap()
                            ),
                            Err(noon_render_wgpu::RetainedPrepareError::UnsupportedEmission(
                                _
                            ))
                        ));
                    }
                    let mut tx = SemanticMutationTransaction::new();
                    tx.set_view_color_profile(camera.node_id(), ViewColorProfile::LinearHdr);
                    scene.live(&mut session).apply(tx).unwrap();
                    let publication = session.take_renderer_publication();
                    assert_eq!(publication.color_profile(), ViewColorProfile::LinearHdr);
                    // The semantic profile selects a linear target; the host does not own
                    // another authoring mode or encode a scene to choose it.
                    let format = match publication.color_profile() {
                        ViewColorProfile::LinearHdr => wgpu::TextureFormat::Rgba16Float,
                        ViewColorProfile::Compatibility => wgpu::TextureFormat::Rgba8Unorm,
                    };
                    let mut renderer = GpuRenderer::new(&device, &queue, format);
                    renderer.set_viewport(&device, &queue, SIZE, SIZE);
                    renderer.set_camera(
                        &queue,
                        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 8.0)).unwrap(),
                    );
                    let candidates = (0..publication.frame().objects.len()).collect::<Vec<_>>();
                    let mut preparer = FramePreparer::new();
                    let mut retained_preparer = RetainedFramePreparer::new();
                    let mut text = renderer.create_retained_text_state(&device, &queue);
                    let metrics = noon_render_wgpu::text::TextDeviceMetrics::uniform(2.0).unwrap();
                    let clear_color = wgpu::Color {
                        r: clear[0],
                        g: clear[1],
                        b: clear[2],
                        a: clear[3],
                    };
                    let mut encoder = device.create_command_encoder(&Default::default());
                    if retained {
                        let prepared = if visible {
                            retained_preparer.prepare_publication_visible(
                                &device,
                                &publication,
                                &candidates,
                                metrics,
                            )
                        } else {
                            retained_preparer.prepare_publication(&device, &publication, metrics)
                        }
                        .unwrap();
                        assert_eq!(prepared.color_profile(), ViewColorProfile::LinearHdr);
                        renderer.upload_retained(&device, &queue, &prepared, &mut text);
                        renderer
                            .encode_retained(
                                &mut encoder,
                                &view,
                                &prepared,
                                &text,
                                clear_color,
                                None,
                            )
                            .unwrap();
                    } else {
                        let prepared = preparer.prepare_publication(&publication);
                        renderer.upload(&device, &queue, &prepared);
                        renderer.encode(&mut encoder, &view, &prepared, clear_color);
                    }
                    queue.submit([encoder.finish()]);
                    let pixels = read(&device, &queue, &target);
                    let expected = expected_emission(paint, reverse, clear, emission);
                    let neutral = self::expected(paint, reverse, clear);
                    for (a, n) in decode(&pixels).iter().zip(neutral.iter().flatten()) {
                        max_source_radiance = max_source_radiance.max(*a);
                        ignored_emission_error = ignored_emission_error.max((a - n).abs());
                    }
                    for (at, (a, e)) in decode(&pixels)
                        .iter()
                        .zip(expected.iter().flatten())
                        .enumerate()
                    {
                        let error = (a - e).abs();
                        max_linear_error = max_linear_error.max(error);
                        assert!(
                            a.is_finite()
                                && error
                                    <= if emission > 0.0 && at % 4 != 3 {
                                        0.006 + 0.004 * e.abs()
                                    } else {
                                        0.002
                                    },
                            "profile {profiles}, channel {at}: {a} != {e}"
                        );
                    }
                    filter
                        .prepare(
                            &device,
                            &queue,
                            &mut scope,
                            HdrBloomCapture {
                                source: &target,
                                revision: profiles + 1,
                            },
                            p,
                        )
                        .unwrap();
                    let mut encoder = device.create_command_encoder(&Default::default());
                    filter.encode(&mut encoder, &mut scope);
                    queue.submit([encoder.finish()]);
                    let (_, display_expected) = reference(&expected, [SIZE, SIZE], p);
                    let display = read(&device, &queue, scope.display_output().unwrap());
                    let error = display
                        .iter()
                        .zip(&display_expected)
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    max_display_error = max_display_error.max(error);
                    assert!(error <= 2, "profile {profiles} display error {error}");
                    drop(publication);
                    let clean = session.take_renderer_publication();
                    if retained {
                        let prepared = if visible {
                            retained_preparer.prepare_publication_visible(
                                &device,
                                &clean,
                                &candidates,
                                metrics,
                            )
                        } else {
                            retained_preparer.prepare_publication(&device, &clean, metrics)
                        }
                        .unwrap();
                        assert_eq!(prepared.geometry_stats().instances_repacked, 0);
                        assert_eq!(
                            renderer
                                .upload_retained(&device, &queue, &prepared, &mut text)
                                .bytes_uploaded(),
                            0
                        );
                    } else {
                        let prepared = preparer.prepare_publication(&clean);
                        assert_eq!(prepared.stats.instances_repacked, 0);
                        assert_eq!(
                            renderer.upload(&device, &queue, &prepared).bytes_uploaded,
                            0
                        );
                    }
                    if profiles == 0 {
                        // Actual draw with the ordinary raw packing, deliberately omitting
                        // the profile consumer. This must disagree with the independent
                        // authored linear-light expectation, rather than a fabricated image.
                        let wrong = preparer.prepare(clean.frame());
                        renderer.upload(&device, &queue, &wrong);
                        let mut encoder = device.create_command_encoder(&Default::default());
                        renderer.encode(&mut encoder, &view, &wrong, wgpu::Color::TRANSPARENT);
                        queue.submit([encoder.finish()]);
                        let actual = read(&device, &queue, &target);
                        negative_error = decode(&actual)
                            .iter()
                            .zip(expected.iter().flatten())
                            .map(|(a, e)| (a - e).abs())
                            .fold(0.0, f64::max);
                        assert!(negative_error > 0.05);
                        if let Some(dir) = &artifact {
                            std::fs::write(dir.join("negative-missing-decode.rgba16f"), actual)
                                .unwrap();
                        }
                    }
                    if let Some(dir) = &artifact {
                        std::fs::write(dir.join(format!("case-{profiles}.json")),format!("{{\"paint\":{paint},\"path\":{path},\"reverse\":{reverse},\"clear\":{clear:?},\"emission\":{emission}}}")).unwrap();
                        std::fs::write(dir.join(format!("case-{profiles}.rgba16f")), pixels)
                            .unwrap();
                        std::fs::write(
                            dir.join(format!("case-{profiles}.expected-rgba64f")),
                            expected
                                .iter()
                                .flatten()
                                .flat_map(|v| v.to_le_bytes())
                                .collect::<Vec<_>>(),
                        )
                        .unwrap();
                        std::fs::write(dir.join(format!("case-{profiles}.rgba")), display).unwrap();
                        std::fs::write(
                            dir.join(format!("case-{profiles}.expected-rgba")),
                            display_expected,
                        )
                        .unwrap();
                    }
                    profiles += 1;
                }
            }
        }
    }
    if emission > 0.0 {
        assert!(
            max_source_radiance > 1.0,
            "authored emission must survive above one before Bloom"
        );
        assert!(
            ignored_emission_error > 0.5,
            "a missing material publication must fail this corpus"
        );
    }
    eprintln!("emission={emission}, maximum pre-Bloom radiance={max_source_radiance}, ignored-material difference={ignored_emission_error}");
    let manifest=format!("{{\"profiles\":{profiles},\"width\":{SIZE},\"height\":{SIZE},\"linear_channels\":{},\"display_channels\":{},\"max_linear_error\":{max_linear_error},\"rgb_absolute_bound\":{},\"rgb_relative_bound\":{},\"alpha_absolute_bound\":0.002,\"emission_strength\":{emission},\"max_display_error\":{max_display_error},\"display_bound\":2,\"missing_decode_negative_error\":{negative_error}}}",profiles*SIZE as u64*SIZE as u64*4,profiles*SIZE as u64*SIZE as u64*4,if emission > 0.0 {0.006} else {0.002},if emission > 0.0 {0.004} else {0.0});
    if let Some(dir) = &artifact {
        std::fs::write(dir.join("manifest.json"), &manifest).unwrap();
    }
    eprintln!("authored view: {manifest}");
}

#[test]
#[ignore = "requires a real native float/blending adapter; absence fails qualification"]
fn authored_view_profile_decodes_before_blending_and_matches_independent_bloom_pixels() {
    qualify_authored_scene(false, false, 0.0);
}

#[test]
#[ignore = "requires a real native float/blending adapter; absence fails qualification"]
fn retained_view_profile_decodes_before_blending_and_matches_independent_bloom_pixels() {
    qualify_authored_scene(true, false, 0.0);
}

mod retained_color_assets;

#[test]
#[ignore = "requires a real native float/blending adapter; absence fails qualification"]
fn visible_retained_view_profile_matches_independent_bloom_pixels() {
    qualify_authored_scene(true, true, 0.0);
}

mod hdr_captures;

mod view_bloom;

#[test]
#[ignore = "requires a real native float/blending adapter; absence fails qualification"]
fn authored_emission_survives_above_one_before_bloom_with_unchanged_alpha() {
    qualify_authored_scene(true, false, 7.0);
}

#[test]
#[ignore = "requires a real native float/blending adapter; absence fails qualification"]
fn authored_maximum_emission_uses_the_hdr_envelope_without_changing_alpha() {
    qualify_authored_scene(true, false, 63.0);
}
