//! End-to-end text color/composition proof. The ordinary compatibility rendering
//! of the same white text supplies qualified glyph coverage only; this test does
//! not claim a new independent font rasterizer. Color decode/source-over is f64.
use super::super::{decode, read, srgb_decode};
use noon::{ExecutionSession, Mobject, Scene, Text};
use noon_core::{SemanticMutationTransaction, Vec2, ViewColorProfile};
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState,
    RetainedUploadStats,
};
const SIZE: u32 = 64;
fn scene(colored: bool) -> (Scene, Mobject, ExecutionSession) {
    let mut scene = Scene::new();
    let camera = scene.camera_frame().unwrap();
    if colored {
        let mut base = scene.rectangle(8.0, 8.0).unwrap();
        base.disable_stroke().unwrap();
        base.set_fill(0.25, 0.5, 0.75, 0.5).unwrap();
        scene.add(&base).unwrap();
    }
    let mut label = scene.text(Text::new("HDR").with_font_size(144.0)).unwrap();
    if colored {
        label.set_fill(0.5, 0.04, 0.75, 0.5).unwrap();
        label.set_object_opacity(0.5).unwrap();
    } else {
        label.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    }
    label.disable_stroke().unwrap();
    scene.add(&label).unwrap();
    let mut session = scene.execution_session().unwrap();
    if colored {
        let mut tx = SemanticMutationTransaction::new();
        tx.set_view_color_profile(camera.node_id(), ViewColorProfile::LinearHdr);
        scene.live(&mut session).apply(tx).unwrap();
    }
    (scene, label, session)
}
struct Raster {
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    target: wgpu::Texture,
}
impl Raster {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let mut renderer = GpuRenderer::new(device, queue, format);
        renderer.set_viewport(device, queue, SIZE, SIZE);
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 8.0)).unwrap(),
        );
        let text = renderer.create_retained_text_state(device, queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("actual authored text working color"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
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
    ) -> (Vec<u8>, RetainedUploadStats) {
        let publication = session.take_renderer_publication();
        let frame = self
            .preparer
            .prepare_publication(
                device,
                &publication,
                TextDeviceMetrics::uniform(8.0).unwrap(),
            )
            .unwrap();
        assert!(
            !frame.text.mask_quads.is_empty(),
            "real Scene must use the mask glyph pipeline"
        );
        let upload = self
            .renderer
            .upload_retained(device, queue, &frame, &mut self.text);
        let mut encoder = device.create_command_encoder(&Default::default());
        self.renderer
            .encode_retained(
                &mut encoder,
                &self.target.create_view(&Default::default()),
                &frame,
                &self.text,
                wgpu::Color::TRANSPARENT,
                None,
            )
            .unwrap();
        queue.submit([encoder.finish()]);
        (read(device, queue, &self.target), upload)
    }
}
#[test]
#[ignore = "requires real native text and float blending; missing adapter fails qualification"]
fn authored_text_linear_tint_composes_with_geometry_and_restores_without_raster_uploads() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (_, _, mut source) = scene(false);
    let mut compatibility = Raster::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let (coverage, _) = compatibility.render(&device, &queue, &mut source);
    assert!(coverage.as_chunks::<4>().0.iter().any(|p| p[3] != 0));
    let (scene, label, mut session) = scene(true);
    let mut linear = Raster::new(&device, &queue, wgpu::TextureFormat::Rgba16Float);
    let artifact =
        std::env::var_os("NOON_RETAINED_TEXT_ARTIFACT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &artifact {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("qualified-white-text-coverage.rgba"), &coverage).unwrap();
    }
    let mut initial = None;
    let mut max_error = 0.0f64;
    for (phase, tint) in [[0.5, 0.04, 0.75], [0.75, 0.5, 0.04], [0.5, 0.04, 0.75]]
        .into_iter()
        .enumerate()
    {
        if phase != 0 {
            scene
                .live(&mut session)
                .set_fill(&label, tint[0], tint[1], tint[2], 0.5)
                .unwrap();
        }
        let (pixels, upload) = linear.render(&device, &queue, &mut session);
        if phase != 0 {
            assert_eq!(upload.geometry.bytes_uploaded, 0);
            assert_eq!(upload.images.pixel_bytes_uploaded, 0);
        }
        let expected: Vec<[f64; 4]> = coverage
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let alpha = f64::from(p[3]) / 255.0 * 0.25;
                [
                    srgb_decode(tint[0]) * alpha + srgb_decode(0.25) * 0.5 * (1.0 - alpha),
                    srgb_decode(tint[1]) * alpha + srgb_decode(0.5) * 0.5 * (1.0 - alpha),
                    srgb_decode(tint[2]) * alpha + srgb_decode(0.75) * 0.5 * (1.0 - alpha),
                    alpha + 0.5 * (1.0 - alpha),
                ]
            })
            .collect();
        for (at, (a, e)) in decode(&pixels)
            .iter()
            .zip(expected.iter().flatten())
            .enumerate()
        {
            let error = (a - e).abs();
            max_error = max_error.max(error);
            assert!(
                a.is_finite() && error <= 0.002,
                "text phase {phase} channel {at}: {a} != {e}"
            );
        }
        if phase == 0 {
            initial = Some(pixels.clone());
        } else if phase == 2 {
            assert_eq!(
                &pixels,
                initial.as_ref().unwrap(),
                "exact text tint restoration"
            );
        }
        let (clean, upload) = linear.render(&device, &queue, &mut session);
        assert_eq!(clean, pixels);
        assert_eq!(upload.bytes_uploaded(), 0);
        if let Some(dir) = &artifact {
            std::fs::write(dir.join(format!("text-{phase}.rgba16f")), pixels).unwrap();
            std::fs::write(
                dir.join(format!("text-{phase}.expected-rgba64f")),
                expected
                    .iter()
                    .flatten()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        }
    }
    let manifest=format!("{{\"phases\":3,\"width\":64,\"height\":64,\"float_channels\":49152,\"linear_bound\":0.002,\"max_linear_error\":{max_error},\"coverage_source\":\"qualified compatibility white-text alpha; color/composition proof only\"}}");
    if let Some(dir) = &artifact {
        std::fs::write(dir.join("manifest.json"), &manifest).unwrap();
    }
    eprintln!("authored text working color: {manifest}");
}
