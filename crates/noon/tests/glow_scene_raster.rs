//! Public authoring -> Scene execution -> ordinary retained wgpu rendering.
//! The independent filter reference uses only a separate no-effect Scene image.
use noon::{
    effects::{EffectUpdate, Pixels},
    AnimationOptions, Color, ExecutionSession, Mobject, RateFunction, Scene,
};
use noon_core::Vec2;
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState,
};
use std::time::Duration;

const SIZE: [u32; 2] = [80, 60];
const START: f64 = 0.4000000000000123;

mod composed;

fn scene(effect: bool) -> (Scene, Mobject, noon::DeclaredAnimation) {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    source.disable_stroke().unwrap();
    source.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    if effect {
        source
            .set_glow(
                EffectUpdate::default()
                    .color(Color::RED)
                    .radius(Pixels(3.25))
                    .intensity(START),
            )
            .unwrap();
    }
    scene.add(&source).unwrap();
    let mut target = source.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    if effect {
        target
            .set_glow(
                EffectUpdate::default()
                    .color(Color::BLUE)
                    .radius(Pixels(6.5))
                    .intensity(1.4),
            )
            .unwrap();
    }
    let animation = scene
        .declare_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    (scene, source, animation)
}

struct Surface {
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    target: wgpu::Texture,
    effect_stats: noon_render_wgpu::RetainedEffectStats,
    program_stats: noon_render_wgpu::EffectProgramStats,
    upload_stats: noon_render_wgpu::RetainedUploadStats,
    capture_budget: u64,
}
impl Surface {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::with_logical_viewport(device, queue, Vec2::new(80.0, 60.0))
    }
    fn with_logical_viewport(device: &wgpu::Device, queue: &wgpu::Queue, logical: Vec2) -> Self {
        Self::with_target_size(device, queue, logical, SIZE)
    }
    fn with_target_size(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        logical: Vec2,
        size: [u32; 2],
    ) -> Self {
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer
            .set_viewport_with_logical_size(device, queue, size[0], size[1], logical)
            .unwrap();
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(6.0 * logical.x / logical.y, 6.0)).unwrap(),
        );
        let text = renderer.create_retained_text_state(device, queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("public glow Scene qualification"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Self {
            renderer,
            preparer: RetainedFramePreparer::new(),
            text,
            target,
            effect_stats: Default::default(),
            program_stats: Default::default(),
            upload_stats: Default::default(),
            capture_budget: 1_000_000,
        }
    }
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        session: &mut ExecutionSession,
    ) -> Vec<u8> {
        self.render_on(device, queue, session, wgpu::Color::BLACK)
    }
    fn render_on(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        session: &mut ExecutionSession,
        background: wgpu::Color,
    ) -> Vec<u8> {
        let publication = session.take_renderer_publication();
        self.render_publication(device, queue, publication, background)
    }
    fn render_publication(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        publication: noon_runtime::RendererPublication<'_>,
        background: wgpu::Color,
    ) -> Vec<u8> {
        self.program_stats =
            pollster::block_on(self.renderer.prepare_effect_programs(device, &publication))
                .unwrap();
        let visible = publication
            .painter_order()
            .iter()
            .map(|&i| i as usize)
            .collect::<Vec<_>>();
        let camera = self.renderer.camera();
        let density = Vec2::new(
            self.target.width() as f32 / camera.world_size.x,
            self.target.height() as f32 / camera.world_size.y,
        );
        let metrics = TextDeviceMetrics::new(density)
            .unwrap()
            .with_world_origin_pixels(Vec2::new(
                self.target.width() as f32 * 0.5 - camera.center.x * density.x,
                self.target.height() as f32 * 0.5 + camera.center.y * density.y,
            ))
            .unwrap();
        let visible = self
            .renderer
            .effect_source_visibility(&publication, &visible)
            .unwrap();
        let prepared = self
            .preparer
            .prepare_planned_publication_visible(device, &publication, visible, metrics)
            .unwrap();
        self.upload_stats = self
            .renderer
            .upload_retained(device, queue, &prepared, &mut self.text);
        let mut encoder = device.create_command_encoder(&Default::default());
        self.effect_stats = self
            .renderer
            .prepare_retained_effects(
                device,
                queue,
                &mut encoder,
                &prepared,
                &self.text,
                &publication,
                background,
                self.capture_budget,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "glow prepare {error:?}: time={} content={:?} reveal={}",
                    publication.frame().time,
                    publication.frame().objects[0].content,
                    publication.frame().reveal(0)
                )
            });
        self.renderer
            .encode_retained(
                &mut encoder,
                &self.target.create_view(&Default::default()),
                &prepared,
                &self.text,
                background,
                None,
            )
            .unwrap();
        queue.submit([encoder.finish()]);
        readback(device, queue, &self.target)
    }
}

fn readback(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let row = texture.width() * 4;
    let stride =
        row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(stride) * u64::from(texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
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
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().unwrap();
    let result = bytes
        .chunks_exact(stride as usize)
        .flat_map(|row_bytes| row_bytes[..row as usize].iter().copied())
        .collect();
    drop(bytes);
    buffer.unmap();
    result
}

// Full two-dimensional square-support Gaussian, independently normalized. Input
// is a white no-effect source on black; its RGB is painted coverage, not a glow
// intermediate. These central sources never cross the viewport boundary.
fn expected(ordinary: &[u8], time: f64) -> Vec<u8> {
    // RED/BLUE are the authored Manim palette constants, not unit RGB primaries.
    // Interpolate their endpoint components independently from runtime output.
    let from = Color::RED;
    let to = Color::BLUE;
    let tint = [
        f64::from(from.red) + f64::from(to.red - from.red) * time,
        f64::from(from.green) + f64::from(to.green - from.green) * time,
        f64::from(from.blue) + f64::from(to.blue - from.blue) * time,
    ];
    expected_glow(
        ordinary,
        3.25 + 3.25 * time,
        tint,
        START + (1.4 - START) * time,
    )
}

fn expected_glow(ordinary: &[u8], sigma: f64, tint: [f64; 3], intensity: f64) -> Vec<u8> {
    let (width, height) = (SIZE[0] as usize, SIZE[1] as usize);
    assert_eq!(ordinary.len(), width * height * 4);
    let radius = (3.0 * sigma).ceil() as isize;
    let mut kernel = Vec::new();
    let mut sum = 0.0;
    for y in -radius..=radius {
        for x in -radius..=radius {
            let value = (-((x * x + y * y) as f64) / (2.0 * sigma * sigma)).exp();
            kernel.push((x, y, value));
            sum += value;
        }
    }
    let mut blurred = vec![0.0; width * height];
    for y in 0..height {
        for x in 0..width {
            let a = f64::from(ordinary[(y * width + x) * 4]) / 255.0;
            if a == 0.0 {
                continue;
            }
            for &(dx, dy, weight) in &kernel {
                let (xx, yy) = (x as isize + dx, y as isize + dy);
                if xx >= 0 && yy >= 0 && xx < width as isize && yy < height as isize {
                    blurred[yy as usize * width + xx as usize] += a * weight / sum;
                }
            }
        }
    }
    let mut result = ordinary.to_vec();
    for (i, rgba) in result.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let a = f64::from(ordinary[i * 4]) / 255.0;
        let halo = (intensity * blurred[i]).min(1.0);
        for (channel, tint) in tint.into_iter().enumerate() {
            rgba[channel] = (255.0 * (a + (1.0 - a) * tint * halo))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        rgba[3] = 255;
    }
    result
}
fn error(a: &[u8], b: &[u8]) -> u8 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap()
}

fn authored_glow_churn(device: &wgpu::Device, queue: &wgpu::Queue) {
    let mut scene = Scene::new();
    let mut objects = Vec::new();
    for index in 0..600 {
        let mut dot = scene.circle(0.05).unwrap();
        dot.set_translation(
            -3.75 + (index % 30) as f64 * 0.25,
            -2.375 + (index / 30) as f64 * 0.25,
        )
        .unwrap();
        dot.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
        dot.disable_stroke().unwrap();
        scene.add(&dot).unwrap();
        objects.push(dot);
    }
    let mut session = scene.execution_session().unwrap();
    let mut surface = Surface::new(device, queue);
    let ordinary = surface.render(device, queue, &mut session);
    let mut attached_pixels = None;
    let mut resident_bytes = None;
    let mut allocations = 0;
    for cycle in 0..1000 {
        scene
            .live(&mut session)
            .set_glow(
                &objects[0],
                EffectUpdate::default()
                    .color(Color::BLUE)
                    .radius(Pixels(3.0))
                    .intensity(1.4),
            )
            .unwrap();
        let attached = surface.render(device, queue, &mut session);
        assert_eq!(
            &attached,
            attached_pixels.get_or_insert_with(|| attached.clone())
        );
        assert!(error(&attached, &ordinary) > 2, "missing glow must fail");
        let stats = surface.effect_stats;
        assert_eq!(stats.rows_examined, 1);
        assert_eq!(
            stats.retained_texture_bytes,
            *resident_bytes.get_or_insert(stats.retained_texture_bytes)
        );
        assert!(stats.retained_texture_bytes > 0);
        if cycle > 0 {
            assert_eq!(stats.pipeline_compiles, 0);
        }
        allocations += stats.texture_allocations;
        assert_eq!(surface.upload_stats.geometry.bytes_uploaded, 0);
        scene.live(&mut session).remove_glow(&objects[0]).unwrap();
        assert_eq!(surface.render(device, queue, &mut session), ordinary);
        assert_eq!(surface.effect_stats.rows_examined, 1);
        assert_eq!(surface.effect_stats.retained_texture_bytes, 0);
        assert_eq!(surface.upload_stats.geometry.bytes_uploaded, 0);
        assert!(!surface.renderer.has_retained_effect_sources());
    }
    assert_eq!(surface.render(device, queue, &mut session), ordinary);
    assert_eq!(surface.effect_stats, Default::default());
    eprintln!("AUTHORED_GLOW_CHURN_PASS objects=600 cycles=1000 resident_plateau={} retired_bytes=0 texture_allocations={allocations} geometry_bytes=0", resident_bytes.unwrap());
}

fn live_source_glow_churn(device: &wgpu::Device, queue: &wgpu::Queue) {
    use noon::{LiveProgramStatus, RustHostCallbackTable};
    let mut source = noon::example_scenes::glow_churn::program().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    let mut surface =
        Surface::with_target_size(device, queue, Vec2::new(1152.0, 648.0), [1152, 648]);
    surface.capture_budget = 32 * 1024 * 1024;
    source.resume().unwrap();
    let ordinary = surface.render_publication(
        device,
        queue,
        source.take_renderer_publication(),
        wgpu::Color::BLACK,
    );
    let mut attached_pixels = None;
    let mut residency = None;
    let mut cycles = 0;
    for phase in 0..=21 {
        let time = if phase == 21 {
            10.0
        } else {
            (1.0 + phase as f64 * 26.0) / 60.0
        };
        match source.drive_to(&mut callbacks, time + 1e-8).unwrap() {
            LiveProgramStatus::PublicationPending(expected) => {
                let publication = source.take_renderer_publication();
                assert_eq!(publication.context(), expected);
                surface.render_publication(device, queue, publication, wgpu::Color::BLACK);
                source.admit_publication(expected).unwrap();
            }
            LiveProgramStatus::ReadyToResume => {}
            other => panic!("unexpected churn phase {phase}: {other:?}"),
        }
        let status = source.resume().unwrap();
        let image = surface.render_publication(
            device,
            queue,
            source.take_renderer_publication(),
            wgpu::Color::BLACK,
        );
        if phase < 20 {
            assert_eq!(surface.upload_stats.geometry.bytes_uploaded, 0);
        }
        if phase < 20 && phase % 2 == 0 {
            cycles += 100;
            assert!(
                image != ordinary,
                "the source must actually attach visible Glows"
            );
            assert!(&image == attached_pixels.get_or_insert_with(|| image.clone()));
            let bytes = surface.effect_stats.retained_texture_bytes;
            assert!(bytes > 0 && bytes <= surface.capture_budget);
            assert_eq!(bytes, *residency.get_or_insert(bytes));
            if phase > 0 {
                assert_eq!(surface.effect_stats.pipeline_compiles, 0);
            }
        } else {
            assert!(
                image == ordinary,
                "each source removal must restore exact ordinary pixels"
            );
            assert_eq!(surface.effect_stats.retained_texture_bytes, 0);
        }
        if phase < 20 {
            assert_eq!(surface.effect_stats.rows_examined, 100);
        }
        assert_eq!(status == LiveProgramStatus::Finished, phase == 21);
    }
    assert_eq!(cycles, 1000);
    eprintln!("G4 live source cycles={cycles} viewport=1152x648 peak_capture_bytes={} retired_bytes=0; forward correctness, not physical timing", residency.unwrap());
}

