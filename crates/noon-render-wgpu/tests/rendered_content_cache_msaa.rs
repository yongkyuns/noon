//! #2094 M0: real-backend storage/restore operator probe, NOT production caching.
//! Explicit synthetic coverage is not a Noon geometry, semantic, host, lifetime,
//! or performance oracle. No production shader, API, admission or quality changes.
//! Run the ignored test explicitly; missing adapter/capability is a FAILURE.
use std::{fs, path::Path, time::Duration};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 8;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const SOURCE: &str = include_str!("rendered_content_cache_msaa.wgsl");

struct Probe {
    device: wgpu::Device,
    queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
}

impl Probe {
    fn texture(&self, width: u32, samples: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("#2094 M0 probe target"),
            size: wgpu::Extent3d {
                width,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | if samples == 1 {
                    wgpu::TextureUsages::COPY_SRC
                } else {
                    wgpu::TextureUsages::empty()
                },
            view_formats: &[],
        })
    }

    fn pipeline(&self, entry: &str, samples: u32, blend: bool) -> wgpu::RenderPipeline {
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: samples,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: blend.then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
    }

    fn binding(&self, pipeline: &wgpu::RenderPipeline, source: &wgpu::Texture) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("#2094 stored samples"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &source.create_view(&Default::default()),
                ),
            }],
        })
    }

    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        resolve: Option<&wgpu::Texture>,
        load: wgpu::LoadOp<wgpu::Color>,
        draws: &[(&wgpu::RenderPipeline, Option<&wgpu::BindGroup>)],
    ) {
        let view = target.create_view(&Default::default());
        let resolve_view = resolve.map(|texture| texture.create_view(&Default::default()));
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: resolve_view.as_ref(),
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("#2094 M0 operator pass"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        for (pipeline, group) in draws {
            pass.set_pipeline(pipeline);
            if let Some(group) = group {
                pass.set_bind_group(0, *group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
    }

    fn render(
        &self,
        target: &wgpu::Texture,
        resolve: Option<&wgpu::Texture>,
        load: wgpu::LoadOp<wgpu::Color>,
        draws: &[(&wgpu::RenderPipeline, Option<&wgpu::BindGroup>)],
    ) {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.encode(&mut encoder, target, resolve, load, draws);
        self.queue.submit([encoder.finish()]);
    }

    fn samples(&self, source: &wgpu::Texture, extract: &wgpu::RenderPipeline) -> Vec<u8> {
        let target = self.texture(WIDTH * 4, 1);
        let binding = self.binding(extract, source);
        self.render(
            &target,
            None,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            &[(extract, Some(&binding))],
        );
        self.readback(&target)
    }

    fn readback(&self, texture: &wgpu::Texture) -> Vec<u8> {
        let row = texture.width() * 4;
        let stride =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("#2094 test-only readback"),
            size: u64::from(stride) * u64::from(HEIGHT),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
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
                    rows_per_image: Some(HEIGHT),
                },
            },
            texture.size(),
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        let result = mapped
            .chunks_exact(stride as usize)
            .flat_map(|bytes| bytes[..row as usize].iter().copied())
            .collect();
        drop(mapped);
        buffer.unmap();
        result
    }
}

// All bytes, including alpha, participate. Save both arrays and a differing-pixel
// mask before asserting so failures retain the evidence. No average-error gate.
fn compare(dir: &Path, name: &str, expected: &[u8], actual: &[u8], equal: bool) {
    assert_eq!(expected.len(), actual.len());
    fs::write(dir.join(format!("{name}-expected.rgba")), expected).unwrap();
    fs::write(dir.join(format!("{name}-actual.rgba")), actual).unwrap();
    let mask: Vec<u8> = expected
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.as_chunks::<4>().0)
        .map(|(a, b)| if a == b { 0 } else { 255 })
        .collect();
    fs::write(dir.join(format!("{name}-diff.mask")), &mask).unwrap();
    let mut max_rgb = 0;
    let mut max_alpha = 0;
    for (i, (&a, &b)) in expected.iter().zip(actual).enumerate() {
        if i % 4 == 3 {
            max_alpha = max_alpha.max(a.abs_diff(b));
        } else {
            max_rgb = max_rgb.max(a.abs_diff(b));
        }
    }
    let differing = mask.iter().filter(|&&v| v != 0).count();
    eprintln!("{name}: differing_pixels={differing} max_rgb={max_rgb} max_alpha={max_alpha}");
    assert_eq!(
        differing == 0,
        equal,
        "{name}: exact comparison / negative control"
    );
}

