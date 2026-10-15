//! #2094 M0: real geometry/painter draws around a sample-preserving prefix.
//! Test-only capture/restore plumbing; NOT automatic cache admission or lifetime.
//! The ordinary `GpuRenderer::encode` is the resolved-image reference. A stored
//! unsplit pass uses `draw_ordered` for per-sample inspection; first prove that
//! its resolved pixels match ordinary encode, including its discard behavior.
//! No alternate geometry shaders or changes to production compositing.
use std::{fs, ops::Range, path::Path, time::Duration};

use crate::gpu::{Camera2D, DrawStats, GpuRenderer, ResolvedOrderedBatch, PATH_SAMPLE_COUNT};
use crate::{FramePreparer, PreparedFrame, RenderPrimitive};
use noon_core::{
    Color, GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, Vec2, VectorPath,
};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/rendered_content_cache_msaa.wgsl"
));
const PREFIX: usize = 2;

struct Probe {
    device: wgpu::Device,
    queue: wgpu::Queue,
    restore: wgpu::RenderPipeline,
    extract: wgpu::RenderPipeline,
}

// Keep the encoder/pass borrow local instead of erasing its lifetime. This
// macro only owns test command encoding; it does not wrap any geometry logic.
macro_rules! render_pass {
    ($probe:expr, $target:expr, $resolve:expr, $load:expr, |$pass:ident| $body:block $(,)?) => {{
        let probe = $probe;
        let target = $target;
        let resolved: Option<&wgpu::Texture> = $resolve;
        let view = target.create_view(&Default::default());
        let resolved = resolved.map(|texture| texture.create_view(&Default::default()));
        let mut encoder = probe.device.create_command_encoder(&Default::default());
        {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: resolved.as_ref(),
                ops: wgpu::Operations {
                    load: $load,
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("#2094 test-only retained-prefix pass"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let $pass = &mut render_pass;
            $body
        }
        probe.queue.submit([encoder.finish()]);
    }};
}

impl Probe {
    fn new(directory: &Path) -> Self {
        let backend = match std::env::var("NOON_CACHE_PROBE_BACKEND").as_deref() {
            Ok("vulkan") | Err(std::env::VarError::NotPresent) => wgpu::Backends::VULKAN,
            Ok("metal") => wgpu::Backends::METAL,
            Ok("dx12") => wgpu::Backends::DX12,
            other => panic!("unqualified geometry-probe backend: {other:?}"),
        };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: backend,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("raster adapter required, not a passing skip");
        let info = adapter.get_info();
        assert_ne!(info.backend, wgpu::Backend::Noop);
        assert!(adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::MULTISAMPLED_SHADING));
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("#2094 reuse qualified sample restore/extraction shader"),
            source: wgpu::ShaderSource::Wgsl(SOURCE.into()),
        });
        let pipeline = |entry, samples| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let restore = pipeline("restore", PATH_SAMPLE_COUNT);
        let extract = pipeline("extract", 1);
        let provenance = format!(
            "#2094 production geometry draw probe\nadapter={info:?}\nformat={FORMAT:?} samples=4 transfer=Direct\nprofiles=96x64,96x64,127x81,127x81; sample dump width=4*width\nNOT image/text, public Scene/group semantics, automatic admission/invalidation, host lifetime, browser or performance qualification\n"
        );
        fs::write(directory.join("profile.txt"), provenance).unwrap();
        Self {
            device,
            queue,
            restore,
            extract,
        }
    }

    fn target(&self, size: [u32; 2], samples: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("#2094 production geometry probe target"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
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

    fn binding(&self, pipeline: &wgpu::RenderPipeline, source: &wgpu::Texture) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("#2094 stored geometry samples"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &source.create_view(&Default::default()),
                ),
            }],
        })
    }

    fn readback(&self, texture: &wgpu::Texture) -> Vec<u8> {
        let row = texture.width() * 4;
        let stride =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("#2094 test-only geometry readback"),
            size: u64::from(stride) * u64::from(texture.height()),
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
                    rows_per_image: Some(texture.height()),
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
            .flat_map(|row_bytes| row_bytes[..row as usize].iter().copied())
            .collect();
        drop(mapped);
        buffer.unmap();
        result
    }

    fn samples(&self, source: &wgpu::Texture) -> Vec<u8> {
        let output = self.target([source.width() * 4, source.height()], 1);
        let group = self.binding(&self.extract, source);
        render_pass!(
            self,
            &output,
            None,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            |pass| {
                pass.set_pipeline(&self.extract);
                pass.set_bind_group(0, &group, &[]);
                pass.draw(0..3, 0..1);
            }
        );
        self.readback(&output)
    }
}

