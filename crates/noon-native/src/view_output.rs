use super::*;
use noon::{
    effects::{Bloom, EffectUpdate, Pixels},
    Scene,
};
use noon_core::{Color, GeometryRef, Transform2D, Vec2, ViewColorProfile};
use noon_render_wgpu::{
    text::TextDeviceMetrics, AnalyticOverlay, Camera2D, HdrBloomStats, InteractiveRetainedFrame,
    RetainedFramePreparer, RetainedTextGpuState, DEFAULT_EFFECT_TEXTURE_BUDGET,
};
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
fn scene() -> (Scene, noon::Mobject, noon::Mobject) {
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame().unwrap();
    camera
        .set_view_color_profile(ViewColorProfile::LinearHdr)
        .unwrap();
    camera.set_object_opacity(0.75).unwrap();
    camera
        .add_effect(
            Bloom::new(
                EffectUpdate::default()
                    .radius(Pixels(1.25))
                    .threshold(0.25)
                    .intensity(1.0),
            )
            .unwrap(),
            "bloom",
        )
        .unwrap();
    let mut panel = scene.rectangle(32.0, 32.0).unwrap();
    panel.set_fill(1.0, 1.0, 1.0, 0.5).unwrap();
    panel.disable_stroke().unwrap();
    scene.add(&panel).unwrap();
    (scene, camera, panel)
}
fn target(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("native HDR output proof"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bpp = if texture.format() == wgpu::TextureFormat::Rgba16Float {
        8
    } else {
        4
    };
    let row = texture.width() * bpp;
    let stride = row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("native HDR proof readback"),
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
    buffer.map_async(wgpu::MapMode::Read, .., move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.get_mapped_range(..).unwrap();
    let mut bytes: Vec<_> = mapped
        .chunks_exact(stride as usize)
        .flat_map(|r| r[..row as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    if texture.format() == FORMAT {
        for pixel in bytes.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
    }
    bytes
}
// Independent full 2D finite Gaussian over literal constant .5 radiance and
// opaque black clear. No production color/kernel/filter helper supplies it.
fn reference(
    size: [u32; 2],
    sigma: f64,
    threshold: f64,
    intensity: f64,
    overlay: bool,
) -> (Vec<f64>, Vec<u8>) {
    let radius = (3.0 * sigma).ceil() as i32;
    let weight = |x: i32, y: i32| {
        if sigma == 0.0 {
            f64::from(x == 0 && y == 0)
        } else {
            (-f64::from(x * x + y * y) / (2.0 * sigma * sigma)).exp()
        }
    };
    let mass: f64 = (-radius..=radius)
        .flat_map(|y| (-radius..=radius).map(move |x| weight(x, y)))
        .sum();
    let mut linear = Vec::new();
    let mut display = Vec::new();
    let extraction = (0.5 - threshold).max(0.0) / 0.5;
    for y in 0..size[1] as i32 {
        for x in 0..size[0] as i32 {
            let mut fraction = 0.0;
            for yy in (y - radius).max(0)..=(y + radius).min(size[1] as i32 - 1) {
                for xx in (x - radius).max(0)..=(x + radius).min(size[0] as i32 - 1) {
                    fraction += weight(xx - x, yy - y) / mass;
                }
            }
            let value = 0.5 * (1.0 + extraction * intensity * fraction);
            linear.extend_from_slice(&[value, value, value, 1.0]);
            let mapped = value / (1.0 + value);
            let encoded = if mapped <= 0.0031308 {
                12.92 * mapped
            } else {
                1.055 * mapped.powf(1.0 / 2.4) - 0.055
            };
            let byte = (encoded * 255.0).round();
            let rgb = if overlay {
                [
                    (128.0 + byte * 127.0 / 255.0).round() as u8,
                    (byte * 127.0 / 255.0).round() as u8,
                    (byte * 127.0 / 255.0).round() as u8,
                ]
            } else {
                [byte as u8; 3]
            };
            display.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    (linear, display)
}
struct Draw {
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    profile: ViewColorProfile,
}
impl Draw {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let renderer = GpuRenderer::new(device, queue, FORMAT);
        let text = renderer.create_retained_text_state(device, queue);
        Self {
            renderer,
            text,
            preparer: RetainedFramePreparer::new(),
            profile: ViewColorProfile::Compatibility,
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output: &mut AuthoredViewOutput,
        publication: &RendererPublication<'_>,
        size: [u32; 2],
        overlay: &OverlayGpuState,
        wrong_input: bool,
    ) -> (Vec<u8>, HdrBloomStats, usize) {
        let profile = output.prepare(device, publication, false, size).unwrap();
        if profile != self.profile {
            let format = if profile == ViewColorProfile::LinearHdr {
                wgpu::TextureFormat::Rgba16Float
            } else {
                FORMAT
            };
            self.renderer = GpuRenderer::new(device, queue, format);
            self.text = self.renderer.create_retained_text_state(device, queue);
            self.preparer = RetainedFramePreparer::new();
            self.profile = profile;
        }
        self.renderer.set_viewport(device, queue, size[0], size[1]);
        self.renderer.set_camera(
            queue,
            Camera2D::new(
                Vec2::ZERO,
                Vec2::new(8.0 * size[0] as f32 / size[1] as f32, 8.0),
            )
            .unwrap(),
        );
        let metrics = TextDeviceMetrics::uniform(size[1] as f32 / 8.0)
            .unwrap()
            .with_world_origin_pixels(Vec2::new(size[0] as f32 * 0.5, size[1] as f32 * 0.5))
            .unwrap();
        let visible: Vec<_> = (0..publication.frame().objects.len()).collect();
        let visible = self
            .renderer
            .effect_source_visibility(publication, &visible)
            .unwrap();
        let prepared = self
            .preparer
            .prepare_planned_publication_visible(device, publication, visible, metrics)
            .unwrap();
        let uploaded = self
            .renderer
            .upload_retained(device, queue, &prepared, &mut self.text)
            .bytes_uploaded();
        let target = target(device, size);
        let view = target.create_view(&Default::default());
        let clear = noon_render_wgpu::authored_clear_color(wgpu::Color::BLACK, profile);
        let mut encoder = device.create_command_encoder(&Default::default());
        self.renderer
            .prepare_retained_effects(
                device,
                queue,
                &mut encoder,
                &prepared,
                &self.text,
                publication,
                clear,
                DEFAULT_EFFECT_TEXTURE_BUDGET,
            )
            .unwrap();
        let empty = OverlayGpuState::default();
        self.renderer
            .encode_retained_with_transient_presentations_and_overlay(
                &mut encoder,
                output.scene_view(&view),
                InteractiveRetainedFrame {
                    prepared: &prepared,
                    text: &self.text,
                    transient: None,
                    overlay: if output.is_hdr() && !wrong_input {
                        &empty
                    } else {
                        overlay
                    },
                },
                clear,
                None,
            )
            .unwrap();
        let stats = output
            .resolve(
                device,
                queue,
                &mut encoder,
                &mut self.renderer,
                publication,
                8.0,
                if wrong_input { &empty } else { overlay },
                &target,
            )
            .unwrap();
        queue.submit([encoder.finish()]);
        (read(device, queue, &target), stats, uploaded)
    }
}
#[test]
fn native_hdr_extent_admission_is_checked_before_view_allocations() {
    let (device, _queue) = wgpu::Device::noop(&Default::default());
    assert!(AuthoredViewOutput::validate_extent(&device, [16, 16]).is_ok());
    assert!(AuthoredViewOutput::validate_extent(&device, [1920, 1080]).is_ok());
    assert!(AuthoredViewOutput::validate_extent(&device, [3840, 2160]).is_err());
    for size in [[0, 1], [1, 0], [u32::MAX, u32::MAX], [16384, 16384]] {
        assert!(AuthoredViewOutput::validate_extent(&device, size).is_err());
    }
}
#[test]
#[ignore = "requires real native float render targets; missing capability fails qualification"]
fn native_hdr_view_matches_capture_keeps_overlay_outside_bloom_and_recovers() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (mut device, mut queue) =
        pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (scene, camera, _panel) = scene();
    let mut session = scene.execution_session().unwrap();
    let mut output = AuthoredViewOutput::new(
        &adapter,
        &device,
        FORMAT,
        noon_render_wgpu::OutputTransfer::Direct,
    );
    assert!(
        output.supports_hdr(),
        "native float/blend/MSAA4 output capability required"
    );
    let mut draw = Draw::new(&device, &queue);
    let mut overlay = OverlayGpuState::default();
    let root = std::env::var_os("NOON_NATIVE_HDR_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(root) = &root {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(
            root.join("adapter.txt"),
            format!("{:?}", adapter.get_info()),
        )
        .unwrap();
    }
    let mut size = [16, 16];
    let mut sigma = 1.25;
    let mut threshold = 0.25;
    let mut intensity = 1.0;
    let mut cases = Vec::new();
    let mut initial = None;
    let mut maximum = 0;
    let mut maximum_float = 0.0f64;
    for phase in 0..13 {
        match phase {
            3 => {
                sigma = 2.0;
                threshold = 0.0;
                intensity = 3.0;
                scene
                    .live(&mut session)
                    .set_effect(
                        &camera,
                        "bloom",
                        EffectUpdate::default()
                            .radius(Pixels(sigma))
                            .threshold(threshold)
                            .intensity(intensity),
                    )
                    .unwrap();
            }
            4 => {
                intensity = 0.0;
                scene
                    .live(&mut session)
                    .set_effect(&camera, "bloom", EffectUpdate::default().intensity(0.0))
                    .unwrap();
            }
            5 => {
                sigma = 0.0;
                threshold = 1.0;
                scene
                    .live(&mut session)
                    .remove_effect(&camera, "bloom")
                    .unwrap();
            }
            6 => scene
                .live(&mut session)
                .set_view_color_profile(&camera, ViewColorProfile::Compatibility)
                .unwrap(),
            7 => scene
                .live(&mut session)
                .set_view_color_profile(&camera, ViewColorProfile::LinearHdr)
                .unwrap(),
            8 => {
                sigma = 1.25;
                threshold = 0.25;
                intensity = 1.0;
                scene
                    .live(&mut session)
                    .add_effect(
                        &camera,
                        Bloom::new(
                            EffectUpdate::default()
                                .radius(Pixels(sigma))
                                .threshold(threshold)
                                .intensity(intensity),
                        )
                        .unwrap(),
                        "bloom",
                    )
                    .unwrap();
            }
            9 => size = [24, 12],
            10 => size = [16, 16],
            11 => {
                output.clear();
            }
            12 => {
                let pair = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
                device = pair.0;
                queue = pair.1;
                output = AuthoredViewOutput::new(
                    &adapter,
                    &device,
                    FORMAT,
                    noon_render_wgpu::OutputTransfer::Direct,
                );
                draw = Draw::new(&device, &queue);
                overlay = OverlayGpuState::default();
            }
            _ => {}
        }
        let highlighted = phase == 1;
        let tint = if highlighted {
            Some(
                AnalyticOverlay::new(
                    &GeometryRef::Rectangle {
                        size: Vec2::new(32.0, 32.0),
                    },
                    Transform2D::default(),
                    Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0),
                )
                .unwrap(),
            )
        } else {
            None
        };
        overlay.update(&device, &queue, tint);
        // A snapshot is used only at the explicit qualification/export boundary.
        // Both normal hosts observe the same published effective state.
        let mut capture_session = session.clone();
        let mut capture = noon_export::SessionCapture::new(
            &capture_session,
            noon_export::CaptureOptions::new(size[0], size[1]),
        )
        .unwrap();
        let capture_pixels = capture.capture(&mut capture_session).unwrap().rgba.to_vec();
        assert_eq!(
            session.frame().time,
            0.0,
            "view resources never advance authored time"
        );
        let publication = session.take_renderer_publication();
        if phase == 0 {
            let mut unsupported = AuthoredViewOutput::new(
                &adapter,
                &device,
                wgpu::TextureFormat::Rgba8Uint,
                noon_render_wgpu::OutputTransfer::Direct,
            );
            assert!(!unsupported.supports_hdr());
            assert!(unsupported
                .prepare(&device, &publication, false, size)
                .is_err());
            assert!(
                unsupported.source_texture().is_none() && unsupported.display_output().is_none()
            );
            assert!(output.prepare(&device, &publication, true, size).is_err());
            assert!(output.source_texture().is_none() && output.display_output().is_none());
        }
        if phase == 1 {
            let previous = output.source_texture().unwrap().clone();
            assert!(output.prepare(&device, &publication, true, size).is_err());
            assert!(output
                .prepare(&device, &publication, false, [3840, 2160])
                .is_err());
            assert_eq!(output.source_texture(), Some(&previous));
        }
        let (actual, stats, uploads) = draw.frame(
            &device,
            &queue,
            &mut output,
            &publication,
            size,
            &overlay,
            false,
        );
        if [1, 2, 3, 4, 5].contains(&phase) {
            assert_eq!(
                uploads, 0,
                "parameter/overlay edit must not upload source geometry"
            );
        }
        let (linear, expected) = if phase == 6 {
            (
                Vec::new(),
                [188, 188, 188, 255].repeat((size[0] * size[1]) as usize),
            )
        } else {
            reference(size, sigma, threshold, intensity, highlighted)
        };
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(&expected) {
            maximum = maximum.max(a.abs_diff(*e));
            assert!(a.abs_diff(*e) <= 2, "phase={phase}: {a} vs {e}");
        }
        if phase != 6 && !highlighted {
            assert_eq!(
                actual, capture_pixels,
                "fixed-view native output/capture equivalence"
            );
        }
        if phase == 0 {
            initial = Some(actual.clone());
        }
        if [2, 8, 10, 11, 12].contains(&phase) {
            assert_eq!(
                initial.as_ref(),
                Some(&actual),
                "exact restoration/recreation"
            );
        }
        if phase != 6 {
            assert_eq!(
                stats.passes,
                if intensity == 0.0 {
                    [0, 0, 0, 1, 1]
                } else {
                    [1; 5]
                }
            );
            let raw = read(&device, &queue, output.linear_output().unwrap());
            let values = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| half::f16::from_le_bytes(*b).to_f64());
            for (a, e) in values.zip(&linear) {
                maximum_float = maximum_float.max((a - e).abs());
                assert!((a - e).abs() <= 0.006 + 0.004 * e.abs());
            }
            if ![0, 12].contains(&phase) {
                assert_eq!(
                    stats.pipeline_compiles, 0,
                    "warm output reuses filter and presenter programs"
                );
            }
            if let Some(root) = &root {
                std::fs::write(root.join(format!("{phase}.linear.rgba16")), raw).unwrap();
            }
        } else {
            assert!(!output.is_hdr());
            assert_eq!(stats.passes, [0; 5]);
        }
        if phase == 1 {
            let (wrong, _, _) = draw.frame(
                &device,
                &queue,
                &mut output,
                &publication,
                size,
                &overlay,
                true,
            );
            let failed = wrong
                .iter()
                .zip(&expected)
                .filter(|(a, e)| a.abs_diff(**e) > 2)
                .count();
            let max = wrong
                .iter()
                .zip(&expected)
                .map(|(a, e)| a.abs_diff(*e))
                .max()
                .unwrap();
            assert!(
                failed > 200 && max > 10,
                "Bloom-contaminated overlay must fail the same bound"
            );
            if let Some(root) = &root {
                std::fs::write(root.join("negative-overlay-before-bloom.rgba8"), wrong).unwrap();
            }
            cases.push(serde_json::json!({"negative":true,"failed_channels":failed,"maximum_error":max,"expected":expected}));
            let restored = draw
                .frame(
                    &device,
                    &queue,
                    &mut output,
                    &publication,
                    size,
                    &overlay,
                    false,
                )
                .0;
            assert_eq!(
                restored, actual,
                "wrong-input control must not leak into restored output"
            );
        }
        if let Some(root) = &root {
            std::fs::write(root.join(format!("{phase}.rgba8")), &actual).unwrap();
            std::fs::write(root.join(format!("{phase}.capture.rgba8")), &capture_pixels).unwrap();
        }
        cases.push(serde_json::json!({"phase":phase,"size":size,"sigma":sigma,"threshold":threshold,
            "intensity":intensity,"overlay":highlighted,"profile":if phase==6{"compatibility"}else{"linear_hdr"},
            "expected":expected,"linear_expected":linear,"uploads":uploads,"pipeline_compiles":stats.pipeline_compiles,
            "scratch_bytes":stats.scratch_bytes}));
    }
    if let Some(root) = root {
        std::fs::write(root.join("report.json"),serde_json::to_vec_pretty(
        &serde_json::json!({"cases":cases,"maximum_error":maximum,"maximum_float_error":maximum_float})).unwrap()).unwrap();
    }
    eprintln!("native HDR view: 13 phases, native/capture equivalence, overlay negative, resize, target/device recreation; display max={maximum}, float max={maximum_float}");
}

struct BloomAnimation {
    animation: noon::DeclaredAnimation,
    started: bool,
}
impl noon::LiveContinuation for BloomAnimation {
    type Error = String;
    fn resume(
        &mut self,
        live: &mut noon::LiveSession<'_>,
    ) -> Result<noon::ContinuationStep, String> {
        if self.started {
            return Ok(noon::ContinuationStep::Finished);
        }
        self.started = true;
        live.play_animation(&self.animation)
            .map(noon::ContinuationStep::Await)
            .map_err(|e| e.to_string())
    }
}

fn bloom_program() -> noon::LiveProgram<BloomAnimation> {
    let (scene, camera, _) = scene();
    let mut target = camera.target_editor().unwrap();
    target
        .set_effect("bloom", EffectUpdate::default().intensity(2.0))
        .unwrap();
    let animation = scene
        .declare_transform_to(
            &camera,
            &target,
            noon::AnimationOptions::new()
                .run_time(0.2)
                .rate_func(noon::RateFunction::Linear),
        )
        .unwrap();
    scene
        .into_live_program(BloomAnimation {
            animation,
            started: false,
        })
        .unwrap()
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires an X11 display and a working native float adapter"]
fn native_surface_smoke_presents_authored_camera_bloom() {
    use winit::platform::x11::EventLoopBuilderExtX11;
    let source = super::super::LiveProgramExecutionSource::new(
        bloom_program(),
        noon::RustHostCallbackTable::new(),
    )
    .unwrap();
    let mut builder = winit::event_loop::EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
    let mut app = super::super::NativeApp::from_source(
        Box::new(source),
        super::super::NativeViewportConfig {
            width: 320,
            height: 180,
            title: "Noon authored HDR Bloom".to_owned(),
            ..Default::default()
        },
    );
    app.exit_after_present = Some(0.2);
    event_loop.run_app(&mut app).unwrap();
    assert!(
        app.error.is_none(),
        "native HDR presentation failed: {:?}",
        app.error
    );
    assert_eq!(app.presented_frame_time, Some(0.2));
    assert!(app.last_geometry_draw_calls > 0);
    assert_eq!(app.last_bloom_passes, [1; 5]);
}

#[test]
fn native_camera_bloom_program_reaches_exact_authored_endpoint() {
    let mut program = bloom_program();
    program.resume().unwrap();
    let mut callbacks = noon::RustHostCallbackTable::new();
    for (time, expected) in [(0.0, 1.0), (0.1, 1.5), (0.2, 2.0)] {
        program.drive_to(&mut callbacks, time).unwrap();
        let publication = program.take_renderer_publication();
        assert_eq!(
            publication.view_effects().unwrap().definitions()[0]
                .as_bloom()
                .unwrap()
                .intensity(),
            expected
        );
    }
    let noon::LiveProgramStatus::PublicationPending(context) = program.status() else {
        panic!("exact endpoint must wait for host admission");
    };
    program.admit_publication(context).unwrap();
    assert_eq!(program.resume().unwrap(), noon::LiveProgramStatus::Finished);
}
