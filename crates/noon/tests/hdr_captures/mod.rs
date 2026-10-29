//! Actual authored composed/mixed captures in the linear working profile.
use super::{decode, read, reference as bloom_reference, srgb_decode};
use noon::{
    effects::{Blur, EffectUpdate, Pixels},
    ExecutionSession, Scene,
};
use noon_core::{SemanticMutationTransaction, Vec2, ViewColorProfile};
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, HdrBloomCapture, HdrBloomFilter,
    HdrBloomParameters, HdrBloomScope, RetainedFramePreparer, RetainedTextGpuState,
};
const SIZE: [u32; 2] = [64, 48];
const CLEAR: [f64; 4] = [0.015625, 0.03125, 0.0625, 0.25];
struct Surface {
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    target: wgpu::Texture,
}
impl Surface {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba16Float);
        renderer.set_viewport(device, queue, SIZE[0], SIZE[1]);
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        );
        let text = renderer.create_retained_text_state(device, queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("authored linear composed capture"),
            size: wgpu::Extent3d {
                width: SIZE[0],
                height: SIZE[1],
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
    ) -> (Vec<u8>, noon_render_wgpu::RetainedEffectStats) {
        let publication = session.take_renderer_publication();
        assert_eq!(publication.color_profile(), ViewColorProfile::LinearHdr);
        let metrics = TextDeviceMetrics::uniform(8.0)
            .unwrap()
            .with_world_origin_pixels(Vec2::new(32.0, 24.0))
            .unwrap();
        let candidates = publication
            .painter_order()
            .iter()
            .map(|&i| i as usize)
            .collect::<Vec<_>>();
        let visible = self
            .renderer
            .effect_source_visibility(&publication, &candidates)
            .unwrap();
        let prepared = self
            .preparer
            .prepare_planned_publication_visible(device, &publication, visible, metrics)
            .unwrap();
        self.renderer
            .upload_retained(device, queue, &prepared, &mut self.text);
        let mut encoder = device.create_command_encoder(&Default::default());
        let clear = wgpu::Color {
            r: CLEAR[0],
            g: CLEAR[1],
            b: CLEAR[2],
            a: CLEAR[3],
        };
        let stats = self
            .renderer
            .prepare_retained_effects(
                device,
                queue,
                &mut encoder,
                &prepared,
                &self.text,
                &publication,
                clear,
                8 * 1024 * 1024,
            )
            .unwrap();
        self.renderer
            .encode_retained(
                &mut encoder,
                &self.target.create_view(&Default::default()),
                &prepared,
                &self.text,
                clear,
                None,
            )
            .unwrap();
        queue.submit([encoder.finish()]);
        (read(device, queue, &self.target), stats)
    }
}
fn over(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    std::array::from_fn(|c| a[c] + b[c] * (1.0 - a[3]))
}
fn source(mixed: bool, child_opacity: f64) -> Vec<[f64; 4]> {
    (0..SIZE[0] * SIZE[1])
        .map(|at| {
            let x = at % SIZE[0];
            let y = at / SIZE[0];
            let mut color = [0.0; 4];
            if (12..36).contains(&x) && (16..32).contains(&y) {
                let a = 0.75 * child_opacity;
                color = [a, a, a, a];
            }
            if (24..48).contains(&x) && (12..28).contains(&y) {
                let a = 0.5 * 0.5;
                color = over([srgb_decode(0.25) * a, srgb_decode(0.5) * a, a, a], color);
            }
            if mixed && (28..44).contains(&x) && (20..28).contains(&y) {
                let a = 128.0 / 255.0 * 0.5;
                color = over(
                    [
                        srgb_decode(192.0 / 255.0) * a,
                        srgb_decode(48.0 / 255.0) * a,
                        srgb_decode(16.0 / 255.0) * a,
                        a,
                    ],
                    color,
                );
            }
            color
        })
        .collect()
}
fn convolve(input: &[[f64; 4]], sigma: f64) -> Vec<[f64; 4]> {
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
    (0..SIZE[0] * SIZE[1])
        .map(|at| {
            let x = (at % SIZE[0]) as i32;
            let y = (at / SIZE[0]) as i32;
            let mut out = [0.0; 4];
            for yy in (y - radius).max(0)..=(y + radius).min(SIZE[1] as i32 - 1) {
                for xx in (x - radius).max(0)..=(x + radius).min(SIZE[0] as i32 - 1) {
                    let w = weight(xx - x, yy - y) / mass;
                    let p = input[(yy as u32 * SIZE[0] + xx as u32) as usize];
                    for c in 0..4 {
                        out[c] += p[c] * w;
                    }
                }
            }
            out
        })
        .collect()
}
fn expected(
    mixed: bool,
    ordered: bool,
    sigma: f64,
    opacity: f64,
    child_opacity: f64,
) -> Vec<[f64; 4]> {
    let mut image = convolve(&source(mixed, child_opacity), sigma);
    if ordered && sigma > 0.0 {
        image = convolve(&image, 0.65);
    }
    image
        .into_iter()
        .map(|pixel| over(pixel.map(|v| v * opacity), CLEAR))
        .collect()
}
#[test]
#[ignore = "requires a real native float/blending adapter; absence fails qualification"]
fn authored_composed_and_mixed_blur_preserve_linear_color_and_local_revisions() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("HDR composed adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let root = std::env::var_os("NOON_HDR_COMPOSED_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(root) = &root {
        std::fs::create_dir_all(root).unwrap();
    }
    let mut cases = Vec::new();
    let mut maximum = 0.0f64;
    let mut display_max = 0;
    let mut bloom_witness = 0.0f64;
    for (mixed, ordered) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut scene = Scene::new();
        let camera = scene.camera_frame().unwrap();
        let mut a = scene.rectangle(3.0, 2.0).unwrap();
        a.disable_stroke().unwrap();
        a.set_fill(1.0, 1.0, 1.0, 0.75).unwrap();
        a.set_object_opacity(0.5).unwrap();
        a.set_translation(-1.0, 0.0).unwrap();
        let mut b = scene.rectangle(3.0, 2.0).unwrap();
        b.disable_stroke().unwrap();
        b.set_fill(0.25, 0.5, 1.0, 0.5).unwrap();
        b.set_object_opacity(0.5).unwrap();
        b.set_translation(0.5, 0.5).unwrap();
        let mut options = noon::ImageMobjectOptions::rgba8(1, 1, [192, 48, 16, 128]).unwrap();
        options.set_height(1.0).unwrap();
        options.set_sampling(noon::RasterImageSampling::Nearest);
        let mut image = scene.image(options).unwrap();
        image.scale(2.0, 1.0).unwrap();
        image.set_translation(0.5, 0.0).unwrap();
        image.set_object_opacity(0.5).unwrap();
        let mut group = if mixed {
            scene
                .family(&[(&a).into(), (&b).into(), (&image).into()])
                .unwrap()
        } else {
            scene.family(&[(&a).into(), (&b).into()]).unwrap()
        };
        group.set_object_opacity(0.75).unwrap();
        group
            .add_effect(
                Blur::new(EffectUpdate::default().radius(Pixels(3.0))).unwrap(),
                "soften",
            )
            .unwrap();
        if ordered {
            group
                .add_effect(
                    Blur::new(EffectUpdate::default().radius(Pixels(0.65))).unwrap(),
                    "polish",
                )
                .unwrap();
        }
        let mut unrelated = scene.square(0.5).unwrap();
        unrelated.set_translation(100.0, 100.0).unwrap();
        scene
            .add_many(&[(&group).into(), (&unrelated).into()])
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut tx = SemanticMutationTransaction::new();
        tx.set_view_color_profile(camera.node_id(), ViewColorProfile::LinearHdr);
        scene.live(&mut session).apply(tx).unwrap();
        let mut surface = Surface::new(&device, &queue);
        let mut bloom = HdrBloomFilter::new(&adapter, &device);
        let mut bloom_scope = HdrBloomScope::default();
        let bp = HdrBloomParameters {
            sigma_pixels: 0.65,
            threshold: 0.02,
            intensity: 3.0,
            scratch_budget_bytes: 8 * 1024 * 1024,
        };
        let mut initial = None;
        for phase in 0..9 {
            let (sigma, opacity, child_opacity) = match phase {
                0 | 6 | 8 => (3.0, 0.75, 0.5),
                1 => (3.0, 0.25, 0.5),
                2 | 3 => (1.25, 0.25, 0.5),
                4 => (0.0, 0.25, 0.5),
                5 => (0.0, 0.0, 0.5),
                7 => (3.0, 0.75, 0.25),
                _ => unreachable!(),
            };
            if phase > 0 {
                scene
                    .live(&mut session)
                    .set_effect(
                        &group,
                        "soften",
                        EffectUpdate::default().radius(Pixels(sigma)),
                    )
                    .unwrap();
                scene
                    .live(&mut session)
                    .set_family_object_opacity(&group, opacity)
                    .unwrap();
            }
            if ordered && phase > 0 {
                scene
                    .live(&mut session)
                    .set_effect(
                        &group,
                        "polish",
                        EffectUpdate::default().radius(Pixels(if sigma > 0.0 {
                            0.65
                        } else {
                            0.0
                        })),
                    )
                    .unwrap();
            }
            if phase == 3 {
                scene
                    .live(&mut session)
                    .set_color(&unrelated, 1.0, 0.0, 0.0, 1.0)
                    .unwrap();
            }
            if phase == 7 || phase == 8 {
                scene
                    .live(&mut session)
                    .set_object_opacity(&a, child_opacity)
                    .unwrap();
            }
            let (raw, stats) = surface.render(&device, &queue, &mut session);
            let expect = expected(mixed, ordered, sigma, opacity, child_opacity);
            let actual = decode(&raw);
            for (at, (a, e)) in actual.iter().zip(expect.iter().flatten()).enumerate() {
                maximum = maximum.max((a - e).abs());
                assert!(
                    (a - e).abs() <= 0.006 + 0.004 * e.abs(),
                    "mixed={mixed} ordered={ordered} phase={phase} at={at}: {a} != {e}"
                );
            }
            if phase == 0 {
                initial = Some(raw.clone());
            }
            if phase == 6 || phase == 8 {
                assert_eq!(Some(&raw), initial.as_ref(), "exact restoration");
            }
            if phase == 1 || phase == 3 {
                assert_eq!(stats.source_passes, 0);
                assert_eq!(stats.blur_passes, 0);
            }
            let (clean, clean_stats) = surface.render(&device, &queue, &mut session);
            assert_eq!(clean, raw);
            assert_eq!(
                (
                    clean_stats.source_passes,
                    clean_stats.blur_passes,
                    clean_stats.bytes_uploaded
                ),
                (0, 0, 0)
            );
            let mut fresh = Surface::new(&device, &queue);
            let (fresh_pixels, _) = fresh.render(&device, &queue, &mut session);
            assert_eq!(fresh_pixels, raw, "fresh/incremental phase={phase}");
            bloom
                .prepare(
                    &device,
                    &queue,
                    &mut bloom_scope,
                    HdrBloomCapture {
                        source: &surface.target,
                        revision: cases.len() as u64 + 1,
                    },
                    bp,
                )
                .unwrap();
            let mut encoder = device.create_command_encoder(&Default::default());
            bloom.encode(&mut encoder, &mut bloom_scope);
            queue.submit([encoder.finish()]);
            let (linear, display) = bloom_reference(&expect, SIZE, bp);
            let bloom_raw = read(&device, &queue, bloom_scope.linear_output().unwrap());
            for (a, e) in decode(&bloom_raw).iter().zip(linear.iter().flatten()) {
                bloom_witness = bloom_witness.max(*a);
                assert!(
                    (a - e).abs() <= 0.006 + 0.004 * e.abs(),
                    "Bloom mixed={mixed} ordered={ordered} phase={phase}: {a} != {e}"
                );
            }
            let display_raw = read(&device, &queue, bloom_scope.display_output().unwrap());
            for (a, e) in display_raw.iter().zip(&display) {
                display_max = display_max.max(a.abs_diff(*e));
                assert!(
                    a.abs_diff(*e) <= 2,
                    "display mixed={mixed} ordered={ordered} phase={phase}: {a} != {e}"
                );
            }
            let id = cases.len();
            if let Some(root) = &root {
                std::fs::write(root.join(format!("{id}.rgba16f")), raw).unwrap();
                std::fs::write(root.join(format!("{id}.bloom.rgba16f")), bloom_raw).unwrap();
                std::fs::write(root.join(format!("{id}.display.rgba8")), display_raw).unwrap();
            }
            cases.push(serde_json::json!({"id":id,"mixed":mixed,"ordered":ordered,"phase":phase,"sigma":sigma,"opacity":opacity,"child_opacity":child_opacity,"expected":expect,"bloom_expected":linear,"display_expected":display,"source_passes":stats.source_passes,"blur_passes":stats.blur_passes,"retained_texture_bytes":stats.retained_texture_bytes}));
        }
    }
    assert!(
        bloom_witness > 1.0,
        "unit-valued authored source produces bright post-Bloom output"
    );
    if let Some(root) = root {
        std::fs::write(root.join("report.json"),serde_json::to_vec(&serde_json::json!({"size":SIZE,"clear":CLEAR,"cases":cases,"maximum_error":maximum,"display_maximum":display_max,"post_bloom_witness":bloom_witness})).unwrap()).unwrap();
    }
    eprintln!("HDR composed: {} authored phases, float max={maximum}, display max={display_max}, post-Bloom witness={bloom_witness}",cases.len());
}