#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn public_scene_glow_pixels_and_lifecycle() {
    let backends = match std::env::var("NOON_GLOW_BACKEND").as_deref() {
        Ok("vulkan") => wgpu::Backends::VULKAN,
        Ok("gl") => wgpu::Backends::GL,
        Ok("metal") => wgpu::Backends::METAL,
        Ok("dx12") => wgpu::Backends::DX12,
        Err(std::env::VarError::NotPresent) => wgpu::Backends::PRIMARY,
        other => panic!("unsupported NOON_GLOW_BACKEND: {other:?}"),
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let info = adapter.get_info();
    assert_ne!(info.backend, wgpu::Backend::Noop);
    eprintln!("public Scene glow raster adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    authored_glow_churn(&device, &queue);
    live_source_glow_churn(&device, &queue);
    let (scene, source, animation) = scene(true);
    let (plain_scene, _, plain_animation) = self::scene(false);
    let mut session = scene.execution_session().unwrap();
    let mut plain = plain_scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    let plain_segment = plain_scene
        .live(&mut plain)
        .play_animation(&plain_animation)
        .unwrap();
    let mut surface = Surface::new(&device, &queue);
    let mut reference = Surface::new(&device, &queue);
    let mut last_plain = Vec::new();
    let mut max_error = 0;
    for time in [0.0, 0.25, 0.5, 0.75, 1.0] {
        scene
            .live(&mut session)
            .advance_segment_to(segment, time)
            .unwrap();
        plain_scene
            .live(&mut plain)
            .advance_segment_to(plain_segment, time)
            .unwrap();
        let actual = surface.render(&device, &queue, &mut session);
        last_plain = reference.render(&device, &queue, &mut plain);
        let expected = expected(&last_plain, time);
        max_error = max_error.max(error(&actual, &expected));
        assert!(
            error(&actual, &expected) <= 2,
            "public Scene full-image error at {time}: {}",
            error(&actual, &expected)
        );
        assert!(
            error(&expected, &last_plain) > 2,
            "missing-glow negative control is ineffective"
        );
        assert_eq!(
            surface.render(&device, &queue, &mut session),
            actual,
            "unchanged publication must preserve pixels"
        );
    }
    scene.live(&mut session).complete_segment(segment).unwrap();
    scene
        .live(&mut session)
        .set_glow(&source, EffectUpdate::default().intensity(0.0))
        .unwrap();
    assert_eq!(
        surface.render(&device, &queue, &mut session),
        last_plain,
        "neutral is ordinary, byte for byte"
    );
    scene
        .live(&mut session)
        .set_glow(&source, EffectUpdate::default().intensity(1.4))
        .unwrap();
    let restored = surface.render(&device, &queue, &mut session);
    assert!(error(&restored, &expected(&last_plain, 1.0)) <= 2);
    scene.live(&mut session).remove_glow(&source).unwrap();
    assert_eq!(
        surface.render(&device, &queue, &mut session),
        last_plain,
        "removal leaves no stale halo"
    );
    scene
        .live(&mut session)
        .set_glow(
            &source,
            EffectUpdate::default()
                .color(Color::BLUE)
                .radius(Pixels(6.5))
                .intensity(1.4),
        )
        .unwrap();
    assert_eq!(
        surface.render(&device, &queue, &mut session),
        restored,
        "new attachment generation reproduces the same image"
    );
    eprintln!("PUBLIC_SCENE_GLOW_PASS frames=14 references=5 max_error={max_error} neutral_exact=true removal_exact=true reattach_exact=true");
    for logical in [Vec2::new(704.0, 396.0), Vec2::new(513.25, 18477.0 / 64.0)] {
        let (scene, source, animation) = self::scene(true);
        let (plain_scene, _, plain_animation) = self::scene(false);
        let mut session = scene.execution_session().unwrap();
        let mut plain = plain_scene.execution_session().unwrap();
        let segment = scene.live(&mut session).play_animation(&animation).unwrap();
        let plain_segment = plain_scene
            .live(&mut plain)
            .play_animation(&plain_animation)
            .unwrap();
        let mut surface = Surface::with_logical_viewport(&device, &queue, logical);
        let mut reference = Surface::with_logical_viewport(&device, &queue, logical);
        let mut last_plain = Vec::new();
        for time in [0.0, 0.5, 1.0] {
            scene
                .live(&mut session)
                .advance_segment_to(segment, time)
                .unwrap();
            plain_scene
                .live(&mut plain)
                .advance_segment_to(plain_segment, time)
                .unwrap();
            let actual = surface.render(&device, &queue, &mut session);
            last_plain = reference.render(&device, &queue, &mut plain);
            let oracle = expected(&last_plain, time);
            assert!(
                error(&actual, &oracle) <= 2,
                "logical viewport {logical:?}: pixel error {}",
                error(&actual, &oracle)
            );
            assert!(
                error(&oracle, &last_plain) > 2,
                "missing-halo negative control"
            );
        }
        scene.live(&mut session).complete_segment(segment).unwrap();
        scene
            .live(&mut session)
            .set_glow(&source, EffectUpdate::default().intensity(0.0))
            .unwrap();
        assert_eq!(surface.render(&device, &queue, &mut session), last_plain);
        scene.live(&mut session).remove_glow(&source).unwrap();
        assert_eq!(surface.render(&device, &queue, &mut session), last_plain);
        eprintln!("LOGICAL_VIEWPORT_GLOW_PASS logical={logical:?} max_error<=2 neutral_exact=true removal_exact=true");
    }
}

#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn authored_glow_pulse_pixels_restore_and_replay() {
    let backends = match std::env::var("NOON_GLOW_BACKEND").as_deref() {
        Ok("vulkan") => wgpu::Backends::VULKAN,
        Ok("gl") => wgpu::Backends::GL,
        Ok("metal") => wgpu::Backends::METAL,
        Ok("dx12") => wgpu::Backends::DX12,
        Err(std::env::VarError::NotPresent) => wgpu::Backends::PRIMARY,
        other => panic!("unsupported NOON_GLOW_BACKEND: {other:?}"),
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let info = adapter.get_info();
    assert_ne!(info.backend, wgpu::Backend::Noop);
    eprintln!("authored GlowPulse raster adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();

    let make_scene = |attached: bool| {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.4).unwrap();
        source.disable_stroke().unwrap();
        source.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
        if attached {
            source
                .set_glow(
                    EffectUpdate::default()
                        .color(Color::RED)
                        .radius(Pixels(3.25))
                        .intensity(START),
                )
                .unwrap();
        }
        scene.add(&source).unwrap();
        (scene, source)
    };
    let (plain_scene, _) = make_scene(false);
    let mut plain = plain_scene.execution_session().unwrap();
    let ordinary = Surface::new(&device, &queue).render(&device, &queue, &mut plain);
    let times = [0.0, 0.25, 0.5, 0.75, 1.0];
    // Literal independent samples: ThereAndBack uses smooth(0/.5/1), while
    // every pulse restores the captured baseline at its raw mapped endpoint.
    let profiles = [
        (
            "default",
            AnimationOptions::new(),
            [0.0, 0.5, 1.0, 0.5, 0.0],
        ),
        (
            "linear",
            AnimationOptions::new().rate_func(RateFunction::Linear),
            [0.0, 0.25, 0.5, 0.75, 0.0],
        ),
        (
            "reversed linear",
            AnimationOptions::new()
                .rate_func(RateFunction::Linear)
                .reverse_rate_function(true),
            [1.0, 0.75, 0.5, 0.25, 0.0],
        ),
    ];
    let mut max_error = 0;
    for attached in [true, false] {
        let baseline = if attached { START } else { 0.0 };
        // This Surface has exactly ten physical pixels per world unit.
        // Missing glow uses the authored defaults: white, scene sigma .15.
        let sigma = if attached { 3.25 } else { 1.5 };
        let tint = if attached {
            [
                f64::from(Color::RED.red),
                f64::from(Color::RED.green),
                f64::from(Color::RED.blue),
            ]
        } else {
            [1.0; 3]
        };
        let baseline_oracle = expected_glow(&ordinary, sigma, tint, baseline);
        for (label, options, amplitudes) in profiles {
            let (scene, source) = make_scene(attached);
            let original = source.get_effect("glow").ok();
            let original_definition = original.as_ref().map(|h| h.authored_definition().unwrap());
            let mut session = scene.execution_session().unwrap();
            let mut surface = Surface::new(&device, &queue);
            let baseline_pixels = surface.render(&device, &queue, &mut session);
            assert!(error(&baseline_pixels, &baseline_oracle) <= 2);
            let segment = scene
                .live(&mut session)
                .declare_and_activate_glow_pulse(&source, 2.0, options.run_time(1.0))
                .unwrap();
            let pulse_binding = source.get_effect("glow").unwrap();
            let mut frames = Vec::new();
            for (index, time) in times.into_iter().enumerate() {
                scene
                    .live(&mut session)
                    .advance_segment_to(segment, time)
                    .unwrap();
                let actual = surface.render(&device, &queue, &mut session);
                let intensity = baseline + (2.0 - baseline) * amplitudes[index];
                let oracle = expected_glow(&ordinary, sigma, tint, intensity);
                let difference = error(&actual, &oracle);
                max_error = max_error.max(difference);
                assert!(
                    difference <= 2,
                    "attached={attached} {label} t={time}: full RGBA error {difference}"
                );
                if amplitudes[index] == 0.0 {
                    assert_eq!(
                        actual, baseline_pixels,
                        "attached={attached} {label} t={time}: neutral/restored pixels changed"
                    );
                } else {
                    assert!(error(&oracle, &baseline_oracle) > 4,
                        "attached={attached} {label} t={time}: missing-pulse negative control is ineffective");
                    assert!(
                        error(&actual, &baseline_pixels) > 2,
                        "attached={attached} {label} t={time}: no visible pulse was delivered"
                    );
                }
                assert_eq!(
                    surface.render(&device, &queue, &mut session),
                    actual,
                    "attached={attached} {label} t={time}: repeated publication changed pixels"
                );
                frames.push(actual);
            }
            for index in [4, 1, 3, 0, 2, 4] {
                session.seek(times[index]).unwrap();
                assert_eq!(
                    surface.render(&device, &queue, &mut session),
                    frames[index],
                    "attached={attached} {label}: rewind differs from forward playback"
                );
            }
            scene.live(&mut session).complete_segment(segment).unwrap();
            assert_eq!(
                surface.render(&device, &queue, &mut session),
                baseline_pixels,
                "attached={attached} {label}: completion changed the baseline image"
            );
            if let Some(original) = original {
                assert_eq!(
                    source.get_effect("glow").unwrap().node_id(),
                    original.node_id()
                );
                assert_eq!(
                    original.authored_definition().unwrap(),
                    original_definition.unwrap()
                );
            } else {
                assert!(source.get_effect("glow").is_err());
                assert!(pulse_binding.authored_definition().is_err());
            }
        }
    }
    eprintln!("GLOW_PULSE_PASS profiles=6 samples=30 max_error={max_error} repeated_exact=true rewind_exact=true restoration_exact=true identity=true temporary_retirement=true");
}

#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn click_glow_pulse_pixels_restore_and_retire() {
    use noon::integration::{NativeInputModifiers, NativePointerInputKind, NativePointerPosition};
    use noon_core::{NativePointerId, NativePointerInput};

    fn click(execution: &mut noon::SceneExecution, sequence: u64) {
        for (sequence, pressed) in [(sequence, true), (sequence + 1, false)] {
            let position = NativePointerPosition::new(Vec2::ZERO, Vec2::new(40.0, 30.0)).unwrap();
            let kind = if pressed {
                NativePointerInputKind::Press {
                    position,
                    button: 0,
                }
            } else {
                NativePointerInputKind::Release {
                    position,
                    button: 0,
                }
            };
            let token = execution.session().native_pointer_input_token().unwrap();
            let input = NativePointerInput::new(
                sequence,
                token.pointer(),
                token.context(),
                NativeInputModifiers::default(),
                kind,
            );
            execution
                .submit_native_pointer_input(&token, input)
                .unwrap();
        }
    }

    let backends = match std::env::var("NOON_GLOW_BACKEND").as_deref() {
        Ok("vulkan") => wgpu::Backends::VULKAN,
        Ok("gl") => wgpu::Backends::GL,
        Ok("metal") => wgpu::Backends::METAL,
        Ok("dx12") => wgpu::Backends::DX12,
        Err(std::env::VarError::NotPresent) => wgpu::Backends::PRIMARY,
        other => panic!("unsupported NOON_GLOW_BACKEND: {other:?}"),
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let info = adapter.get_info();
    assert_ne!(info.backend, wgpu::Backend::Noop);
    eprintln!("click GlowPulse raster adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let make_scene = |attached: bool| {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.4).unwrap();
        source.disable_stroke().unwrap();
        source.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
        if attached {
            source
                .set_glow(
                    EffectUpdate::default()
                        .color(Color::RED)
                        .radius(Pixels(3.25))
                        .intensity(START),
                )
                .unwrap();
        }
        scene.add(&source).unwrap();
        (scene, source)
    };
    let (plain_scene, _) = make_scene(false);
    let mut plain = plain_scene.into_execution().unwrap();
    let ordinary = Surface::new(&device, &queue).render(&device, &queue, plain.session_mut());
    let mut max_error = 0;
    let mut samples = 0;
    for attached in [true, false] {
        let (mut scene, source) = make_scene(attached);
        let pulse = scene
            .declare_glow_pulse(&source, 2.0, AnimationOptions::new().run_time(1.0))
            .unwrap();
        scene.on_click(&source, Some(&pulse)).unwrap();
        assert_eq!(
            source.get_effect("glow").is_ok(),
            attached,
            "binding alone must not install an absent glow"
        );
        let mut execution = scene.into_execution().unwrap();
        // The declaration saw START, while the admitted click must capture this
        // later authored value. No declaration-time binding/source is authoritative.
        let baseline = if attached { 0.9 } else { 0.0 };
        if attached {
            execution
                .live()
                .set_glow(&source, EffectUpdate::default().intensity(baseline))
                .unwrap();
        }
        let original = source.get_effect("glow").ok();
        let original_definition = original.as_ref().map(|h| h.authored_definition().unwrap());
        execution
            .session_mut()
            .configure_native_pointer_input(
                NativePointerId {
                    source: 81,
                    pointer: 2,
                },
                1,
            )
            .unwrap();
        let mut surface = Surface::new(&device, &queue);
        let baseline_pixels = surface.render(&device, &queue, execution.session_mut());
        let sigma = if attached { 3.25 } else { 1.5 };
        let tint = if attached {
            [
                f64::from(Color::RED.red),
                f64::from(Color::RED.green),
                f64::from(Color::RED.blue),
            ]
        } else {
            [1.0; 3]
        };
        let baseline_oracle = expected_glow(&ordinary, sigma, tint, baseline);
        assert!(error(&baseline_pixels, &baseline_oracle) <= 2);
        if attached {
            assert!(
                error(
                    &baseline_oracle,
                    &expected_glow(&ordinary, sigma, tint, START)
                ) > 4,
                "stale declaration-time capture negative control is ineffective"
            );
        }

        click(&mut execution, 1);
        assert!(execution.session().interactions_active());
        let first_binding = source.get_effect("glow").unwrap();
        for (index, amplitude) in [0.0, 0.5, 1.0, 0.5, 0.0].into_iter().enumerate() {
            let wall_time = 100.0 + index as f64 * 0.25;
            execution.advance_interactions(wall_time).unwrap();
            if index == 1 {
                click(&mut execution, 3);
                assert_eq!(
                    source.get_effect("glow").unwrap().node_id(),
                    first_binding.node_id()
                );
            }
            let actual = surface.render(&device, &queue, execution.session_mut());
            let oracle = expected_glow(
                &ordinary,
                sigma,
                tint,
                baseline + (2.0 - baseline) * amplitude,
            );
            let difference = error(&actual, &oracle);
            max_error = max_error.max(difference);
            samples += 1;
            assert!(
                difference <= 2,
                "attached={attached} wall={wall_time}: full RGBA error {difference}"
            );
            assert_eq!(
                execution.session().frame().time,
                0.0,
                "click ticks advanced authored time"
            );
            assert_eq!(
                surface.render(&device, &queue, execution.session_mut()),
                actual,
                "identical publication changed click-pulse pixels"
            );
            if amplitude == 0.0 {
                assert_eq!(
                    actual, baseline_pixels,
                    "neutral/restored frame differs from the ordinary baseline"
                );
            } else {
                assert!(
                    error(&oracle, &baseline_oracle) > 4,
                    "missing-pulse negative control is ineffective"
                );
                assert!(
                    error(&actual, &baseline_pixels) > 2,
                    "no visible click-pulse was rendered"
                );
            }
        }
        assert!(
            !execution.session().interactions_active(),
            "retrigger extended the original deadline"
        );
        if let Some(original) = &original {
            assert_eq!(
                source.get_effect("glow").unwrap().node_id(),
                original.node_id()
            );
            assert_eq!(
                original.authored_definition().unwrap(),
                original_definition.clone().unwrap()
            );
        } else {
            assert!(source.get_effect("glow").is_err());
            assert!(first_binding.authored_definition().is_err());
        }
        let settled = execution.session().publication_context();
        execution.advance_interactions(101.5).unwrap();
        assert_eq!(
            execution.session().publication_context(),
            settled,
            "settled pulse kept publishing"
        );

        // A later click acquires a new temporary generation, and an admitted seek
        // cancels that exact pulse. Seeking never replays an unrecorded click.
        click(&mut execution, 5);
        let second_binding = source.get_effect("glow").unwrap();
        if attached {
            assert_eq!(second_binding.node_id(), first_binding.node_id());
        } else {
            assert_ne!(second_binding.node_id(), first_binding.node_id());
        }
        execution.advance_interactions(200.0).unwrap();
        execution.advance_interactions(200.5).unwrap();
        let peak = surface.render(&device, &queue, execution.session_mut());
        let difference = error(&peak, &expected_glow(&ordinary, sigma, tint, 2.0));
        max_error = max_error.max(difference);
        samples += 1;
        assert!(
            difference <= 2,
            "later click failed its independent peak oracle"
        );
        execution.seek(0.0).unwrap();
        assert!(!execution.session().interactions_active());
        assert_eq!(
            surface.render(&device, &queue, execution.session_mut()),
            baseline_pixels,
            "seek cancellation failed to restore the ordinary baseline"
        );
        if let Some(original) = original {
            assert_eq!(
                source.get_effect("glow").unwrap().node_id(),
                original.node_id()
            );
            assert_eq!(
                original.authored_definition().unwrap(),
                original_definition.unwrap()
            );
        } else {
            assert!(source.get_effect("glow").is_err());
            assert!(second_binding.authored_definition().is_err());
        }
    }
    eprintln!("CLICK_GLOW_PULSE_PASS profiles=2 samples={samples} max_error={max_error} capture_at_click=true retrigger_original_deadline=true repeated_exact=true restoration_exact=true identity=true temporary_retirement=true seek_cleanup=true idle=true");
}

#[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
fn drawable_scene(kind: &str, glowing: bool, opacity: f64) -> (Scene, Mobject) {
    let mut scene = Scene::new();
    let mut source = match kind {
        "circle" => scene.circle(0.8).unwrap(),
        "rectangle" => scene.rectangle(2.0, 1.3).unwrap(),
        "line" => scene.line((-1.0, -0.6), (1.0, 0.6)).unwrap(),
        "path" => scene
            .path(
                noon_core::VectorPath::new()
                    .move_to(Vec2::new(-1.0, -0.5))
                    .cubic_to(
                        Vec2::new(-0.5, 1.2),
                        Vec2::new(0.6, -1.1),
                        Vec2::new(1.0, 0.5),
                    ),
                noon_core::SemanticStyle::default(),
            )
            .unwrap(),
        "text" => scene
            .text(noon::Text::new("Noon").with_font_size(36.0))
            .unwrap(),
        "math" => scene
            .math_typst(noon::MathTypst::new("x^2 + y^2").with_font_size(36.0))
            .unwrap(),
        other => panic!("unknown source {other}"),
    };
    if matches!(kind, "circle" | "rectangle" | "line" | "path") {
        source.disable_fill().unwrap();
        source.set_stroke_color(1.0, 1.0, 1.0, 1.0).unwrap();
        source.set_stroke_width(0.18).unwrap();
    }
    source.set_opacity(opacity).unwrap();
    if glowing {
        source
            .set_glow(
                EffectUpdate::default()
                    .color(Color::RED)
                    .radius(Pixels(3.25))
                    .intensity(START),
            )
            .unwrap();
    }
    (scene, source)
}

#[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn generic_sources_follow_real_reveal_pixels_and_apply_scope_opacity_once() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let mut max_error = 0;
    for kind in ["circle", "rectangle", "line", "path", "text", "math"] {
        for write in [false, true] {
            if write && !matches!(kind, "text" | "math") {
                continue;
            }
            let (scene, source) = drawable_scene(kind, true, 1.0);
            let (plain_scene, plain_source) = drawable_scene(kind, false, 1.0);
            let mut session = scene.execution_session().unwrap();
            let mut plain = plain_scene.execution_session().unwrap();
            let segment = if write {
                scene
                    .live(&mut session)
                    .declare_and_activate_text_write(&source, false, options)
            } else {
                scene
                    .live(&mut session)
                    .declare_and_activate_create(&source, options)
            }
            .unwrap();
            let plain_segment = if write {
                plain_scene
                    .live(&mut plain)
                    .declare_and_activate_text_write(&plain_source, false, options)
            } else {
                plain_scene
                    .live(&mut plain)
                    .declare_and_activate_create(&plain_source, options)
            }
            .unwrap();
            let mut surface = Surface::new(&device, &queue);
            let mut reference = Surface::new(&device, &queue);
            let mut last_plain = Vec::new();
            for time in [0.0, 0.125, 0.5, 0.875, 1.0] {
                scene
                    .live(&mut session)
                    .advance_segment_to(segment, time)
                    .unwrap();
                plain_scene
                    .live(&mut plain)
                    .advance_segment_to(plain_segment, time)
                    .unwrap();
                let actual = surface.render(&device, &queue, &mut session);
                last_plain = reference.render(&device, &queue, &mut plain);
                let expected = expected(&last_plain, 0.0);
                let difference = error(&actual, &expected);
                max_error = max_error.max(difference);
                assert!(
                    difference <= 2,
                    "{kind} write={write} reveal={time}: full-image Gaussian error {difference}"
                );
                let repeated = surface.render(&device, &queue, &mut session);
                assert!(
                    repeated == actual,
                    "{kind} write={write} reveal={time}: clean pixel error {} ({} channels)",
                    error(&repeated, &actual),
                    repeated.iter().zip(&actual).filter(|(a, b)| a != b).count()
                );
            }
            scene.live(&mut session).complete_segment(segment).unwrap();
            plain_scene
                .live(&mut plain)
                .complete_segment(plain_segment)
                .unwrap();
            scene
                .live(&mut session)
                .set_object_opacity(&source, 0.35)
                .unwrap();
            let actual = surface.render(&device, &queue, &mut session);
            let opaque = expected(&last_plain, 0.0);
            let mut faded = opaque.clone();
            for pixel in faded.as_chunks_mut::<4>().0 {
                for channel in &mut pixel[..3] {
                    *channel = (f64::from(*channel) * 0.35).round() as u8;
                }
            }
            assert!(
                error(&actual, &faded) <= 2,
                "{kind} opacity applied more than once: {}",
                error(&actual, &faded)
            );
            scene
                .live(&mut session)
                .set_object_opacity(&source, 1.0)
                .unwrap();
            scene
                .live(&mut session)
                .set_glow(&source, EffectUpdate::default().intensity(0.0))
                .unwrap();
            assert_eq!(
                surface.render(&device, &queue, &mut session),
                last_plain,
                "{kind} neutral changed source pixels"
            );
            scene.live(&mut session).remove_glow(&source).unwrap();
            assert_eq!(
                surface.render(&device, &queue, &mut session),
                last_plain,
                "{kind} removal changed source pixels"
            );
            assert_eq!(surface.renderer.effect_capture_texture_bytes(), 0);
        }
    }
    eprintln!("GENERIC_SOURCE_GLOW_PASS cases=8 reveal_frames=40 max_error={max_error} opacity_once=true neutral_exact=true retirement=true");
}

