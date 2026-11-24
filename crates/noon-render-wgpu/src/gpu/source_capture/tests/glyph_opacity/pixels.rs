//! Native glyph capture compared to ordinary unfiltered glyph rendering.
use super::*;
use crate::RetainedTextGpuState;

struct Raster {
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    target: wgpu::Texture,
    multisampled: wgpu::Texture,
}
impl Raster {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(device, queue, SIZE[0], SIZE[1]);
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(4.0, 3.0)).unwrap(),
        );
        let text = renderer.create_retained_text_state(device, queue);
        Self {
            renderer,
            preparer: RetainedFramePreparer::new(),
            text,
            target: target(device, 1),
            multisampled: target(device, PATH_SAMPLE_COUNT),
        }
    }
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        runtime: &mut SceneInstance,
        capture: Option<bool>,
    ) -> Vec<u8> {
        let publication = runtime.take_renderer_publication();
        let prepared = self
            .preparer
            .prepare_planned_publication_visible(
                device,
                &publication,
                &[0, 1],
                TextDeviceMetrics::uniform(20.0)
                    .unwrap()
                    .with_world_origin_pixels(Vec2::new(40.0, 30.0))
                    .unwrap(),
            )
            .unwrap();
        self.renderer
            .upload_retained(device, queue, &prepared, &mut self.text);
        let mut encoder = device.create_command_encoder(&Default::default());
        if let Some(silhouette) = capture {
            prepared.text.atlas.encode_pending_uploads(&mut encoder);
            let source = PreparedSource::prepare(
                &prepared,
                &[(ObjectId::new(1), 1.0), (ObjectId::new(2), 1.0)],
                silhouette,
                device.limits().min_uniform_buffer_offset_alignment as usize,
            )
            .unwrap();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("glyph opacity capture"),
                size: source.bytes.len().next_power_of_two() as u64,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::UNIFORM
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buffer, 0, &source.bytes);
            let text_camera = self
                .text
                .glyphs
                .capture_camera_binding(device, &self.renderer.camera_buffer);
            source
                .encode(
                    &self.renderer,
                    &self.text,
                    &prepared.geometry,
                    &mut encoder,
                    &self.target.create_view(&Default::default()),
                    &self.multisampled.create_view(&Default::default()),
                    &self.renderer.camera_bind_group,
                    &text_camera,
                    &buffer,
                    &[],
                    None,
                    SIZE,
                )
                .unwrap();
        } else {
            self.renderer
                .encode_retained(
                    &mut encoder,
                    &self.target.create_view(&Default::default()),
                    &prepared,
                    &self.text,
                    wgpu::Color::TRANSPARENT,
                    None,
                )
                .unwrap();
        }
        queue.submit([encoder.finish()]);
        if capture.is_some() {
            prepared.text.atlas.acknowledge_pending_uploads();
        }
        read(device, queue, &self.target)
    }
}
fn set(runtime: &mut SceneInstance, opacity: f32, alpha: f32, white: bool) {
    runtime
        .apply_execution_patch(&ExecutionPatch::SetStyle {
            object: ObjectId::new(2),
            style: Style {
                fill: Some(if white {
                    Color::WHITE
                } else {
                    Color::rgba(0.9, 0.7, 0.1, alpha)
                }),
                stroke: None,
                opacity,
                ..Style::default()
            },
            fill_paint: None,
        })
        .unwrap();
}
fn retain(step: usize, silhouette: bool, actual: &[u8], expected: &[u8], opacity: f32, alpha: f32) {
    let Ok(root) = std::env::var("NOON_GLYPH_COVERAGE_ARTIFACT_DIR") else {
        return;
    };
    let path = std::path::Path::new(&root).join(format!("step-{step}-silhouette-{silhouette}"));
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("actual.rgba"), actual).unwrap();
    std::fs::write(path.join("expected.rgba"), expected).unwrap();
    std::fs::write(path.join("profile.json"),format!(r#"{{"width":80,"height":60,"bound":2,"childOpacity":{opacity},"paintAlpha":{alpha},"silhouette":{silhouette},"reference":"ordinary unfiltered glyph rendering; white paint for original coverage, preserving child opacity"}}"#)).unwrap();
}
#[test]
#[ignore = "requires a real raster adapter; grouped glyph coverage and ordinary painter parity"]
fn glyph_capture_pixels_preserve_child_opacity_and_ignore_paint_alpha() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("glyph capture adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let compiled = compiled_scene();
    let mut source = SceneInstance::new(compiled.clone());
    let mut painted = SceneInstance::new(compiled.clone());
    let mut original = SceneInstance::new(compiled);
    for runtime in [&mut source, &mut painted, &mut original] {
        for (id, x) in [(1, -0.2), (2, 0.2)] {
            runtime
                .apply_execution_patch(&ExecutionPatch::SetTransform {
                    object: ObjectId::new(id),
                    transform: Transform2D {
                        translation: Vec2::new(x, 0.0),
                        ..Transform2D::IDENTITY
                    },
                })
                .unwrap();
        }
    }
    let mut capture = Raster::new(&device, &queue);
    let mut ordinary = Raster::new(&device, &queue);
    let mut maximum = 0;
    let mut first = Vec::new();
    for (step, (opacity, alpha)) in [(0.7, 0.4), (0.25, 0.0), (0.0, 0.6), (0.65, 1.0), (0.7, 0.4)]
        .into_iter()
        .enumerate()
    {
        set(&mut source, opacity, alpha, false);
        set(&mut painted, opacity, alpha, false);
        set(&mut original, opacity, alpha, true);
        for silhouette in [false, true] {
            let actual = capture.render(&device, &queue, &mut source, Some(silhouette));
            let expected = ordinary.render(
                &device,
                &queue,
                if silhouette {
                    &mut original
                } else {
                    &mut painted
                },
                None,
            );
            assert!(
                expected.iter().skip(3).step_by(4).any(|&a| a > 0),
                "nonblank glyph fixture required"
            );
            let error = maximum_error(&actual, &expected);
            maximum = maximum.max(error);
            retain(step, silhouette, &actual, &expected, opacity, alpha);
            assert!(
                error <= 2,
                "glyph capture step {step} silhouette={silhouette}: maximum byte error {error}"
            );
            if step == 0 {
                first.push(actual.clone());
            }
            if step == 4 {
                assert_eq!(
                    actual,
                    first[usize::from(silhouette)],
                    "restored source must be pixel exact"
                );
            }
        }
    }
    eprintln!("glyph capture: 10 full-image comparisons / 192000 channels, max byte error {maximum}, exact restoration; ordinary draws are the shared source-raster reference");
}