#[test]
#[ignore = "real raster adapter required; explicitly invoked by native raster qualification"]
fn rendered_content_cache_msaa_operator() {
    let dir = std::env::var_os("NOON_CACHE_PROBE_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "target/cache-msaa-probe".into());
    fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("PASS.txt");
    if marker.exists() {
        fs::remove_file(&marker).unwrap();
    }
    let backends = match std::env::var("NOON_CACHE_PROBE_BACKEND").as_deref() {
        Ok("vulkan") | Err(std::env::VarError::NotPresent) => wgpu::Backends::VULKAN,
        Ok("metal") => wgpu::Backends::METAL,
        Ok("dx12") => wgpu::Backends::DX12,
        Ok("gl") => wgpu::Backends::GL,
        other => panic!("unsupported NOON_CACHE_PROBE_BACKEND: {other:?}"),
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("real adapter required; absence is not a passing skip");
    let info = adapter.get_info();
    assert_ne!(info.backend, wgpu::Backend::Noop);
    let downlevel = adapter.get_downlevel_capabilities();
    let features = adapter.get_texture_format_features(FORMAT);
    let provenance = format!(
        "#2094 M0 operator-only probe\nadapter={info:?}\ndownlevel={downlevel:?}\nformat={FORMAT:?} features={features:?}\nwidth={WIDTH} height={HEIGHT} samples=4\nsample-dump width=1024; rgba=unorm8; mask=one byte per pixel\nNOT production geometry, cache admission/lifetime, cross-host, or performance qualification\n"
    );
    eprintln!("{provenance}");
    fs::write(dir.join("profile.txt"), provenance).unwrap();
    assert!(downlevel
        .flags
        .contains(wgpu::DownlevelFlags::MULTISAMPLED_SHADING));
    assert!(features.allowed_usages.contains(
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
    ));
    assert!(features.flags.contains(
        wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4
            | wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE
            | wgpu::TextureFormatFeatureFlags::BLENDABLE
    ));
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("#2094 M0 synthetic operator"),
        source: wgpu::ShaderSource::Wgsl(SOURCE.into()),
    });
    let probe = Probe {
        device,
        queue,
        shader,
    };
    let seed = probe.pipeline("seed", 4, false);
    let prefix = probe.pipeline("prefix", 4, true);
    let suffix = probe.pipeline("suffix", 4, true);
    let other = probe.pipeline("other_suffix", 4, true);
    let restore = probe.pipeline("restore", 4, false);
    let permuted = probe.pipeline("permuted", 4, false);
    let averaged = probe.pipeline("averaged", 4, false);
    let extract = probe.pipeline("extract", 1, false);
    let clear = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
    let mut stored = probe.texture(WIDTH, 4);
    let mut work = probe.texture(WIDTH, 4);
    let baseline = probe.texture(WIDTH, 4);
    let reference_image = probe.texture(WIDTH, 1);
    let candidate_image = probe.texture(WIDTH, 1);

    // Check the extraction path independently against integer codes, then check
    // eight successive sample-preserving round trips for drift.
    probe.render(&stored, None, clear, &[(&seed, None)]);
    let mut codes = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            for s in 0..4 {
                let a = (x + s * 37 + y * 13) & 255;
                codes.extend([a as u8, (x & a) as u8, ((s * 17) % (a + 1)) as u8, a as u8]);
            }
        }
    }
    compare(
        &dir,
        "seed-samples",
        &codes,
        &probe.samples(&stored, &extract),
        true,
    );
    for round in 0..8 {
        let binding = probe.binding(&restore, &stored);
        probe.render(&work, None, clear, &[(&restore, Some(&binding))]);
        compare(
            &dir,
            &format!("round-{round}-samples"),
            &codes,
            &probe.samples(&work, &extract),
            true,
        );
        std::mem::swap(&mut stored, &mut work);
    }

    for (index, backdrop) in [
        wgpu::Color::TRANSPARENT,
        wgpu::Color::BLACK,
        wgpu::Color::WHITE,
        wgpu::Color {
            r: 0.125,
            g: 0.25,
            b: 0.375,
            a: 0.5,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let load = wgpu::LoadOp::Clear(backdrop);
        probe.render(&stored, None, load, &[(&prefix, None)]);
        let stored_samples = probe.samples(&stored, &extract);
        let binding = probe.binding(&restore, &stored);
        // Source is built once. Draw changing suffixes after exact restoration;
        // the ordinary reference rebuilds prefix + suffix in one pass each time.
        for (step, foreground) in [&suffix, &other, &suffix, &suffix].into_iter().enumerate() {
            probe.render(
                &baseline,
                Some(&reference_image),
                load,
                &[(&prefix, None), (foreground, None)],
            );
            probe.render(
                &work,
                Some(&candidate_image),
                clear,
                &[(&restore, Some(&binding)), (foreground, None)],
            );
            compare(
                &dir,
                &format!("b{index}-s{step}-resolved"),
                &probe.readback(&reference_image),
                &probe.readback(&candidate_image),
                true,
            );
            compare(
                &dir,
                &format!("b{index}-s{step}-samples"),
                &probe.samples(&baseline, &extract),
                &probe.samples(&work, &extract),
                true,
            );
        }
        // An encoded but abandoned write must not alter the previously submitted
        // source. This probes queue behavior, NOT production cache installation.
        let mut abandoned = probe.device.create_command_encoder(&Default::default());
        probe.encode(&mut abandoned, &stored, None, clear, &[(&seed, None)]);
        drop(abandoned);
        compare(
            &dir,
            &format!("b{index}-abandoned-samples"),
            &stored_samples,
            &probe.samples(&stored, &extract),
            true,
        );

        let ordinary = probe.readback(&reference_image);
        // Each deliberate defect must be detected at resolved output, too.
        for (name, broken) in [("permuted", &permuted), ("averaged", &averaged)] {
            let broken_binding = probe.binding(broken, &stored);
            probe.render(
                &work,
                Some(&candidate_image),
                clear,
                &[(broken, Some(&broken_binding)), (&suffix, None)],
            );
            compare(
                &dir,
                &format!("b{index}-{name}"),
                &ordinary,
                &probe.readback(&candidate_image),
                false,
            );
        }
        probe.render(
            &work,
            Some(&candidate_image),
            clear,
            &[(&restore, Some(&binding)), (&suffix, None)],
        );
        probe.render(
            &work,
            Some(&candidate_image),
            wgpu::LoadOp::Load,
            &[(&suffix, None)],
        );
        compare(
            &dir,
            &format!("b{index}-accumulated"),
            &ordinary,
            &probe.readback(&candidate_image),
            false,
        );
    }
    fs::write(
        marker,
        "CACHE_MSAA_OPERATOR_PASS: RGBA8Unorm 4x, exact samples and resolved RGBA; three seeded defects rejected. Operator only.\n",
    )
    .unwrap();
    eprintln!("CACHE_MSAA_OPERATOR_PASS");
}