fn image_scene(
    effect: Option<noon::effects::GlowSource>,
    sampling: noon::RasterImageSampling,
    paint_opacity: f64,
    scope_opacity: f64,
) -> (Scene, Mobject, noon::DeclaredAnimation) {
    let mut scene = Scene::new();
    let mut options = noon::example_scenes::image_glow::image_options().unwrap();
    options.set_sampling(sampling);
    options.set_opacity(paint_opacity).unwrap();
    let mut source = scene.image(options).unwrap();
    source.set_translation(-0.65, -0.15).unwrap();
    source.scale(-1.1, 0.85).unwrap();
    source.rotate(0.17).unwrap();
    source.set_object_opacity(scope_opacity).unwrap();
    if let Some(mode) = effect {
        source
            .set_glow(
                EffectUpdate::default()
                    .color(Color::RED)
                    .radius(Pixels(3.25))
                    .intensity(START)
                    .source(mode),
            )
            .unwrap();
    }
    scene.add(&source).unwrap();
    let mut target = source.target_editor().unwrap();
    target.shift(1.2, 0.3).unwrap();
    target.rotate(0.2).unwrap();
    if effect.is_some() {
        target
            .set_glow(
                EffectUpdate::default()
                    .color(Color::BLUE)
                    .radius(Pixels(6.5))
                    .intensity(1.4),
            )
            .unwrap();
    }
    let animation = scene
        .declare_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    (scene, source, animation)
}