fn draw_range<'a>(
    renderer: &'a GpuRenderer,
    pass: &mut wgpu::RenderPass<'a>,
    prepared: &PreparedFrame<'_>,
    range: Range<usize>,
) -> DrawStats {
    // Restore uses bind group zero too. Rebind the real camera and reset the
    // batching state before ordinary draws; never reuse stale pipeline bindings.
    pass.set_bind_group(0, &renderer.camera_bind_group, &[]);
    let mut binding = None;
    let mut stats = DrawStats::default();
    for item in prepared
        .ordered_render_batches()
        .skip(range.start)
        .take(range.len())
    {
        stats += renderer.draw_resolved_ordered_batch(
            pass,
            prepared,
            &ResolvedOrderedBatch {
                batch: item.batch.clone(),
                mega: item.mega_path_batch.cloned(),
            },
            false,
            &mut binding,
        );
    }
    stats
}

fn frame() -> FrameState {
    let lower = VectorPath::new()
        .move_to(Vec2::new(-1.6, -0.8))
        .line_to(Vec2::new(1.3, -0.7))
        .quadratic_to(Vec2::new(-0.9, 1.6), Vec2::new(-1.6, -0.8))
        .close();
    let upper = VectorPath::new()
        .move_to(Vec2::new(-1.1, -0.35))
        .quadratic_to(Vec2::new(0.0, 1.2), Vec2::new(1.4, -0.45));
    let reveal = VectorPath::new()
        .move_to(Vec2::new(-1.3, 0.55))
        .line_to(Vec2::new(1.1, -0.6));
    let geometry = [
        GeometryRef::path(lower),
        GeometryRef::circle(0.65),
        GeometryRef::rectangle(1.4, 1.0),
        GeometryRef::path(upper),
        GeometryRef::path(reveal),
    ];
    let colors = [
        Color::rgba(1.0, 0.0, 0.0, 1.0),
        Color::rgba(0.0, 1.0, 0.0, 1.0),
        Color::rgba(0.0, 0.0, 1.0, 1.0),
        Color::WHITE,
        Color::rgba(1.0, 0.0, 0.0, 1.0),
    ];
    let objects = geometry
        .into_iter()
        .zip(colors)
        .enumerate()
        .map(|(i, (geometry, color))| FrameObjectState {
            glow: None,
            spatial: None,
            id: ObjectId::new(i as u64 + 1),
            z_index: 0.0,
            content: ObjectContentRef::Geometry(geometry),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style {
                fill: (i < 3).then_some(color),
                stroke: Some(color),
                stroke_width: if i == 3 { 0.055 } else { 0.019 },
                opacity: 0.65,
                ..Style::default()
            },
            appearance: 1.0,
        })
        .collect();
    FrameState {
        time: 0.0,
        objects,
        presences: vec![true; 5],
        reveals: vec![1.0; 5],
        morphs: vec![0.0; 5],
        render_geometries: vec![None; 5],
        render_transforms: vec![None; 5],
        family_animations: vec![],
        family_animation_plan_indices: vec![],
    }
}

fn fresh_pixels(
    probe: &Probe,
    frame: &FrameState,
    size: [u32; 2],
    center: Vec2,
    clear: wgpu::Color,
) -> Vec<u8> {
    let mut renderer = GpuRenderer::new(&probe.device, &probe.queue, FORMAT);
    renderer.set_viewport(&probe.device, &probe.queue, size[0], size[1]);
    renderer.set_camera(
        &probe.queue,
        Camera2D::new(center, Vec2::new(4.0, 3.0)).unwrap(),
    );
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(frame);
    renderer.upload(&probe.device, &probe.queue, &prepared);
    let output = probe.target(size, 1);
    let mut encoder = probe.device.create_command_encoder(&Default::default());
    renderer.encode(
        &mut encoder,
        &output.create_view(&Default::default()),
        &prepared,
        clear,
    );
    probe.queue.submit([encoder.finish()]);
    probe.readback(&output)
}

fn compare(directory: &Path, name: &str, expected: &[u8], actual: &[u8], equal: bool) {
    assert_eq!(expected.len(), actual.len());
    assert!(expected.len().is_multiple_of(4));
    fs::write(directory.join(format!("{name}-expected.rgba")), expected).unwrap();
    fs::write(directory.join(format!("{name}-actual.rgba")), actual).unwrap();
    let mask: Vec<u8> = expected
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.as_chunks::<4>().0)
        .map(|(a, b)| if a == b { 0 } else { 255 })
        .collect();
    fs::write(directory.join(format!("{name}-diff.mask")), &mask).unwrap();
    let differing = mask.iter().filter(|&&v| v != 0).count();
    let mut max_rgb = 0;
    let mut max_alpha = 0;
    for (index, (&a, &b)) in expected.iter().zip(actual).enumerate() {
        if index % 4 == 3 {
            max_alpha = max_alpha.max(a.abs_diff(b));
        } else {
            max_rgb = max_rgb.max(a.abs_diff(b));
        }
    }
    eprintln!("{name}: differing_pixels={differing} max_rgb={max_rgb} max_alpha={max_alpha}");
    assert_eq!(
        differing == 0,
        equal,
        "{name}: exact comparison or seeded defect"
    );
}

