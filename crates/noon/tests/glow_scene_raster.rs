//! Public authoring -> Scene execution -> ordinary retained wgpu rendering.
//! The independent filter reference uses only a separate no-effect Scene image.
use noon::{
    effects::{GlowUpdate, Pixels},
    AnimationOptions, Color, ExecutionSession, Mobject, RateFunction, Scene,
};
use noon_core::Vec2;
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState,
};
use std::time::Duration;

const SIZE: [u32; 2] = [80, 60];
const START: f64 = 0.4000000000000123;

fn scene(glow: bool) -> (Scene, Mobject, noon::DeclaredAnimation) {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    source.disable_stroke().unwrap();
    source.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    if glow {
        source
            .set_glow(
                GlowUpdate::default()
                    .color(Color::RED)
                    .radius(Pixels(3.25))
                    .intensity(START),
            )
            .unwrap();
    }
    scene.add(&source).unwrap();
    let mut target = source.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    if glow {
        target
            .set_glow(
                GlowUpdate::default()
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
}
impl Surface {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::with_logical_viewport(device, queue, Vec2::new(80.0, 60.0))
    }
    fn with_logical_viewport(device: &wgpu::Device, queue: &wgpu::Queue, logical: Vec2) -> Self {
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer
            .set_viewport_with_logical_size(device, queue, SIZE[0], SIZE[1], logical)
            .unwrap();
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(6.0 * logical.x / logical.y, 6.0)).unwrap(),
        );
        let text = renderer.create_retained_text_state(device, queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("public glow Scene qualification"),
            size: wgpu::Extent3d {
                width: SIZE[0],
                height: SIZE[1],
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
        let visible = publication
            .painter_order()
            .iter()
            .map(|&i| i as usize)
            .collect::<Vec<_>>();
        let visible = self
            .renderer
            .glow_source_visibility(&publication, &visible)
            .unwrap();
        let metrics = TextDeviceMetrics::uniform(10.0)
            .unwrap()
            .with_world_origin_pixels(Vec2::new(40.0, 30.0))
            .unwrap();
        let prepared = self
            .preparer
            .prepare_planned_publication_visible(device, &publication, visible, metrics)
            .unwrap();
        self.renderer
            .upload_retained(device, queue, &prepared, &mut self.text);
        let mut encoder = device.create_command_encoder(&Default::default());
        self.renderer
            .prepare_retained_analytic_glows(
                device,
                queue,
                &mut encoder,
                &prepared,
                &self.text,
                &publication,
                1_000_000,
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
    let (width, height) = (SIZE[0] as usize, SIZE[1] as usize);
    assert_eq!(ordinary.len(), width * height * 4);
    let sigma = 3.25 + 3.25 * time;
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
    let intensity = START + (1.4 - START) * time;
    // RED/BLUE are the authored Manim palette constants, not unit RGB primaries.
    // Interpolate their endpoint components independently from runtime output.
    let from = Color::RED;
    let to = Color::BLUE;
    let tint = [
        f64::from(from.red) + f64::from(to.red - from.red) * time,
        f64::from(from.green) + f64::from(to.green - from.green) * time,
        f64::from(from.blue) + f64::from(to.blue - from.blue) * time,
    ];
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
        .set_glow(&source, GlowUpdate::default().intensity(0.0))
        .unwrap();
    assert_eq!(
        surface.render(&device, &queue, &mut session),
        last_plain,
        "neutral is ordinary, byte for byte"
    );
    scene
        .live(&mut session)
        .set_glow(&source, GlowUpdate::default().intensity(1.4))
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
            GlowUpdate::default()
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
            .set_glow(&source, GlowUpdate::default().intensity(0.0))
            .unwrap();
        assert_eq!(surface.render(&device, &queue, &mut session), last_plain);
        scene.live(&mut session).remove_glow(&source).unwrap();
        assert_eq!(surface.render(&device, &queue, &mut session), last_plain);
        eprintln!("LOGICAL_VIEWPORT_GLOW_PASS logical={logical:?} max_error<=2 neutral_exact=true removal_exact=true");
    }
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
                GlowUpdate::default()
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
                .set_glow(&source, GlowUpdate::default().intensity(0.0))
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
            assert_eq!(surface.renderer.analytic_glow_texture_bytes(), 0);
        }
    }
    eprintln!("GENERIC_SOURCE_GLOW_PASS cases=8 reveal_frames=40 max_error={max_error} opacity_once=true neutral_exact=true retirement=true");
}

fn image_scene(
    glow: Option<noon::effects::GlowSource>,
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
    if let Some(mode) = glow {
        source
            .set_glow(
                GlowUpdate::default()
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
    if glow.is_some() {
        target
            .set_glow(
                GlowUpdate::default()
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
            .set_glow(&source, GlowUpdate::default().intensity(0.0))
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
                .set_glow(&source, GlowUpdate::default().intensity(0.0))
                .unwrap();
            assert_eq!(
                surface.render_on(&device, &queue, &mut session, background),
                plain_pixels,
                "neutral effect changed ordinary image"
            );
            scene
                .live(&mut session)
                .set_glow(&source, GlowUpdate::default().intensity(1.4))
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
            assert_eq!(surface.renderer.analytic_glow_texture_bytes(), 0);
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
    glow: Option<noon::effects::GlowSource>,
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
    if let Some(source) = glow {
        image.set_object_opacity(EDGE_IMAGE_SCOPE).unwrap();
        image
            .set_glow(
                GlowUpdate::default()
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