// Independent full 2D convolution of ordinary transparent-background image
// alpha. Source rasterization is qualified separately; neither a production
// glow intermediate nor its separable shader/kernel helper supplies this oracle.
// Intrinsic image alpha is coverage, including for Silhouette; paint modulation
// is independent from that coverage and final object opacity.
fn expected_image_glow(
    source: &[u8],
    mask: &[u8],
    time: f64,
    scope_opacity: f64,
    background: wgpu::Color,
) -> Vec<u8> {
    let (width, height) = (SIZE[0] as usize, SIZE[1] as usize);
    assert_eq!(source.len(), width * height * 4);
    assert_eq!(mask.len(), source.len());
    let sigma = 3.25 + 3.25 * time;
    let radius = (3.0 * sigma).ceil() as isize;
    let mut weights = Vec::new();
    let mut sum = 0.0;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let weight = (-((dx * dx + dy * dy) as f64) / (2.0 * sigma * sigma)).exp();
            weights.push((dx, dy, weight));
            sum += weight;
        }
    }
    let mut blurred = vec![0.0; width * height];
    for y in 0..height {
        for x in 0..width {
            let alpha = f64::from(mask[(y * width + x) * 4 + 3]) / 255.0;
            if alpha == 0.0 {
                continue;
            }
            for &(dx, dy, weight) in &weights {
                let (xx, yy) = (x as isize + dx, y as isize + dy);
                if xx >= 0 && yy >= 0 && xx < width as isize && yy < height as isize {
                    blurred[yy as usize * width + xx as usize] += alpha * weight / sum;
                }
            }
        }
    }
    let tint = [
        f64::from(Color::RED.red) + f64::from(Color::BLUE.red - Color::RED.red) * time,
        f64::from(Color::RED.green) + f64::from(Color::BLUE.green - Color::RED.green) * time,
        f64::from(Color::RED.blue) + f64::from(Color::BLUE.blue - Color::RED.blue) * time,
    ];
    let bg = [background.r, background.g, background.b];
    let intensity = START + (1.4 - START) * time;
    let mut output = Vec::with_capacity(source.len());
    for (index, pixel) in source.as_chunks::<4>().0.iter().enumerate() {
        let source_alpha = f64::from(pixel[3]) / 255.0;
        let halo = (intensity * blurred[index]).clamp(0.0, 1.0);
        let alpha = scope_opacity * (source_alpha + (1.0 - source_alpha) * halo);
        for channel in 0..3 {
            let value = scope_opacity
                * (f64::from(pixel[channel]) / 255.0 + (1.0 - source_alpha) * tint[channel] * halo)
                + (1.0 - alpha) * bg[channel];
            output.push((value * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        output.push(
            ((alpha + (1.0 - alpha) * background.a) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8,
        );
    }
    output
}

fn gradient_glow_scene(
    kind: usize,
    mode: Option<noon::effects::GlowSource>,
    opacity: f64,
    silhouette: bool,
) -> (Scene, Mobject, noon::DeclaredAnimation) {
    use noon::{GradientStop, LinearGradient};
    let mut scene = Scene::new();
    let mut source = match kind {
        0 => scene.circle(0.6).unwrap(),
        1 => scene.rectangle(1.2, 1.6).unwrap(),
        _ => Mobject::manim_rectangle(std::rc::Rc::clone(scene.integration_store()), 1.2, 1.6)
            .unwrap(),
    };
    source.disable_stroke().unwrap();
    let paint = |vertical| {
        LinearGradient::new(
            if vertical { [0.5, 0.0] } else { [0.0, 0.5] },
            if vertical { [0.5, 1.0] } else { [1.0, 0.5] },
            [
                GradientStop::new(
                    0.0,
                    Color::rgba(1.0, 0.0, 0.0, if vertical { 0.8 } else { 0.2 }),
                )
                .unwrap(),
                GradientStop::new(
                    1.0,
                    Color::rgba(0.0, 0.0, 1.0, if vertical { 0.3 } else { 0.9 }),
                )
                .unwrap(),
            ],
        )
        .unwrap()
    };
    if silhouette {
        source.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    } else {
        source.set_fill_gradient(paint(false), Some(0.55)).unwrap();
    }
    source.set_object_opacity(opacity).unwrap();
    source.scale(-1.1, 0.85).unwrap();
    source.rotate(0.17).unwrap();
    source.shift(-0.6, 0.1).unwrap();
    if kind == 2 {
        source
            .set_stroke_color(0.0, 1.0, 0.0, if silhouette { 1.0 } else { 0.5 })
            .unwrap();
        source.set_stroke_width(0.12).unwrap();
    }
    if let Some(mode) = mode {
        source
            .set_glow(
                EffectUpdate::default()
                    .source(mode)
                    .color(Color::RED)
                    .radius(Pixels(3.25))
                    .intensity(START),
            )
            .unwrap();
    }
    scene.add(&source).unwrap();
    let mut target = source.target_editor().unwrap();
    if !silhouette {
        target.set_fill_gradient(paint(true), Some(0.55)).unwrap();
    }
    if mode.is_some() {
        target
            .set_glow(
                EffectUpdate::default()
                    .color(Color::BLUE)
                    .radius(Pixels(6.5))
                    .intensity(1.4),
            )
            .unwrap();
    }
    let animation = scene
        .declare_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    (scene, source, animation)
}

#[test]
#[ignore = "requires a native raster adapter; absence fails qualification"]
fn gradient_glow_preserves_paint_alpha_seek_and_local_capture() {
    use noon::effects::GlowSource;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("gradient glow adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut samples = 0;
    let mut max_error = 0;
    for kind in 0..3 {
        for mode in [GlowSource::Painted, GlowSource::Silhouette] {
            let (scene, source, animation) = gradient_glow_scene(kind, Some(mode), 0.45, false);
            let (plain_scene, plain_source, plain_animation) =
                gradient_glow_scene(kind, None, 1.0, false);
            let (mask_scene, _, mask_animation) = gradient_glow_scene(kind, None, 1.0, true);
            let mut session = scene.execution_session().unwrap();
            let mut plain = plain_scene.execution_session().unwrap();
            let mut mask = mask_scene.execution_session().unwrap();
            let segment = scene.live(&mut session).play_animation(&animation).unwrap();
            let plain_segment = plain_scene
                .live(&mut plain)
                .play_animation(&plain_animation)
                .unwrap();
            let mask_segment = mask_scene
                .live(&mut mask)
                .play_animation(&mask_animation)
                .unwrap();
            let mut surface = Surface::new(&device, &queue);
            let mut reference = Surface::new(&device, &queue);
            let mut mask_surface = Surface::new(&device, &queue);
            let times = [0.0, 0.25, 0.5, 0.75, 1.0];
            let mut frames = Vec::new();
            for time in times {
                scene
                    .live(&mut session)
                    .advance_segment_to(segment, time)
                    .unwrap();
                plain_scene
                    .live(&mut plain)
                    .advance_segment_to(plain_segment, time)
                    .unwrap();
                mask_scene
                    .live(&mut mask)
                    .advance_segment_to(mask_segment, time)
                    .unwrap();
                let ordinary =
                    reference.render_on(&device, &queue, &mut plain, wgpu::Color::TRANSPARENT);
                let silhouette =
                    mask_surface.render_on(&device, &queue, &mut mask, wgpu::Color::TRANSPARENT);
                let alpha = if mode == GlowSource::Painted {
                    &ordinary
                } else {
                    &silhouette
                };
                let expected =
                    expected_image_glow(&ordinary, alpha, time, 0.45, wgpu::Color::TRANSPARENT);
                let actual =
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
                let difference = error(&actual, &expected);
                max_error = max_error.max(difference);
                let worst = actual
                    .iter()
                    .zip(&expected)
                    .enumerate()
                    .max_by_key(|(_, (a, b))| a.abs_diff(**b))
                    .unwrap()
                    .0;
                assert!(difference <= 2, "kind={kind} mode={mode:?} time={time}: RGBA error {difference} at ({},{}) actual={:?} expected={:?} ordinary={:?}", (worst / 4) % SIZE[0] as usize, (worst / 4) / SIZE[0] as usize, &actual[worst/4*4..worst/4*4+4], &expected[worst/4*4..worst/4*4+4], &ordinary[worst/4*4..worst/4*4+4]);
                let missing = expected_image_glow(
                    &ordinary,
                    &vec![0; ordinary.len()],
                    time,
                    0.45,
                    wgpu::Color::TRANSPARENT,
                );
                assert!(error(&expected, &missing) > 2, "missing halo control");
                assert_eq!(
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                    actual
                );
                assert_eq!(surface.effect_stats.source_passes, 0);
                assert_eq!(surface.effect_stats.blur_passes, 0);
                assert_eq!(surface.effect_stats.bytes_uploaded, 0);
                assert_eq!(surface.effect_stats.texture_allocations, 0);
                frames.push(actual);
                samples += 1;
            }
            for index in [4, 1, 3, 0, 2, 4] {
                session.seek(times[index]).unwrap();
                assert_eq!(
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                    frames[index]
                );
            }
            scene.live(&mut session).complete_segment(segment).unwrap();
            plain_scene
                .live(&mut plain)
                .complete_segment(plain_segment)
                .unwrap();
            let changed = noon::LinearGradient::new(
                [0.5, 0.0],
                [0.5, 1.0],
                [
                    noon::GradientStop::new(0.0, Color::rgba(0.0, 1.0, 0.0, 0.1)).unwrap(),
                    noon::GradientStop::new(1.0, Color::rgba(1.0, 0.0, 0.0, 0.6)).unwrap(),
                ],
            )
            .unwrap();
            scene
                .live(&mut session)
                .set_fill_gradient(&source, changed.clone(), None)
                .unwrap();
            plain_scene
                .live(&mut plain)
                .set_fill_gradient(&plain_source, changed, None)
                .unwrap();
            let ordinary =
                reference.render_on(&device, &queue, &mut plain, wgpu::Color::TRANSPARENT);
            let silhouette =
                mask_surface.render_on(&device, &queue, &mut mask, wgpu::Color::TRANSPARENT);
            let alpha = if mode == GlowSource::Painted {
                &ordinary
            } else {
                &silhouette
            };
            let background = wgpu::Color {
                r: 0.125,
                g: 0.25,
                b: 0.375,
                a: 1.0,
            };
            let expected = expected_image_glow(&ordinary, alpha, 1.0, 0.45, background);
            let actual = surface.render_on(&device, &queue, &mut session, background);
            assert!(
                error(&actual, &expected) <= 2,
                "paint-only edit must recapture current stops"
            );
            assert!(surface.effect_stats.source_passes > 0);
            assert_eq!(surface.effect_stats.texture_allocations, 0);
            // A warmed intensity edit uses the captured paint and blur again.
            scene
                .live(&mut session)
                .set_glow(&source, EffectUpdate::default().intensity(1.2))
                .unwrap();
            surface.render_on(&device, &queue, &mut session, background);
            assert_eq!(surface.effect_stats.source_passes, 0);
            assert_eq!(surface.effect_stats.blur_passes, 0);
            assert_eq!(surface.effect_stats.texture_allocations, 0);
            plain_scene
                .live(&mut plain)
                .set_object_opacity(&plain_source, 0.45)
                .unwrap();
            let ordinary = reference.render(&device, &queue, &mut plain);
            scene
                .live(&mut session)
                .set_glow(&source, EffectUpdate::default().intensity(0.0))
                .unwrap();
            assert_eq!(
                surface.render(&device, &queue, &mut session),
                ordinary,
                "neutral paint must be exact"
            );
            scene.live(&mut session).remove_glow(&source).unwrap();
            assert_eq!(
                surface.render(&device, &queue, &mut session),
                ordinary,
                "removed paint must be exact"
            );
        }
    }
    eprintln!("gradient glow: 6 profiles / {samples} samples / max RGBA error {max_error}");
}

#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn image_glow_keeps_foreground_painter_order() {
    use noon::{effects::GlowSource, RasterImageSampling};

    // Independently composite separately rendered premultiplied RGBA layers.
    fn over(foreground: &[u8], background: &[u8]) -> Vec<u8> {
        assert_eq!(foreground.len(), background.len());
        let mut output = Vec::with_capacity(foreground.len());
        for (front, back) in foreground
            .as_chunks::<4>()
            .0
            .iter()
            .zip(background.as_chunks::<4>().0)
        {
            let remaining = 1.0 - f64::from(front[3]) / 255.0;
            for channel in 0..4 {
                output.push(
                    (f64::from(front[channel]) + remaining * f64::from(back[channel]))
                        .round()
                        .clamp(0.0, 255.0) as u8,
                );
            }
        }
        output
    }

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let info = adapter.get_info();
    assert_ne!(info.backend, wgpu::Backend::Noop);
    eprintln!("image glow painter-order adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut max_error = 0;

    for foreground_alpha in [1.0, 0.45] {
        let add_foreground = |scene: &mut Scene| {
            let mut foreground = scene.rectangle(1.6, 4.6).unwrap();
            foreground.set_translation(0.9, 0.0).unwrap();
            foreground.disable_stroke().unwrap();
            foreground
                .set_fill(0.05, 0.8, 0.2, foreground_alpha)
                .unwrap();
            scene.add(&foreground).unwrap();
        };
        let (mut scene, source, animation) = image_scene(
            Some(GlowSource::Painted),
            RasterImageSampling::Linear,
            1.0,
            1.0,
        );
        // The later rectangle overlaps both the image and its halo. Capturing
        // or drawing the image after this item would reverse painter order.
        add_foreground(&mut scene);
        let (plain_scene, _, plain_animation) =
            image_scene(None, RasterImageSampling::Linear, 1.0, 1.0);
        let (mut neutral_scene, _, neutral_animation) =
            image_scene(None, RasterImageSampling::Linear, 1.0, 1.0);
        add_foreground(&mut neutral_scene);
        let mut foreground_scene = Scene::new();
        add_foreground(&mut foreground_scene);

        let mut session = scene.execution_session().unwrap();
        let mut plain = plain_scene.execution_session().unwrap();
        let mut neutral = neutral_scene.execution_session().unwrap();
        let mut foreground_session = foreground_scene.execution_session().unwrap();
        let segment = scene.live(&mut session).play_animation(&animation).unwrap();
        let plain_segment = plain_scene
            .live(&mut plain)
            .play_animation(&plain_animation)
            .unwrap();
        let neutral_segment = neutral_scene
            .live(&mut neutral)
            .play_animation(&neutral_animation)
            .unwrap();
        let mut surface = Surface::new(&device, &queue);
        let mut reference = Surface::new(&device, &queue);
        let mut neutral_surface = Surface::new(&device, &queue);
        let mut foreground_surface = Surface::new(&device, &queue);
        let foreground = foreground_surface.render_on(
            &device,
            &queue,
            &mut foreground_session,
            wgpu::Color::TRANSPARENT,
        );
        let mut last_neutral = Vec::new();
        for time in [0.5, 1.0] {
            scene
                .live(&mut session)
                .advance_segment_to(segment, time)
                .unwrap();
            plain_scene
                .live(&mut plain)
                .advance_segment_to(plain_segment, time)
                .unwrap();
            neutral_scene
                .live(&mut neutral)
                .advance_segment_to(neutral_segment, time)
                .unwrap();
            let ordinary =
                reference.render_on(&device, &queue, &mut plain, wgpu::Color::TRANSPARENT);
            let filtered =
                expected_image_glow(&ordinary, &ordinary, time, 1.0, wgpu::Color::TRANSPARENT);
            let expected = over(&foreground, &filtered);
            let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
            let difference = error(&actual, &expected);
            max_error = max_error.max(difference);
            assert!(
                difference <= 2,
                "foreground alpha={foreground_alpha} t={time}: RGBA error {difference}"
            );
            assert!(
                error(&expected, &over(&filtered, &foreground)) > 2,
                "reversed-painter-order negative control is ineffective"
            );
            last_neutral =
                neutral_surface.render_on(&device, &queue, &mut neutral, wgpu::Color::TRANSPARENT);
            assert!(
                error(&expected, &last_neutral) > 2,
                "missing-image-glow negative control is ineffective"
            );
        }
        scene.live(&mut session).complete_segment(segment).unwrap();
        scene
            .live(&mut session)
            .set_glow(&source, EffectUpdate::default().intensity(0.0))
            .unwrap();
        assert_eq!(
            surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
            last_neutral,
            "neutral image glow changed source/foreground painter composition"
        );
        scene.live(&mut session).remove_glow(&source).unwrap();
        assert_eq!(
            surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
            last_neutral,
            "removed image glow changed source/foreground painter composition"
        );
    }
    eprintln!("IMAGE_GLOW_PAINTER_ORDER_PASS cases=2 frames=4 max_error={max_error} neutral_exact=true removal_exact=true");
}

#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn image_glow_preserves_texture_alpha_sampling_replay_and_lifecycle() {
    use noon::{effects::GlowSource, RasterImageSampling};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let info = adapter.get_info();
    assert_ne!(info.backend, wgpu::Backend::Noop);
    eprintln!("image glow raster adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    const PAINT: f64 = 0.4;
    const OPACITY: f64 = 0.45;
    let mut max_error = 0;
    for sampling in [
        RasterImageSampling::Nearest,
        RasterImageSampling::Linear,
        RasterImageSampling::Bicubic,
    ] {
        for mode in [GlowSource::Painted, GlowSource::Silhouette] {
            let (scene, source, animation) = image_scene(Some(mode), sampling, PAINT, OPACITY);
            let (plain_scene, plain_source, plain_animation) =
                image_scene(None, sampling, PAINT, 1.0);
            let mask_opacity = if mode == GlowSource::Painted {
                PAINT
            } else {
                1.0
            };
            let (mask_scene, _, mask_animation) = image_scene(None, sampling, mask_opacity, 1.0);
            let resource = source.state().unwrap().content.image().unwrap().resource();
            let mut session = scene.execution_session().unwrap();
            let mut plain = plain_scene.execution_session().unwrap();
            let mut mask = mask_scene.execution_session().unwrap();
            let segment = scene.live(&mut session).play_animation(&animation).unwrap();
            let plain_segment = plain_scene
                .live(&mut plain)
                .play_animation(&plain_animation)
                .unwrap();
            let mask_segment = mask_scene
                .live(&mut mask)
                .play_animation(&mask_animation)
                .unwrap();
            let mut surface = Surface::new(&device, &queue);
            let mut reference = Surface::new(&device, &queue);
            let mut mask_surface = Surface::new(&device, &queue);
            let times = [0.0, 0.25, 0.5, 0.75, 1.0];
            let mut frames = Vec::new();
            for time in times {
                scene
                    .live(&mut session)
                    .advance_segment_to(segment, time)
                    .unwrap();
                plain_scene
                    .live(&mut plain)
                    .advance_segment_to(plain_segment, time)
                    .unwrap();
                mask_scene
                    .live(&mut mask)
                    .advance_segment_to(mask_segment, time)
                    .unwrap();
                let ordinary =
                    reference.render_on(&device, &queue, &mut plain, wgpu::Color::TRANSPARENT);
                let alpha =
                    mask_surface.render_on(&device, &queue, &mut mask, wgpu::Color::TRANSPARENT);
                let actual =
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
                let expected =
                    expected_image_glow(&ordinary, &alpha, time, OPACITY, wgpu::Color::TRANSPARENT);
                let difference = error(&actual, &expected);
                max_error = max_error.max(difference);
                assert!(
                    difference <= 2,
                    "{sampling:?} {mode:?} t={time}: RGBA error {difference}"
                );
                let missing = expected_image_glow(
                    &ordinary,
                    &vec![0; alpha.len()],
                    time,
                    OPACITY,
                    wgpu::Color::TRANSPARENT,
                );
                assert!(
                    error(&expected, &missing) > 2,
                    "missing-glow negative control is ineffective"
                );
                assert_eq!(
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                    actual,
                    "clean publication changed image pixels"
                );
                assert_eq!(
                    session.frame().objects[0]
                        .content
                        .image()
                        .unwrap()
                        .resource(),
                    resource
                );
                frames.push(actual);
            }
            // Same runtime and retained GPU resources must reconstruct these
            // images when presented out of order; no frame-count shader clock.
            for index in [4, 1, 3, 0, 2, 4] {
                session.seek(times[index]).unwrap();
                assert_eq!(
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                    frames[index],
                    "{sampling:?} {mode:?}: seek differs from forward"
                );
            }
            scene.live(&mut session).complete_segment(segment).unwrap();
            plain_scene
                .live(&mut plain)
                .complete_segment(plain_segment)
                .unwrap();
            mask_scene
                .live(&mut mask)
                .complete_segment(mask_segment)
                .unwrap();
            let background = wgpu::Color {
                r: 0.125,
                g: 0.25,
                b: 0.375,
                a: 1.0,
            };
            let ordinary =
                reference.render_on(&device, &queue, &mut plain, wgpu::Color::TRANSPARENT);
            let alpha =
                mask_surface.render_on(&device, &queue, &mut mask, wgpu::Color::TRANSPARENT);
            let expected = expected_image_glow(&ordinary, &alpha, 1.0, OPACITY, background);
            let restored = surface.render_on(&device, &queue, &mut session, background);
            assert!(
                error(&restored, &expected) <= 2,
                "colored-background composition differs"
            );
            plain_scene
                .live(&mut plain)
                .set_object_opacity(&plain_source, OPACITY)
                .unwrap();
            let plain_pixels = reference.render_on(&device, &queue, &mut plain, background);
            scene
                .live(&mut session)
                .set_glow(&source, EffectUpdate::default().intensity(0.0))
                .unwrap();
            assert_eq!(
                surface.render_on(&device, &queue, &mut session, background),
                plain_pixels,
                "neutral effect changed ordinary image"
            );
            scene
                .live(&mut session)
                .set_glow(&source, EffectUpdate::default().intensity(1.4))
                .unwrap();
            assert_eq!(
                surface.render_on(&device, &queue, &mut session, background),
                restored,
                "glow restoration changed image"
            );
            scene.live(&mut session).remove_glow(&source).unwrap();
            assert_eq!(
                surface.render_on(&device, &queue, &mut session, background),
                plain_pixels,
                "effect removal left a stale image or halo"
            );
            assert_eq!(surface.renderer.effect_capture_texture_bytes(), 0);
        }
    }
    eprintln!("IMAGE_GLOW_PASS cases=6 frames=30 max_error={max_error} alpha=true sampling=true replay_exact=true neutral_exact=true retirement=true");
}

const EDGE_IMAGE_RGBA: [u8; 4] = [96, 160, 224, 192];
const EDGE_IMAGE_PAINT: f64 = 0.6;
const EDGE_IMAGE_SCOPE: f64 = 0.65;
const EDGE_IMAGE_SIGMA: f64 = 1.25;
const EDGE_IMAGE_INTENSITY: f64 = 1.8;

fn fractional_edge_image(
    bounds: [f64; 4],
    sampling: noon::RasterImageSampling,
    effect: Option<noon::effects::GlowSource>,
) -> Scene {
    let mut scene = Scene::new();
    let mut options = noon::ImageMobjectOptions::rgba8(1, 1, EDGE_IMAGE_RGBA).unwrap();
    options.set_height(1.0).unwrap();
    options.set_sampling(sampling);
    options.set_opacity(EDGE_IMAGE_PAINT).unwrap();
    let mut image = scene.image(options).unwrap();
    // Surface's 8 by 6 world view has ten physical pixels per scene unit.
    image
        .scale(
            (bounds[2] - bounds[0]) / 10.0,
            (bounds[3] - bounds[1]) / 10.0,
        )
        .unwrap();
    image
        .set_translation(
            ((bounds[0] + bounds[2]) * 0.5 - 40.0) / 10.0,
            (30.0 - (bounds[1] + bounds[3]) * 0.5) / 10.0,
        )
        .unwrap();
    if let Some(source) = effect {
        image.set_object_opacity(EDGE_IMAGE_SCOPE).unwrap();
        image
            .set_glow(
                EffectUpdate::default()
                    .color(Color::BLUE)
                    .radius(Pixels(EDGE_IMAGE_SIGMA))
                    .intensity(EDGE_IMAGE_INTENSITY)
                    .source(source),
            )
            .unwrap();
    }
    scene.add(&image).unwrap();
    scene
}

// This oracle derives a one-texel image's ordinary pixel-center coverage directly
// from its authored rectangle, including the canonical integer image crop. It
// evaluates the complete source outside the output viewport before convolution.
// No renderer image, capture texture, shader helper or source-mask readback is an
// input. The floor variant is a deliberate tile-local-crop defect witness.
fn fractional_edge_reference(
    bounds: [f64; 4],
    source: noon::effects::GlowSource,
    global_crop: bool,
) -> (Vec<u8>, Vec<u8>) {
    let crop = bounds.map(|bound| {
        if global_crop {
            bound.trunc()
        } else {
            bound.floor()
        }
    });
    let covered = |x: i32, y: i32| {
        let (x, y) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        x >= bounds[0]
            && y >= bounds[1]
            && x < bounds[2]
            && y < bounds[3]
            && x >= crop[0]
            && y >= crop[1]
            && x < crop[2]
            && y < crop[3]
    };
    let quantize = |value: f64| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    let intrinsic_alpha = f64::from(EDGE_IMAGE_RGBA[3]) / 255.0;
    let painted_alpha = intrinsic_alpha * EDGE_IMAGE_PAINT;
    let painted = [
        quantize(f64::from(EDGE_IMAGE_RGBA[0]) / 255.0 * painted_alpha),
        quantize(f64::from(EDGE_IMAGE_RGBA[1]) / 255.0 * painted_alpha),
        quantize(f64::from(EDGE_IMAGE_RGBA[2]) / 255.0 * painted_alpha),
        quantize(painted_alpha),
    ];
    let mask_alpha = f64::from(match source {
        noon::effects::GlowSource::Painted => painted[3],
        noon::effects::GlowSource::Silhouette => EDGE_IMAGE_RGBA[3],
    }) / 255.0;
    let radius = (3.0 * EDGE_IMAGE_SIGMA).ceil() as i32;
    let mut kernel = Vec::new();
    let mut total = 0.0;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let weight =
                (-f64::from(dx * dx + dy * dy) / (2.0 * EDGE_IMAGE_SIGMA * EDGE_IMAGE_SIGMA)).exp();
            kernel.push((dx, dy, weight));
            total += weight;
        }
    }
    let tint = [Color::BLUE.red, Color::BLUE.green, Color::BLUE.blue];
    let mut ordinary = Vec::with_capacity((SIZE[0] * SIZE[1] * 4) as usize);
    let mut expected = Vec::with_capacity(ordinary.capacity());
    for y in 0..SIZE[1] as i32 {
        for x in 0..SIZE[0] as i32 {
            let pixel = if covered(x, y) { painted } else { [0; 4] };
            ordinary.extend_from_slice(&pixel);
            let coverage = kernel
                .iter()
                .filter(|&&(dx, dy, _)| covered(x + dx, y + dy))
                .map(|&(_, _, weight)| weight)
                .sum::<f64>()
                / total;
            let halo = (EDGE_IMAGE_INTENSITY * mask_alpha * coverage).clamp(0.0, 1.0);
            let alpha = f64::from(pixel[3]) / 255.0;
            for channel in 0..3 {
                expected.push(quantize(
                    EDGE_IMAGE_SCOPE
                        * (f64::from(pixel[channel]) / 255.0
                            + (1.0 - alpha) * f64::from(tint[channel]) * halo),
                ));
            }
            expected.push(quantize(EDGE_IMAGE_SCOPE * (alpha + (1.0 - alpha) * halo)));
        }
    }
    (ordinary, expected)
}

#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn offscreen_image_glow_uses_global_pixel_crop() {
    use noon::{effects::GlowSource, RasterImageSampling};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let info = adapter.get_info();
    assert_ne!(info.backend, wgpu::Backend::Noop);
    eprintln!("offscreen image glow raster adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut max_error = 0;
    let mut rejected_wrong_crop = 0;
    for bounds in [
        [-0.7, -0.7, 5.7, 4.7],
        [-5.7, 10.3, -0.2, 16.2],
        [22.3, -6.7, 28.2, -0.2],
        [78.3, 58.3, 84.7, 65.7],
    ] {
        for sampling in [
            RasterImageSampling::Nearest,
            RasterImageSampling::Linear,
            RasterImageSampling::Bicubic,
        ] {
            for mode in [GlowSource::Painted, GlowSource::Silhouette] {
                let (expected_ordinary, expected) = fractional_edge_reference(bounds, mode, true);
                let (_, wrong_crop) = fractional_edge_reference(bounds, mode, false);
                if error(&expected, &wrong_crop) > 2 {
                    rejected_wrong_crop += 1;
                }
                let ordinary_scene = fractional_edge_image(bounds, sampling, None);
                let mut ordinary_session = ordinary_scene.execution_session().unwrap();
                let mut ordinary_surface = Surface::new(&device, &queue);
                let ordinary = ordinary_surface.render_on(
                    &device,
                    &queue,
                    &mut ordinary_session,
                    wgpu::Color::TRANSPARENT,
                );
                assert!(
                    error(&ordinary, &expected_ordinary) <= 1,
                    "ordinary {sampling:?} bounds={bounds:?} differs from analytic coverage"
                );
                let glow_scene = fractional_edge_image(bounds, sampling, Some(mode));
                let mut glow_session = glow_scene.execution_session().unwrap();
                let mut glow_surface = Surface::new(&device, &queue);
                let actual = glow_surface.render_on(
                    &device,
                    &queue,
                    &mut glow_session,
                    wgpu::Color::TRANSPARENT,
                );
                let difference = error(&actual, &expected);
                max_error = max_error.max(difference);
                assert!(
                    difference <= 2,
                    "{sampling:?} {mode:?} bounds={bounds:?}: RGBA error {difference}"
                );
                assert!(
                    expected.iter().skip(3).step_by(4).any(|&alpha| alpha > 2),
                    "offscreen source must contribute visible output"
                );
            }
        }
    }
    assert_eq!(
        rejected_wrong_crop, 18,
        "negative-origin crop defect must be detected"
    );
    eprintln!("OFFSCREEN_IMAGE_GLOW_PASS cases=24 max_error={max_error} analytic_source=true global_crop=true offscreen_support=true wrong_crop_rejected={rejected_wrong_crop}");
}

// Direct four-channel square convolution: independent of the separable GPU
// operator and driven only by an ordinary no-effect Scene raster.
fn expected_rgba_blur(source: &[u8], sigma: f64, opacity: f64) -> Vec<u8> {
    expected_rgba_blur_in_extent(source, sigma, opacity, SIZE)
}

#[test]
#[ignore = "requires a native raster adapter; absence fails qualification"]
fn public_ordered_leaf_stack_matches_independent_source_operators_and_neutral_restoration() {
    use noon::effects::{Blur, EffectOffset, Shadow};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let fixture = |opacity: f64| {
        let mut scene = Scene::new();
        let mut source = scene.rectangle(1.1, 0.7).unwrap();
        source.disable_stroke().unwrap();
        source.set_fill(0.2, 0.7, 0.3, 0.65).unwrap();
        source.set_object_opacity(opacity).unwrap();
        scene.add(&source).unwrap();
        (scene, source)
    };
    let (plain_scene, _) = fixture(1.0);
    let ordinary = Surface::new(&device, &queue).render_on(
        &device,
        &queue,
        &mut plain_scene.execution_session().unwrap(),
        wgpu::Color::TRANSPARENT,
    );
    let glow =
        |pixels: &[u8]| expected_image_glow(pixels, pixels, 0.0, 1.0, wgpu::Color::TRANSPARENT);
    let shadow = |pixels: &[u8], opacity| {
        expected_shadow(pixels, 1.75, [8.0, -4.0], [0.15, 0.25, 0.95, 0.7], opacity)
    };
    let correct = expected_rgba_blur(&shadow(&glow(&ordinary), 1.0), 0.75, 0.37);
    let wrong_order = expected_rgba_blur(&glow(&shadow(&ordinary, 1.0)), 0.75, 0.37);
    assert!(
        error(&correct, &wrong_order) > 2,
        "fixture must distinguish noncommuting order within the predeclared two-byte bound"
    );
    let mut maximum_error = 0;
    for reversed in [false, true] {
        for opacity in [1.0, 0.37] {
            let (mut scene, source) = fixture(opacity);
            let add_glow = |scene: &mut Scene| {
                scene
                    .set_glow(
                        &source,
                        EffectUpdate::default()
                            .color(Color::RED)
                            .radius(Pixels(3.25))
                            .intensity(START),
                    )
                    .unwrap()
            };
            let add_shadow = |scene: &mut Scene| {
                scene
                    .add_effect(
                        &source,
                        Shadow::new(
                            EffectUpdate::default()
                                .color(Color::rgba(0.15, 0.25, 0.95, 0.7))
                                .radius(Pixels(1.75))
                                .offset(EffectOffset::Pixels([8.0, -4.0])),
                        )
                        .unwrap(),
                        "shadow",
                    )
                    .unwrap()
            };
            if reversed {
                add_shadow(&mut scene);
                add_glow(&mut scene);
            } else {
                add_glow(&mut scene);
                add_shadow(&mut scene);
            }
            scene
                .add_effect(
                    &source,
                    Blur::new(EffectUpdate::default().radius(Pixels(0.75))).unwrap(),
                    "soften",
                )
                .unwrap();
            let mut session = scene.execution_session().unwrap();
            let mut surface = Surface::new(&device, &queue);
            let expected = if reversed {
                expected_rgba_blur(&glow(&shadow(&ordinary, 1.0)), 0.75, opacity)
            } else {
                expected_rgba_blur(&shadow(&glow(&ordinary), 1.0), 0.75, opacity)
            };
            let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
            let delta = error(&actual, &expected);
            maximum_error = maximum_error.max(delta);
            assert!(
                delta <= 2,
                "ordered public stack reversed={reversed} opacity={opacity} error={delta}"
            );
            let clean = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
            assert_eq!(actual, clean);
            assert_eq!(
                surface.effect_stats.source_passes
                    + surface.effect_stats.blur_passes
                    + surface.effect_stats.composite_passes
                    + surface.effect_stats.bytes_uploaded
                    + surface.effect_stats.texture_allocations,
                0
            );
            scene
                .live(&mut session)
                .set_glow(&source, EffectUpdate::default().intensity(0.0))
                .unwrap();
            scene
                .live(&mut session)
                .set_effect(
                    &source,
                    "shadow",
                    EffectUpdate::default().color(Color::rgba(0.15, 0.25, 0.95, 0.0)),
                )
                .unwrap();
            scene
                .live(&mut session)
                .set_effect(
                    &source,
                    "soften",
                    EffectUpdate::default().radius(Pixels(0.0)),
                )
                .unwrap();
            let (plain_scene, _) = fixture(opacity);
            let plain = Surface::new(&device, &queue).render_on(
                &device,
                &queue,
                &mut plain_scene.execution_session().unwrap(),
                wgpu::Color::TRANSPARENT,
            );
            assert_eq!(
                surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                plain
            );
            for name in ["glow", "shadow", "soften"] {
                scene
                    .live(&mut session)
                    .remove_effect(&source, name)
                    .unwrap();
            }
            assert_eq!(
                surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                plain
            );
            assert_eq!(surface.effect_stats.retained_texture_bytes, 0);
        }
    }
    eprintln!("ordered public leaf: 4 three-pass images, exact clean/neutral/removal, max byte error {maximum_error}");
}

fn expected_rgba_blur_in_extent(
    source: &[u8],
    sigma: f64,
    opacity: f64,
    size: [u32; 2],
) -> Vec<u8> {
    if sigma == 0.0 {
        return source
            .iter()
            .map(|&v| (f64::from(v) * opacity).round() as u8)
            .collect();
    }
    let [width, height] = size.map(|v| v as usize);
    let radius = (3.0 * sigma).ceil() as isize;
    let mut taps = Vec::new();
    let mut mass = 0.0;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let weight = (-((dx * dx + dy * dy) as f64) / (2.0 * sigma * sigma)).exp();
            mass += weight;
            taps.push((dx, dy, weight));
        }
    }
    let mut output = vec![0_u8; source.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0_f64; 4];
            for &(dx, dy, weight) in &taps {
                let (sx, sy) = (x as isize + dx, y as isize + dy);
                if sx < 0 || sy < 0 || sx >= width as isize || sy >= height as isize {
                    continue;
                }
                for channel in 0..4 {
                    sum[channel] +=
                        f64::from(source[(sy as usize * width + sx as usize) * 4 + channel])
                            * weight
                            / mass;
                }
            }
            for channel in 0..4 {
                output[(y * width + x) * 4 + channel] = (sum[channel] * opacity).round() as u8;
            }
        }
    }
    output
}