#[test]
#[ignore = "explicit native raster qualification; missing adapter is a failure"]
fn ordered_prefix_sample_restore() {
    let directory = std::env::var_os("NOON_CACHE_PROBE_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .expect("qualification requires a fresh artifact directory");
    fs::create_dir_all(&directory).unwrap();
    let marker = directory.join("PASS.txt");
    if marker.exists() {
        fs::remove_file(&marker).unwrap();
    }
    let probe = Probe::new(&directory);
    for profile in 0..4 {
        let size = if profile < 2 { [96, 64] } else { [127, 81] };
        let center = if profile < 2 {
            Vec2::ZERO
        } else {
            Vec2::new(0.013, -0.027)
        };
        let clear = if profile & 1 == 0 {
            wgpu::Color::TRANSPARENT
        } else {
            wgpu::Color {
                r: 0.125,
                g: 0.25,
                b: 0.375,
                a: 0.5,
            }
        };
        let mut renderer = GpuRenderer::new(&probe.device, &probe.queue, FORMAT);
        renderer.set_viewport(&probe.device, &probe.queue, size[0], size[1]);
        renderer.set_camera(
            &probe.queue,
            Camera2D::new(center, Vec2::new(4.0, 3.0)).unwrap(),
        );
        let mut frame = frame();
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);
        assert_eq!(renderer.scene_render_sample_count(&prepared, None), 4);
        let order: Vec<_> = prepared
            .ordered_render_batches()
            .map(|r| r.batch.clone())
            .collect();
        assert!(order.len() > PREFIX + 1);
        assert!(matches!(
            order[0].primitive,
            RenderPrimitive::Path { .. } | RenderPrimitive::MegaPath { .. }
        ));
        assert_eq!(order[1].primitive, RenderPrimitive::Circle);
        assert_eq!(order[2].primitive, RenderPrimitive::Rectangle);
        renderer.upload(&probe.device, &probe.queue, &prepared);
        let prefix = probe.target(size, 4);
        let unsplit = probe.target(size, 4);
        let work = probe.target(size, 4);
        let ordinary = probe.target(size, 1);
        let reference = probe.target(size, 1);
        let candidate = probe.target(size, 1);
        render_pass!(&probe, &prefix, None, wgpu::LoadOp::Clear(clear), |pass| {
            assert!(draw_range(&renderer, pass, &prepared, 0..PREFIX).draw_calls > 0);
        });
        let restore = probe.binding(&probe.restore, &prefix);
        let mut first = None;
        for (step, offset) in [0.0, 0.37, 0.0, 0.0].into_iter().enumerate() {
            frame.objects[2].transform.translation = Vec2::new(offset, -offset * 0.7);
            frame.reveals[4] = if step == 1 { 0.4 } else { 1.0 };
            let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![2, 4]));
            let current: Vec<_> = prepared
                .ordered_render_batches()
                .map(|r| r.batch.clone())
                .collect();
            assert_eq!(
                &current[..PREFIX],
                &order[..PREFIX],
                "controlled prefix unchanged"
            );
            let total = current.len();
            renderer.upload(&probe.device, &probe.queue, &prepared);
            let mut encoder = probe.device.create_command_encoder(&Default::default());
            let full = renderer.encode(
                &mut encoder,
                &ordinary.create_view(&Default::default()),
                &prepared,
                clear,
            );
            probe.queue.submit([encoder.finish()]);
            render_pass!(
                &probe,
                &unsplit,
                Some(&reference),
                wgpu::LoadOp::Clear(clear),
                |pass| {
                    renderer.draw_ordered(pass, &prepared, false);
                }
            );
            render_pass!(
                &probe,
                &work,
                Some(&candidate),
                wgpu::LoadOp::Clear(clear),
                |pass| {
                    pass.set_pipeline(&probe.restore);
                    pass.set_bind_group(0, &restore, &[]);
                    pass.draw(0..3, 0..1);
                    let tail = draw_range(&renderer, pass, &prepared, PREFIX..total);
                    assert!(tail.draw_calls > 0 && tail.instances_drawn < full.instances_drawn);
                }
            );
            let pixels = probe.readback(&ordinary);
            assert!(
                pixels.as_chunks::<4>().0.windows(2).any(|p| p[0] != p[1]),
                "nonblank geometry witness"
            );
            compare(
                &directory,
                &format!("p{profile}-s{step}-ordinary"),
                &pixels,
                &probe.readback(&reference),
                true,
            );
            compare(
                &directory,
                &format!("p{profile}-s{step}-resolved"),
                &pixels,
                &probe.readback(&candidate),
                true,
            );
            compare(
                &directory,
                &format!("p{profile}-s{step}-fresh"),
                &pixels,
                &fresh_pixels(&probe, &frame, size, center, clear),
                true,
            );
            let samples = probe.samples(&work);
            assert!(
                samples.as_chunks::<16>().0.iter().any(|pixel| {
                    pixel[..4] != pixel[4..8]
                        || pixel[..4] != pixel[8..12]
                        || pixel[..4] != pixel[12..16]
                }),
                "real geometry must exercise unequal multisample coverage"
            );
            compare(
                &directory,
                &format!("p{profile}-s{step}-samples"),
                &probe.samples(&unsplit),
                &samples,
                true,
            );
            if step == 0 {
                first = Some((pixels.clone(), samples.clone()));
            }
            if step >= 2 {
                assert_eq!(
                    first.as_ref().unwrap(),
                    &(pixels, samples),
                    "history independence"
                );
            }
        }
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::default());
        let total = prepared.ordered_render_batches().count();
        let pixels = probe.readback(&ordinary);
        render_pass!(
            &probe,
            &work,
            Some(&candidate),
            wgpu::LoadOp::Load,
            |pass| {
                draw_range(&renderer, pass, &prepared, PREFIX..total);
            }
        );
        compare(
            &directory,
            &format!("p{profile}-accumulated"),
            &pixels,
            &probe.readback(&candidate),
            false,
        );
        render_pass!(
            &probe,
            &work,
            Some(&candidate),
            wgpu::LoadOp::Clear(clear),
            |pass| {
                pass.set_pipeline(&probe.restore);
                pass.set_bind_group(0, &restore, &[]);
                pass.draw(0..3, 0..1);
                // Forbidden gather: move later static geometry below the active layer.
                draw_range(&renderer, pass, &prepared, PREFIX + 1..total);
                draw_range(&renderer, pass, &prepared, PREFIX..PREFIX + 1);
            }
        );
        compare(
            &directory,
            &format!("p{profile}-reordered"),
            &pixels,
            &probe.readback(&candidate),
            false,
        );
        frame.objects[0].style.opacity = 0.15;
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        let total = prepared.ordered_render_batches().count();
        renderer.upload(&probe.device, &probe.queue, &prepared);
        let mut encoder = probe.device.create_command_encoder(&Default::default());
        renderer.encode(
            &mut encoder,
            &ordinary.create_view(&Default::default()),
            &prepared,
            clear,
        );
        probe.queue.submit([encoder.finish()]);
        let changed = probe.readback(&ordinary);
        render_pass!(
            &probe,
            &work,
            Some(&candidate),
            wgpu::LoadOp::Clear(clear),
            |pass| {
                pass.set_pipeline(&probe.restore);
                pass.set_bind_group(0, &restore, &[]);
                pass.draw(0..3, 0..1);
                draw_range(&renderer, pass, &prepared, PREFIX..total);
            }
        );
        compare(
            &directory,
            &format!("p{profile}-stale-prefix"),
            &changed,
            &probe.readback(&candidate),
            false,
        );
        // Explicit test rebuild, NOT an implementation of automatic invalidation.
        render_pass!(&probe, &prefix, None, wgpu::LoadOp::Clear(clear), |pass| {
            draw_range(&renderer, pass, &prepared, 0..PREFIX);
        });
        render_pass!(
            &probe,
            &work,
            Some(&candidate),
            wgpu::LoadOp::Clear(clear),
            |pass| {
                pass.set_pipeline(&probe.restore);
                pass.set_bind_group(0, &restore, &[]);
                pass.draw(0..3, 0..1);
                draw_range(&renderer, pass, &prepared, PREFIX..total);
            }
        );
        compare(
            &directory,
            &format!("p{profile}-rebuilt-resolved"),
            &changed,
            &probe.readback(&candidate),
            true,
        );
        render_pass!(&probe, &unsplit, None, wgpu::LoadOp::Clear(clear), |pass| {
            renderer.draw_ordered(pass, &prepared, false);
        });
        compare(
            &directory,
            &format!("p{profile}-rebuilt-samples"),
            &probe.samples(&unsplit),
            &probe.samples(&work),
            true,
        );
    }
    fs::write(
        marker,
        "CACHE_GEOMETRY_PROBE_PASS: ordinary production geometry and sample restoration; test-only prefix selection\n",
    )
    .unwrap();
    eprintln!("CACHE_GEOMETRY_PROBE_PASS");
}

mod mixed;
mod mixed_images;
