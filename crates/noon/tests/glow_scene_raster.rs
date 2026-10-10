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
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(device, queue, SIZE[0], SIZE[1]);
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
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
                &publication,
                1_000_000,
            )
            .unwrap();
        self.renderer
            .encode_retained(
                &mut encoder,
                &self.target.create_view(&Default::default()),
                &prepared,
                &self.text,
                wgpu::Color::BLACK,
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
}