fn blur_fixture(
    profile: usize,
    attached: bool,
    opacity: f32,
    pixels: bool,
) -> (Scene, Mobject, noon::DeclaredAnimation) {
    use noon::effects::Blur;
    let mut scene = Scene::new();
    let mut source = match profile {
        0 => scene.circle(0.6).unwrap(),
        1 => scene.rectangle(1.4, 0.8).unwrap(),
        2 => scene
            .image(noon::example_scenes::image_glow::image_options().unwrap())
            .unwrap(),
        3 => scene
            .path(
                noon_core::VectorPath::new()
                    .move_to(Vec2::new(-1.0, -0.5))
                    .cubic_to(
                        Vec2::new(-0.5, 1.2),
                        Vec2::new(0.6, -1.1),
                        Vec2::new(1.0, 0.5),
                    ),
                noon_core::SemanticStyle::default(),
            )
            .unwrap(),
        4 => scene.rectangle(1.4, 0.8).unwrap(),
        #[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
        5 => scene
            .text(noon::Text::new("Noon").with_font_size(36.0))
            .unwrap(),
        #[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
        6 => scene
            .math_typst(noon::MathTypst::new("x^2 + y^2").with_font_size(36.0))
            .unwrap(),
        _ => unreachable!(),
    };
    if profile != 2 {
        source.disable_stroke().unwrap();
        source.set_fill(0.2, 0.7, 0.9, 0.6).unwrap();
    }
    if profile == 3 {
        source.disable_fill().unwrap();
        source.set_stroke_color(0.8, 0.2, 0.6, 0.7).unwrap();
        source.set_stroke_width(0.18).unwrap();
        source.scale(-1.1, 0.85).unwrap();
        source.rotate(0.17).unwrap();
    }
    if profile == 4 {
        source
            .set_fill_gradient(
                noon::LinearGradient::new(
                    [0.0, 0.5],
                    [1.0, 0.5],
                    [
                        noon::GradientStop::new(0.0, Color::rgba(1.0, 0.0, 0.0, 0.2)).unwrap(),
                        noon::GradientStop::new(1.0, Color::rgba(0.0, 0.0, 1.0, 0.9)).unwrap(),
                    ],
                )
                .unwrap(),
                Some(0.55),
            )
            .unwrap();
    }
    source.set_object_opacity(f64::from(opacity)).unwrap();
    let radius = |sigma| {
        if pixels {
            noon::effects::EffectRadius::Pixels(sigma)
        } else {
            noon::effects::EffectRadius::Scene(sigma / 10.0)
        }
    };
    if attached {
        source
            .add_effect(
                Blur::new(EffectUpdate::default().radius(radius(1.25))).unwrap(),
                "soften",
            )
            .unwrap();
    }
    scene.add(&source).unwrap();
    let mut target = source.target_editor().unwrap();
    if attached {
        target
            .set_effect("soften", EffectUpdate::default().radius(radius(3.25)))
            .unwrap();
    }
    let animation = scene
        .declare_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    (scene, source, animation)
}

#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn public_scene_blur_rgba_pixels_seek_neutral_and_removal() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("public Blur adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut maximum_error = 0;
    let mut sample_count = 0;
    let profiles = 0..if cfg!(all(
        feature = "native-text",
        feature = "typst",
        feature = "bundled-fonts"
    )) {
        7
    } else {
        5
    };
    let profile_count = profiles.len() * 2;
    for profile in profiles {
        let (plain_scene, _, _) = blur_fixture(profile, false, 1.0, true);
        let mut plain = plain_scene.execution_session().unwrap();
        let ordinary = Surface::new(&device, &queue).render_on(
            &device,
            &queue,
            &mut plain,
            wgpu::Color::TRANSPARENT,
        );
        for (pixels, opacity) in [(true, 1.0_f32), (false, 0.37_f32)] {
            let (scene, source, animation) = blur_fixture(profile, true, opacity, pixels);
            let mut session = scene.execution_session().unwrap();
            let segment = scene.live(&mut session).play_animation(&animation).unwrap();
            let mut surface = Surface::new(&device, &queue);
            for time in [0.0, 0.5, 1.0, 0.25, 0.5] {
                session.seek(time).unwrap();
                let actual =
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
                let expected = expected_rgba_blur(&ordinary, 1.25 + 2.0 * time, f64::from(opacity));
                let delta = error(&actual, &expected);
                maximum_error = maximum_error.max(delta);
                sample_count += 1;
                assert!(delta <= 1,"Blur profile={profile} pixels={pixels} opacity={opacity} time={time} full-image error={delta}");
                assert!(
                    error(&actual, &ordinary) > 2,
                    "fixture must detect a missing Blur"
                );
                assert_eq!(
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                    actual
                );
                assert_eq!(surface.effect_stats.source_passes, 0);
                assert_eq!(surface.effect_stats.blur_passes, 0);
                assert_eq!(surface.effect_stats.composite_passes, 0);
                assert_eq!(surface.effect_stats.bytes_uploaded, 0);
                assert_eq!(surface.effect_stats.texture_allocations, 0);
            }
            scene
                .live(&mut session)
                .advance_segment_to(segment, 1.0)
                .unwrap();
            scene.live(&mut session).complete_segment(segment).unwrap();
            scene
                .live(&mut session)
                .set_effect(
                    &source,
                    "soften",
                    EffectUpdate::default().radius(if pixels {
                        noon::effects::EffectRadius::Pixels(0.0)
                    } else {
                        noon::effects::EffectRadius::Scene(0.0)
                    }),
                )
                .unwrap();
            let neutral =
                surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
            scene
                .live(&mut session)
                .remove_effect(&source, "soften")
                .unwrap();
            assert_eq!(
                surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                neutral
            );
            assert_eq!(surface.renderer.effect_capture_texture_bytes(), 0);
        }
    }
    eprintln!("public Blur profiles={profile_count} samples={sample_count} max_rgba8_error={maximum_error}");
}

// Specification reference: direct 2-D convolution, RGBA8 intermediate alpha,
// transparent bilinear translation, then original source over colored alpha.
// It shares no production filter, tile planner, uniform or shader arithmetic.
fn expected_shadow(
    source: &[u8],
    sigma: f64,
    offset: [f64; 2],
    tint: [f64; 4],
    opacity: f64,
) -> Vec<u8> {
    expected_shadow_in_extent(source, sigma, offset, tint, opacity, SIZE)
}

fn expected_shadow_in_extent(
    source: &[u8],
    sigma: f64,
    offset: [f64; 2],
    tint: [f64; 4],
    opacity: f64,
    size: [u32; 2],
) -> Vec<u8> {
    let blurred = expected_rgba_blur_in_extent(source, sigma, 1.0, size);
    let [width, height] = size.map(|v| v as usize);
    let alpha = |x: isize, y: isize| {
        if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
            0.0
        } else {
            f64::from(blurred[(y as usize * width + x as usize) * 4 + 3]) / 255.0
        }
    };
    let mut result = Vec::with_capacity(source.len());
    for y in 0..height {
        for x in 0..width {
            let qx = x as f64 - offset[0];
            let qy = y as f64 + offset[1];
            let (ix, iy) = (qx.floor() as isize, qy.floor() as isize);
            let (fx, fy) = (qx - qx.floor(), qy - qy.floor());
            let mask = ((1.0 - fy) * ((1.0 - fx) * alpha(ix, iy) + fx * alpha(ix + 1, iy))
                + fy * ((1.0 - fx) * alpha(ix, iy + 1) + fx * alpha(ix + 1, iy + 1)))
                * tint[3];
            let pixel = &source[(y * width + x) * 4..][..4];
            let behind = 1.0 - f64::from(pixel[3]) / 255.0;
            for channel in 0..4 {
                let value = (f64::from(pixel[channel]) / 255.0
                    + behind * mask * if channel == 3 { 1.0 } else { tint[channel] })
                    * opacity;
                result.push((255.0 * value).round().clamp(0.0, 255.0) as u8);
            }
        }
    }
    result
}

#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn public_shadow_pixels_offset_animation_hard_shadow_and_retirement() {
    use noon::effects::{EffectOffset, EffectRadius, Shadow};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("public Shadow adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let profiles = if cfg!(all(
        feature = "native-text",
        feature = "typst",
        feature = "bundled-fonts"
    )) {
        7
    } else {
        5
    };
    let mut samples = 0;
    let mut maximum_error = 0;
    for profile in 0..profiles {
        let (plain_scene, _, _) = blur_fixture(profile, false, 1.0, true);
        let mut plain = plain_scene.execution_session().unwrap();
        let ordinary = Surface::new(&device, &queue).render_on(
            &device,
            &queue,
            &mut plain,
            wgpu::Color::TRANSPARENT,
        );
        for (pixels, opacity) in [(true, 1.0_f32), (false, 0.37_f32)] {
            let radius = |v| {
                if pixels {
                    EffectRadius::Pixels(v)
                } else {
                    EffectRadius::Scene(v / 10.0)
                }
            };
            let offset = |v: [f64; 2]| {
                if pixels {
                    EffectOffset::Pixels(v)
                } else {
                    EffectOffset::Scene(v.map(|c| c / 10.0))
                }
            };
            let (scene, mut source, _) = blur_fixture(profile, false, opacity, pixels);
            source
                .add_effect(
                    Shadow::new(
                        EffectUpdate::default()
                            .color(Color::rgba(1.0, 0.0, 0.0, 0.35))
                            .radius(radius(1.25))
                            .offset(offset([4.25, -3.5])),
                    )
                    .unwrap(),
                    "depth",
                )
                .unwrap();
            let mut target = source.target_editor().unwrap();
            target
                .set_effect(
                    "depth",
                    EffectUpdate::default()
                        .color(Color::rgba(0.0, 0.0, 1.0, 0.8))
                        .radius(radius(3.25))
                        .offset(offset([-5.5, 2.25])),
                )
                .unwrap();
            let animation = scene
                .declare_transform_to(
                    &source,
                    &target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )
                .unwrap();
            let mut session = scene.execution_session().unwrap();
            let segment = scene.live(&mut session).play_animation(&animation).unwrap();
            let mut surface = Surface::new(&device, &queue);
            for time in [0.0, 0.5, 1.0, 0.25, 0.5] {
                session.seek(time).unwrap();
                let actual =
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
                let expected = expected_shadow(
                    &ordinary,
                    1.25 + 2.0 * time,
                    [4.25 - 9.75 * time, -3.5 + 5.75 * time],
                    [1.0 - time, 0.0, time, 0.35 + 0.45 * time],
                    f64::from(opacity),
                );
                let delta = error(&actual, &expected);
                maximum_error = maximum_error.max(delta);
                samples += 1;
                let worst = actual
                    .iter()
                    .zip(&expected)
                    .enumerate()
                    .max_by_key(|(_, (&a, &b))| a.abs_diff(b))
                    .unwrap()
                    .0;
                let at = (worst / 4) * 4;
                assert!(delta<=1,"Shadow profile={profile} pixels={pixels} opacity={opacity} time={time}: error={delta} pixel={} actual={:?} expected={:?} state={:?}",at/4,&actual[at..at+4],&expected[at..at+4],session.frame().objects[0]);
                assert!(
                    error(&actual, &ordinary) > 2,
                    "fixture must reject missing Shadow"
                );
                assert_eq!(
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                    actual
                );
                assert_eq!(surface.effect_stats.source_passes, 0);
                assert_eq!(surface.effect_stats.blur_passes, 0);
                assert_eq!(surface.effect_stats.composite_passes, 0);
                assert_eq!(surface.effect_stats.bytes_uploaded, 0);
                assert_eq!(surface.effect_stats.texture_allocations, 0);
            }
            scene
                .live(&mut session)
                .advance_segment_to(segment, 1.0)
                .unwrap();
            scene.live(&mut session).complete_segment(segment).unwrap();
            surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
            // Color-only composition must reuse the captured source and Gaussian.
            scene
                .live(&mut session)
                .set_effect(
                    &source,
                    "depth",
                    EffectUpdate::default().color(Color::rgba(0.2, 0.7, 0.1, 0.6)),
                )
                .unwrap();
            surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
            assert_eq!(surface.effect_stats.source_passes, 0);
            assert_eq!(surface.effect_stats.blur_passes, 0);
            assert_eq!(surface.effect_stats.texture_allocations, 0);
            assert_eq!(surface.effect_stats.bytes_uploaded, 32);
            for translation in [[0.0, 0.0], [-4.25, 3.5]] {
                scene
                    .live(&mut session)
                    .set_effect(
                        &source,
                        "depth",
                        EffectUpdate::default()
                            .radius(radius(0.0))
                            .offset(offset(translation)),
                    )
                    .unwrap();
                let hard =
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
                let expected = expected_shadow(
                    &ordinary,
                    0.0,
                    translation,
                    [0.2, 0.7, 0.1, 0.6],
                    f64::from(opacity),
                );
                assert!(
                    error(&hard, &expected) <= 1,
                    "hard Shadow profile={profile} offset={translation:?}"
                );
            }
            scene
                .live(&mut session)
                .set_effect(
                    &source,
                    "depth",
                    EffectUpdate::default().color(Color::rgba(0.2, 0.7, 0.1, 0.0)),
                )
                .unwrap();
            let neutral =
                surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
            scene
                .live(&mut session)
                .remove_effect(&source, "depth")
                .unwrap();
            assert_eq!(
                surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                neutral
            );
            assert_eq!(surface.renderer.effect_capture_texture_bytes(), 0);
        }
    }
    eprintln!("public Shadow profiles={} samples={samples} max_rgba8_error={maximum_error} hard=true neutral_exact=true retirement=true",profiles*2);
}

#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn offscreen_shadow_support_and_foreground_painter_order_match_extended_field() {
    use noon::effects::{EffectOffset, Shadow};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let extended = [SIZE[0] + 80, SIZE[1] + 80];
    let background = [26_u8, 51, 77, 255];
    for (bounds, offset, front) in [
        ([-10, 20, -2, 36], [18.25, -2.5], [10, 26, 16, 32]),
        ([82, 23, 90, 39], [-18.25, 2.5], [66, 26, 72, 32]),
    ] {
        let mut source_pixels = Vec::with_capacity((extended[0] * extended[1] * 4) as usize);
        for y in -40..SIZE[1] as i32 + 40 {
            for x in -40..SIZE[0] as i32 + 40 {
                source_pixels.extend_from_slice(
                    if x >= bounds[0] && x < bounds[2] && y >= bounds[1] && y < bounds[3] {
                        &[38, 77, 115, 153]
                    } else {
                        &[0; 4]
                    },
                );
            }
        }
        let field = expected_shadow_in_extent(
            &source_pixels,
            2.25,
            offset,
            [0.8, 0.1, 0.3, 0.75],
            1.0,
            extended,
        );
        let mut expected = Vec::with_capacity((SIZE[0] * SIZE[1] * 4) as usize);
        let mut unoccluded = Vec::with_capacity(expected.capacity());
        for y in 0..SIZE[1] as i32 {
            for x in 0..SIZE[0] as i32 {
                let p = (((y + 40) as u32 * extended[0] + (x + 40) as u32) * 4) as usize;
                let alpha = f64::from(field[p + 3]) / 255.0;
                let mut pixel = [0; 4];
                for c in 0..3 {
                    pixel[c] = (f64::from(field[p + c]) + (1.0 - alpha) * f64::from(background[c]))
                        .round() as u8;
                }
                pixel[3] = 255;
                unoccluded.extend_from_slice(&pixel);
                if x >= front[0] && x < front[2] && y >= front[1] && y < front[3] {
                    pixel = [0, 255, 0, 255];
                }
                expected.extend_from_slice(&pixel);
            }
        }
        let mut scene = Scene::new();
        let mut source = scene
            .rectangle(
                f64::from(bounds[2] - bounds[0]) / 10.0,
                f64::from(bounds[3] - bounds[1]) / 10.0,
            )
            .unwrap();
        source
            .set_translation(
                f64::from(bounds[0] + bounds[2]) / 20.0 - 4.0,
                3.0 - f64::from(bounds[1] + bounds[3]) / 20.0,
            )
            .unwrap();
        source.disable_stroke().unwrap();
        source.set_fill(0.25, 0.5, 0.75, 0.6).unwrap();
        source
            .add_effect(
                Shadow::new(
                    EffectUpdate::default()
                        .radius(Pixels(2.25))
                        .offset(EffectOffset::Pixels(offset))
                        .color(Color::rgba(0.8, 0.1, 0.3, 0.75)),
                )
                .unwrap(),
                "depth",
            )
            .unwrap();
        scene.add(&source).unwrap();
        let mut foreground = scene
            .rectangle(
                f64::from(front[2] - front[0]) / 10.0,
                f64::from(front[3] - front[1]) / 10.0,
            )
            .unwrap();
        foreground
            .set_translation(
                f64::from(front[0] + front[2]) / 20.0 - 4.0,
                3.0 - f64::from(front[1] + front[3]) / 20.0,
            )
            .unwrap();
        foreground.disable_stroke().unwrap();
        foreground.set_fill(0.0, 1.0, 0.0, 1.0).unwrap();
        scene.add(&foreground).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut surface = Surface::new(&device, &queue);
        let actual = surface.render_on(
            &device,
            &queue,
            &mut session,
            wgpu::Color {
                r: f64::from(background[0]) / 255.0,
                g: f64::from(background[1]) / 255.0,
                b: f64::from(background[2]) / 255.0,
                a: 1.0,
            },
        );
        assert!(
            error(&actual, &expected) <= 1,
            "offscreen Shadow {bounds:?}: error {}",
            error(&actual, &expected)
        );
        assert!(
            error(&expected, &unoccluded) > 100,
            "foreground must witness painter order"
        );
        assert!(
            actual
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| *p != background && *p != [0, 255, 0, 255]),
            "fully offscreen source must cast a visible shadow"
        );
    }
    eprintln!("offscreen Shadow cases=2 max_rgba8_error<=1 outside_source=true extended_reference=true foreground_order=true");
}

#[test]
#[ignore = "requires a native raster adapter; absence is a failure, never a successful skip"]
fn shadow_scene_and_output_units_reprepare_under_view_changes() {
    use noon::effects::{EffectOffset, EffectRadius, Shadow};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut maximum_error = 0;
    for pixels in [true, false] {
        let (plain_scene, _, _) = blur_fixture(2, false, 1.0, pixels);
        let mut plain = plain_scene.execution_session().unwrap();
        let (scene, mut source, _) = blur_fixture(2, false, 0.37, pixels);
        source
            .add_effect(
                Shadow::new(
                    EffectUpdate::default()
                        .radius(if pixels {
                            EffectRadius::Pixels(1.25)
                        } else {
                            EffectRadius::Scene(0.125)
                        })
                        .offset(if pixels {
                            EffectOffset::Pixels([4.25, -3.5])
                        } else {
                            EffectOffset::Scene([0.425, -0.35])
                        }),
                )
                .unwrap(),
                "depth",
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut plain_surface = Surface::new(&device, &queue);
        let mut surface = Surface::new(&device, &queue);
        let context = session.publication_context();
        for height in [6.0_f32, 3.0, 6.0] {
            let camera = Camera2D::new(Vec2::ZERO, Vec2::new(height * 4.0 / 3.0, height)).unwrap();
            plain_surface.renderer.set_camera(&queue, camera);
            surface.renderer.set_camera(&queue, camera);
            let ordinary =
                plain_surface.render_on(&device, &queue, &mut plain, wgpu::Color::TRANSPARENT);
            let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
            let scale = if pixels { 1.0 } else { 6.0 / f64::from(height) };
            let expected = expected_shadow(
                &ordinary,
                1.25 * scale,
                [4.25 * scale, -3.5 * scale],
                [0.0, 0.0, 0.0, 0.5],
                f64::from(0.37_f32),
            );
            let delta = error(&actual, &expected);
            maximum_error = maximum_error.max(delta);
            assert!(
                delta <= 1,
                "Shadow view height={height} pixels={pixels}: error {delta}"
            );
            assert_eq!(session.publication_context(), context);
            assert_eq!(
                surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                actual
            );
            assert_eq!(surface.effect_stats.source_passes, 0);
            assert_eq!(surface.effect_stats.blur_passes, 0);
            assert_eq!(surface.effect_stats.composite_passes, 0);
        }
    }
    eprintln!("Shadow view samples=6 max_rgba8_error={maximum_error} scene_units=true output_units=true no_semantic_mutation=true");
}

/// Literal physical fitted rectangles and a standalone reconstruction kernel.
/// This reference never calls ImageMapping or the engine's CPU pixel sampler.
#[test]
#[ignore = "requires a real native raster adapter; absence is a failure"]
fn public_image_fill_pixels_fit_uv_filter_alpha_and_texture_reuse() {
    use noon::{
        ImageAddressMode as Address, ImageFit as Fit, ImageMapping, ImagePaintOptions,
        ImageUvTransform, RasterImageSampling as Sampling,
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("image paint adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let bytes = vec![
        255, 0, 0, 255, 0, 255, 255, 0, 0, 0, 255, 128, 240, 100, 10, 64, 20, 240, 100, 192, 150,
        20, 240, 255,
    ];
    let mut profiles = 0;
    let mut channels = 0;
    let mut max_error = 0;
    for geometry in 0..3 {
        for fit in [Fit::Stretch, Fit::Cover, Fit::Contain] {
            for address in [Address::Transparent, Address::Clamp, Address::Repeat] {
                for sampling in [Sampling::Nearest, Sampling::Linear, Sampling::Bicubic] {
                    let mut scene = Scene::new();
                    let mut source = match geometry {
                        0 => scene.rectangle(4.0, 2.0).unwrap(),
                        1 => scene.circle(2.0).unwrap(),
                        _ => scene
                            .path(
                                noon_core::VectorPath::new()
                                    .move_to(Vec2::new(-2.0, -1.0))
                                    .line_to(Vec2::new(2.0, -1.0))
                                    .line_to(Vec2::new(1.0, 1.0))
                                    .line_to(Vec2::new(-2.0, 1.0))
                                    .close(),
                                noon_core::SemanticStyle::default(),
                            )
                            .unwrap(),
                    };
                    source.disable_stroke().unwrap();
                    source.set_object_opacity(0.6).unwrap();
                    let mut options = ImagePaintOptions::rgba8(3, 2, bytes.clone()).unwrap();
                    options.set_sampling(sampling);
                    source.set_fill_image(options, Some(0.7)).unwrap();
                    let mut bitmap = scene.image_rgba8(3, 2, bytes.clone()).unwrap();
                    bitmap.shift(20.0, 20.0).unwrap();
                    scene.add(&bitmap).unwrap();
                    scene.add(&source).unwrap();
                    let mut session = scene.execution_session().unwrap();
                    let mut surface = Surface::new(&device, &queue);
                    for pose in 0..4 {
                        let (translation, scale, angle) = match pose {
                            0 => ([0.0, 0.0], [1.0, 1.0], 0.0_f64),
                            1 => ([0.173, -0.231], [0.63, 1.37], 0.37_f64),
                            _ => ([-0.29, 0.413], [-0.71, 0.58], -0.61_f64),
                        };
                        let mapping = ImageMapping::new(
                            fit,
                            [0.23, 0.71],
                            address,
                            ImageUvTransform::new(translation, scale, angle).unwrap(),
                        )
                        .unwrap();
                        scene
                            .live(&mut session)
                            .set_fill_image_mapping(&source, mapping, Some(0.7))
                            .unwrap();
                        if pose == 3 {
                            let mut live = scene.live(&mut session);
                            live.set_scale(&source, -0.9, 1.2).unwrap();
                            live.set_rotation(&source, 0.413).unwrap();
                            live.shift(&source, 0.17, -0.23).unwrap();
                        }
                        let pixels = surface.render_on(
                            &device,
                            &queue,
                            &mut session,
                            wgpu::Color::TRANSPARENT,
                        );
                        assert_eq!(
                            surface.upload_stats.images.textures_uploaded,
                            usize::from(pose == 0)
                        );
                        assert_eq!(
                            surface.upload_stats.images.pixel_bytes_uploaded,
                            if pose == 0 { 24 } else { 0 }
                        );
                        assert_eq!(
                            surface.render_on(
                                &device,
                                &queue,
                                &mut session,
                                wgpu::Color::TRANSPARENT
                            ),
                            pixels
                        );
                        assert_eq!(surface.upload_stats.images, Default::default());
                        let size = [4.0, if geometry == 1 { 4.0 } else { 2.0 }];
                        for y in (13..47).step_by(3) {
                            for x in (20..60).step_by(3) {
                                let mut local = [
                                    (f64::from(x) + 0.5 - 40.0) / 10.0,
                                    (30.0 - f64::from(y) - 0.5) / 10.0,
                                ];
                                if pose == 3 {
                                    let dx = local[0] - 0.17;
                                    let dy = local[1] + 0.23;
                                    local = [
                                        (0.413_f64.cos() * dx + 0.413_f64.sin() * dy) / -0.9,
                                        (-0.413_f64.sin() * dx + 0.413_f64.cos() * dy) / 1.2,
                                    ];
                                }
                                let interior = if geometry == 1 {
                                    local[0] * local[0] + local[1] * local[1] < 3.0
                                } else if geometry == 2 {
                                    local[0] > -1.75
                                        && local[1].abs() < 0.75
                                        && local[0] < 1.5 - 0.5 * local[1] - 0.25
                                } else {
                                    local[0].abs() < 1.75 && local[1].abs() < 0.75
                                };
                                if !interior {
                                    continue;
                                }
                                let normalized =
                                    [local[0] / size[0] + 0.5, local[1] / size[1] + 0.5];
                                let delta = [
                                    normalized[0] - 0.5 - translation[0],
                                    normalized[1] - 0.5 - translation[1],
                                ];
                                let placed = [
                                    (angle.cos() * delta[0] + angle.sin() * delta[1]) / scale[0]
                                        + 0.5,
                                    (-angle.sin() * delta[0] + angle.cos() * delta[1]) / scale[1]
                                        + 0.5,
                                ];
                                let factor = match fit {
                                    Fit::Stretch => 1.0,
                                    Fit::Cover => (size[0] / 3.0).max(size[1] / 2.0),
                                    Fit::Contain => (size[0] / 3.0).min(size[1] / 2.0),
                                };
                                let physical = if fit == Fit::Stretch {
                                    size
                                } else {
                                    [3.0 * factor, 2.0 * factor]
                                };
                                let lower = [
                                    (size[0] - physical[0]) * 0.23,
                                    (size[1] - physical[1]) * 0.71,
                                ];
                                let uv = [
                                    (placed[0] * size[0] - lower[0]) / physical[0],
                                    1.0 - (placed[1] * size[1] - lower[1]) / physical[1],
                                ];
                                let mut expected = [0.0; 4];
                                if fit != Fit::Contain || uv.iter().all(|v| (0.0..=1.0).contains(v))
                                {
                                    expected =
                                        independent_image_sample(&bytes, uv, sampling, address);
                                }
                                let expected = expected.map(|v| {
                                    (((v * 0.7 * 0.6).clamp(0.0, 1.0) * 65535.0 + 0.5).floor()
                                        / 256.0)
                                        .floor() as u8
                                });
                                let start = ((y * SIZE[0] + x) * 4) as usize;
                                for (actual, expected) in
                                    pixels[start..start + 4].iter().zip(expected)
                                {
                                    let error = actual.abs_diff(expected);
                                    max_error = max_error.max(error);
                                    channels += 1;
                                    assert!(error<=1, "geometry={geometry} fit={fit:?} address={address:?} sampling={sampling:?} pose={pose} ({x},{y}) uv={uv:?} actual={:?} expected={expected}", &pixels[start..start+4]);
                                }
                            }
                        }
                        profiles += 1;
                    }
                    assert_eq!(surface.renderer.image_residency_stats().textures, 1);
                    assert_eq!(surface.renderer.image_residency_stats().pixel_bytes, 24);
                    scene.live(&mut session).remove(&bitmap).unwrap();
                    surface.render(&device, &queue, &mut session);
                    assert_eq!(surface.upload_stats.images.textures_retired, 0);
                    assert_eq!(surface.renderer.image_residency_stats().textures, 1);
                    scene
                        .live(&mut session)
                        .set_fill(&source, 0.2, 0.4, 0.8, 1.0)
                        .unwrap();
                    surface.render(&device, &queue, &mut session);
                    assert_eq!(surface.upload_stats.images.textures_retired, 1);
                    assert_eq!(surface.renderer.image_residency_stats().textures, 0);
                }
            }
        }
    }
    eprintln!("image fill independent reference: {profiles} profiles, {channels} channels, max byte error {max_error}");
}

fn independent_image_sample(
    bytes: &[u8],
    uv: [f64; 2],
    sampling: noon::RasterImageSampling,
    address: noon::ImageAddressMode,
) -> [f64; 4] {
    use noon::{ImageAddressMode as Address, RasterImageSampling as Sampling};
    let tap = |x: i32, y: i32| -> [f64; 4] {
        let (x, y) = match address {
            Address::Transparent if !(0..3).contains(&x) || !(0..2).contains(&y) => {
                return [0.0; 4]
            }
            Address::Repeat => (x.rem_euclid(3), y.rem_euclid(2)),
            _ => (x.clamp(0, 2), y.clamp(0, 1)),
        };
        let i = ((y * 3 + x) * 4) as usize;
        let a = f64::from(bytes[i + 3]) / 255.0;
        [
            f64::from(bytes[i]) / 255.0 * a,
            f64::from(bytes[i + 1]) / 255.0 * a,
            f64::from(bytes[i + 2]) / 255.0 * a,
            a,
        ]
    };
    if sampling == Sampling::Nearest {
        return tap((uv[0] * 3.0).floor() as i32, (uv[1] * 2.0).floor() as i32);
    }
    let position = [uv[0] * 3.0 - 0.5, uv[1] * 2.0 - 0.5];
    let base = position.map(|v| v.floor() as i32);
    let fraction = [
        position[0] - f64::from(base[0]),
        position[1] - f64::from(base[1]),
    ];
    let kernel = |distance: f64| {
        let t = distance.abs();
        if sampling == Sampling::Linear {
            (1.0 - t).max(0.0)
        } else if t <= 1.0 {
            t.powi(3) - 2.0 * t.powi(2) + 1.0
        } else if t < 2.0 {
            -t.powi(3) + 5.0 * t.powi(2) - 8.0 * t + 4.0
        } else {
            0.0
        }
    };
    let mut result = [0.0; 4];
    for y in -1..=2 {
        for x in -1..=2 {
            let color = tap(base[0] + x, base[1] + y);
            let weight = kernel(f64::from(x) - fraction[0]) * kernel(f64::from(y) - fraction[1]);
            for channel in 0..4 {
                result[channel] += color[channel] * weight;
            }
        }
    }
    result[3] = result[3].clamp(0.0, 1.0);
    for channel in 0..3 {
        result[channel] = result[channel].clamp(0.0, result[3]);
    }
    result
}

#[test]
#[ignore = "requires a real native raster adapter; absence is a failure"]
fn public_image_fill_reuses_source_capture_for_blur_shadow_and_glow() {
    continuous_fill_filter_capture(ContinuousFillFixture::Image);
}

#[test]
#[ignore = "requires a real native raster adapter; absence is a failure"]
fn public_noise_fill_reuses_source_capture_for_blur_shadow_and_glow() {
    continuous_fill_filter_capture(ContinuousFillFixture::Noise);
}

#[test]
#[ignore = "requires a real native raster adapter; absence is a failure"]
fn public_hatch_fill_reuses_source_capture_for_blur_shadow_and_glow() {
    for cross in [false, true] {
        continuous_fill_filter_capture(ContinuousFillFixture::Hatch { cross });
    }
}

#[derive(Clone, Copy, Debug)]
enum ContinuousFillFixture {
    Image,
    Noise,
    Hatch { cross: bool },
}

fn continuous_fill_filter_capture(fixture: ContinuousFillFixture) {
    use noon::effects::{Blur, EffectOffset, Glow, GlowSource, Shadow};
    use noon::{ImageAddressMode, ImageFit, ImageMapping, ImagePaintOptions, ImageUvTransform};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let make = |scope: f64, silhouette: bool| {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.8).unwrap();
        source.disable_stroke().unwrap();
        if silhouette {
            source.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
        } else {
            match fixture {
                ContinuousFillFixture::Noise => source
                    .set_fill_noise(
                        noon::NoisePaint::new(
                            7,
                            [16.0, 12.0],
                            [-0.173, 0.231],
                            [
                                Color::rgba(1.0, 0.0, 0.0, 0.2),
                                Color::rgba(0.0, 0.0, 1.0, 0.9),
                            ],
                        )
                        .unwrap(),
                        Some(0.73),
                    )
                    .unwrap(),
                ContinuousFillFixture::Image => source
                    .set_fill_image(
                        ImagePaintOptions::rgba8(3, 2, noon::example_scenes::image_fill::pixels())
                            .unwrap(),
                        Some(0.73),
                    )
                    .unwrap(),
                ContinuousFillFixture::Hatch { cross } => source
                    .set_fill_hatch(
                        noon::HatchPaint::new(
                            Color::rgba(1.0, 0.0, 0.0, 0.2),
                            Color::rgba(0.0, 0.0, 1.0, 0.9),
                            noon::HatchPaintOptions {
                                spacing: 0.4,
                                width: 0.12,
                                angle: 0.37,
                                phase: [-0.173, 0.231],
                                cross,
                            },
                        )
                        .unwrap(),
                        Some(0.73),
                    )
                    .unwrap(),
            }
        }
        if matches!(fixture, ContinuousFillFixture::Hatch { .. }) {
            source.scale(-0.9, 1.2).unwrap();
            source.rotate(0.413).unwrap();
            source.shift(0.17, -0.23).unwrap();
        }
        source.set_object_opacity(scope).unwrap();
        scene.add(&source).unwrap();
        (scene, source)
    };
    let mut samples = 0;
    let mut maximum_byte_error = 0;
    for kind in 0..4 {
        let (plain_scene, plain_source) = make(1.0, false);
        let mut plain = plain_scene.execution_session().unwrap();
        let mut reference = Surface::new(&device, &queue);
        let (mask_scene, _) = make(1.0, true);
        let mut mask = mask_scene.execution_session().unwrap();
        let mask_pixels = Surface::new(&device, &queue).render_on(
            &device,
            &queue,
            &mut mask,
            wgpu::Color::TRANSPARENT,
        );
        let (scene, mut source) = make(0.37, false);
        let definition: noon::effects::EffectDefinition = match kind {
            0 => Blur::new(EffectUpdate::default().radius(Pixels(3.25)))
                .unwrap()
                .into(),
            1 => Shadow::new(
                EffectUpdate::default()
                    .radius(Pixels(3.25))
                    .offset(EffectOffset::Pixels([2.5, -1.25]))
                    .color(Color::rgba(0.2, 0.7, 0.8, 0.6)),
            )
            .unwrap()
            .into(),
            _ => Glow::new(
                EffectUpdate::default()
                    .radius(Pixels(3.25))
                    .color(Color::RED)
                    .intensity(START)
                    .source(if kind == 2 {
                        GlowSource::Painted
                    } else {
                        GlowSource::Silhouette
                    }),
            )
            .unwrap()
            .into(),
        };
        source.add_effect(definition, "surface").unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut surface = Surface::new(&device, &queue);
        for stage in 0..2 {
            if stage == 1 && matches!(fixture, ContinuousFillFixture::Noise) {
                let paint = noon::NoisePaint::new(
                    7,
                    [64.0, 64.0],
                    [0.531, -0.719],
                    [
                        Color::rgba(1.0, 0.0, 0.0, 0.2),
                        Color::rgba(0.0, 0.0, 1.0, 0.9),
                    ],
                )
                .unwrap();
                scene
                    .live(&mut session)
                    .set_fill_noise(&source, paint.clone(), None)
                    .unwrap();
                plain_scene
                    .live(&mut plain)
                    .set_fill_noise(&plain_source, paint, None)
                    .unwrap();
            } else if let (1, ContinuousFillFixture::Hatch { cross }) = (stage, fixture) {
                let paint = noon::HatchPaint::new(
                    Color::rgba(1.0, 0.0, 0.0, 0.2),
                    Color::rgba(0.0, 0.0, 1.0, 0.9),
                    noon::HatchPaintOptions {
                        spacing: 0.06,
                        width: 0.021,
                        angle: 0.67,
                        phase: [0.531, -0.719],
                        cross,
                    },
                )
                .unwrap();
                scene
                    .live(&mut session)
                    .set_fill_hatch(&source, paint.clone(), None)
                    .unwrap();
                plain_scene
                    .live(&mut plain)
                    .set_fill_hatch(&plain_source, paint, None)
                    .unwrap();
            } else if stage == 1 {
                let mapping = ImageMapping::new(
                    ImageFit::Cover,
                    [0.5; 2],
                    ImageAddressMode::Clamp,
                    ImageUvTransform::new([0.173, -0.231], [0.63, 1.37], 0.37).unwrap(),
                )
                .unwrap();
                scene
                    .live(&mut session)
                    .set_fill_image_mapping(&source, mapping, None)
                    .unwrap();
                plain_scene
                    .live(&mut plain)
                    .set_fill_image_mapping(&plain_source, mapping, None)
                    .unwrap();
            }
            let ordinary =
                reference.render_on(&device, &queue, &mut plain, wgpu::Color::TRANSPARENT);
            let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
            let expected = match kind {
                0 => expected_rgba_blur(&ordinary, 3.25, 0.37),
                1 => expected_shadow(&ordinary, 3.25, [2.5, -1.25], [0.2, 0.7, 0.8, 0.6], 0.37),
                _ => expected_image_glow(
                    &ordinary,
                    if kind == 2 { &ordinary } else { &mask_pixels },
                    0.0,
                    0.37,
                    wgpu::Color::TRANSPARENT,
                ),
            };
            let byte_error = error(&actual, &expected);
            maximum_byte_error = maximum_byte_error.max(byte_error);
            assert!(
                byte_error <= 1,
                "{fixture:?} fill filter={kind} stage={stage}, max byte error {byte_error}"
            );
            // Silhouette Glow retains the painted source and a separate geometry mask.
            assert_eq!(
                surface.effect_stats.source_passes,
                1 + usize::from(kind == 3)
            );
            if stage == 1 {
                assert_eq!(surface.upload_stats.images, Default::default());
            }
            assert_eq!(
                surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
                actual
            );
            assert_eq!(surface.effect_stats.source_passes, 0);
            assert_eq!(surface.effect_stats.blur_passes, 0);
            assert_eq!(surface.effect_stats.composite_passes, 0);
            assert_eq!(surface.effect_stats.bytes_uploaded, 0);
            assert_eq!(surface.effect_stats.texture_allocations, 0);
            assert_eq!(surface.upload_stats.bytes_uploaded(), 0);
            samples += 1;
        }
        scene
            .live(&mut session)
            .remove_effect(&source, "surface")
            .unwrap();
        surface.render(&device, &queue, &mut session);
        assert_eq!(surface.renderer.effect_capture_texture_bytes(), 0);
    }
    eprintln!("{fixture:?} filter/composition qualification from ordinary source: {samples} whole-image comparisons, max byte error {maximum_byte_error} (bound 1)");
}

#[test]
#[ignore = "requires a real native raster adapter; absence is a failure"]
fn public_image_fill_preserves_ordinary_geometry_coverage_and_solid_strokes() {
    continuous_fill_coverage_and_strokes(ContinuousFillFixture::Image);
}

#[test]
#[ignore = "requires a real native raster adapter; absence is a failure"]
fn public_noise_fill_preserves_ordinary_geometry_coverage_and_solid_strokes() {
    continuous_fill_coverage_and_strokes(ContinuousFillFixture::Noise);
}

#[test]
#[ignore = "requires a real native raster adapter; absence is a failure"]
fn public_hatch_fill_preserves_ordinary_geometry_coverage_and_solid_strokes() {
    for cross in [false, true] {
        continuous_fill_coverage_and_strokes(ContinuousFillFixture::Hatch { cross });
    }
}

fn continuous_fill_coverage_and_strokes(fixture: ContinuousFillFixture) {
    use noon::ImagePaintOptions;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut images = 0;
    for kind in 0..3 {
        for opaque in [false, true] {
            let make = |image: bool| {
                let mut scene = Scene::new();
                let mut source = match kind {
                    0 => scene.circle(0.8).unwrap(),
                    1 => scene.rectangle(2.0, 1.5).unwrap(),
                    _ => scene
                        .path(
                            noon_core::VectorPath::new()
                                .move_to(Vec2::new(-1.0, -1.0))
                                .line_to(Vec2::new(1.0, -1.0))
                                .line_to(Vec2::new(0.2, 0.0))
                                .line_to(Vec2::new(1.0, 1.0))
                                .line_to(Vec2::new(-1.0, 1.0))
                                .close(),
                            noon_core::SemanticStyle::default(),
                        )
                        .unwrap(),
                };
                if image {
                    let rgba = if opaque { [255; 4] } else { [255, 0, 255, 0] };
                    let color = if opaque {
                        Color::WHITE
                    } else {
                        Color::rgba(1.0, 0.0, 1.0, 0.0)
                    };
                    match fixture {
                        ContinuousFillFixture::Noise => source
                            .set_fill_noise(
                                noon::NoisePaint::new(7, [8.0; 2], [-0.173, 0.231], [color; 2])
                                    .unwrap(),
                                Some(0.73),
                            )
                            .unwrap(),
                        ContinuousFillFixture::Image => source
                            .set_fill_image(
                                ImagePaintOptions::rgba8(1, 1, rgba).unwrap(),
                                Some(0.73),
                            )
                            .unwrap(),
                        ContinuousFillFixture::Hatch { cross } => source
                            .set_fill_hatch(
                                noon::HatchPaint::new(
                                    color,
                                    color,
                                    noon::HatchPaintOptions {
                                        phase: [-0.173, 0.231],
                                        cross,
                                        ..Default::default()
                                    },
                                )
                                .unwrap(),
                                Some(0.73),
                            )
                            .unwrap(),
                    }
                } else if opaque {
                    source.set_fill(1.0, 1.0, 1.0, 0.73).unwrap();
                } else {
                    source.disable_fill().unwrap();
                }
                source.set_stroke_color(0.1, 0.8, 0.2, 0.67).unwrap();
                source.set_stroke_width(0.18).unwrap();
                source.set_object_opacity(0.37).unwrap();
                source.scale(-0.9, 1.2).unwrap();
                source.rotate(0.413).unwrap();
                source.shift(0.17, -0.23).unwrap();
                scene.add(&source).unwrap();
                scene
            };
            let mut image = make(true).execution_session().unwrap();
            let mut ordinary = make(false).execution_session().unwrap();
            let image = Surface::new(&device, &queue).render_on(
                &device,
                &queue,
                &mut image,
                wgpu::Color::TRANSPARENT,
            );
            let ordinary = Surface::new(&device, &queue).render_on(
                &device,
                &queue,
                &mut ordinary,
                wgpu::Color::TRANSPARENT,
            );
            assert_eq!(
                image, ordinary,
                "{fixture:?} paint changed geometry/stroke kind={kind} opaque={opaque}"
            );
            assert!(
                image
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[1] > pixel[0] && pixel[1] > pixel[2]),
                "fixture must contain a visible green stroke"
            );
            images += 1;
        }
    }
    eprintln!("{fixture:?} paint coverage/strokes: {images} byte-identical whole images");
}
