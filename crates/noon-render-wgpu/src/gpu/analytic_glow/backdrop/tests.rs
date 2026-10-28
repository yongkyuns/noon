use super::*;
use crate::{text::TextDeviceMetrics, RetainedEffectStats, RetainedFramePreparer};
use noon_compile::{
    CompiledEffect, CompiledEffectStack, CompiledObject, CompiledScene, ExecutionPatch,
};
use noon_core::{BackdropBlur, Color, EffectUpdate, Pixels, SemanticNodeId, Style};
use noon_runtime::{ExecutionSlotId, ExecutionSpatialIndex, SceneInstance};

const VIEW: [u32; 2] = [80, 60];
const BUDGET: u64 = 64 * 1024 * 1024;
fn panel_effect(radius: f64) -> Arc<CompiledEffectStack> {
    Arc::new(
        CompiledEffectStack::new([CompiledEffect {
            attachment: SemanticNodeId::new(83, 2),
            definition: BackdropBlur::new(EffectUpdate::default().radius(Pixels(radius)))
                .unwrap()
                .into(),
        }])
        .unwrap(),
    )
}
fn row(id: u64, x: f32, size: [f32; 2], color: Color) -> CompiledObject {
    CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::rectangle(size[0], size[1]),
        Transform2D {
            translation: Vec2::new(x, 0.0),
            ..Transform2D::IDENTITY
        },
        Style {
            fill: Some(color),
            stroke: None,
            stroke_width: 0.0,
            ..Style::default()
        },
    )
}
fn rows(radius: Option<f64>, opacity: f32) -> Vec<CompiledObject> {
    let source = row(10, -0.7, [2.0, 2.4], Color::rgba(0.9, 0.1, 0.2, 0.65));
    let mut panel = row(20, 0.3, [3.6, 2.0], Color::rgba(0.1, 0.6, 0.9, 0.27));
    panel.base_style.opacity = opacity;
    panel.effects = radius.map(panel_effect);
    vec![source, panel]
}
struct Fixture {
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    target: wgpu::Texture,
    source_bounds_override: Option<noon_core::Rect>,
}
impl Fixture {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(device, queue, VIEW[0], VIEW[1]);
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        );
        let text = renderer.create_retained_text_state(device, queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("backdrop integration output"),
            size: wgpu::Extent3d {
                width: VIEW[0],
                height: VIEW[1],
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
            source_bounds_override: None,
        }
    }
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        runtime: &mut SceneInstance,
        clear: wgpu::Color,
    ) -> RetainedEffectStats {
        let mut index = ExecutionSpatialIndex::default();
        index.rebuild(
            runtime.frame(),
            runtime.geometry_resources(),
            runtime
                .painter_order()
                .iter()
                .map(|&i| (ExecutionSlotId::new(i, 0), i as usize)),
        );
        if let Some(bounds) = self.source_bounds_override {
            index.upsert_bounds(ExecutionSlotId::new(0, 0), ObjectId::new(10), bounds, 0);
        }
        let publication = runtime
            .take_renderer_publication()
            .with_spatial_index(&index);
        let visible = publication
            .painter_order()
            .iter()
            .map(|&i| i as usize)
            .collect::<Vec<_>>();
        let visible = self
            .renderer
            .effect_source_visibility(&publication, &visible)
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
                BUDGET,
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
        stats
    }
}
fn runtime(rows: Vec<CompiledObject>) -> SceneInstance {
    SceneInstance::new(CompiledScene::compile_objects(rows, &[]).unwrap())
}

#[test]
fn backdrop_retained_coordinator_reuses_clean_inputs_refilters_radius_and_retires() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut fixture = Fixture::new(&device, &queue);
    let mut scene = runtime(rows(Some(2.0), 0.6));
    let cold = fixture.render(&device, &queue, &mut scene, wgpu::Color::TRANSPARENT);
    assert_eq!(
        cold.source_passes, 3,
        "target paint, original coverage, prefix"
    );
    assert_eq!(cold.blur_passes, 2);
    assert!(cold.retained_texture_bytes > 0);
    scene.take_renderer_publication();
    let clean = fixture.render(&device, &queue, &mut scene, wgpu::Color::TRANSPARENT);
    assert_eq!(
        (
            clean.sources_prepared,
            clean.source_passes,
            clean.blur_passes,
            clean.composite_passes,
            clean.bytes_uploaded,
            clean.texture_allocations
        ),
        (0, 0, 0, 0, 0, 0)
    );
    scene
        .apply_execution_patch(&ExecutionPatch::SetEffects {
            object: ObjectId::new(20),
            effects: panel_effect(1.99),
        })
        .unwrap();
    let radius = fixture.render(&device, &queue, &mut scene, wgpu::Color::TRANSPARENT);
    assert_eq!(
        radius.source_passes, 0,
        "same finite support preserves paint and prefix"
    );
    assert_eq!(radius.blur_passes, 2);
    assert_eq!(radius.texture_allocations, 0);
    assert_eq!(radius.retained_texture_bytes, cold.retained_texture_bytes);
    scene.take_renderer_publication();
    let clear = fixture.render(
        &device,
        &queue,
        &mut scene,
        wgpu::Color {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 0.5,
        },
    );
    assert_eq!(clear.source_passes, 1, "view clear dirties prefix alone");
    assert_eq!(clear.blur_passes, 2);
    assert_eq!(clear.texture_allocations, 0);
    assert_eq!(clear.retained_texture_bytes, cold.retained_texture_bytes);
    scene
        .apply_execution_patch(&ExecutionPatch::SetEffectStack {
            object: ObjectId::new(20),
            expected: vec![SemanticNodeId::new(83, 2)].into_boxed_slice(),
            effects: None,
        })
        .unwrap();
    let retired = fixture.render(&device, &queue, &mut scene, wgpu::Color::TRANSPARENT);
    assert_eq!(retired.retained_texture_bytes, 0);
    assert!(!fixture.renderer.has_retained_effect_sources());
}

#[test]
fn cold_offscreen_backdrop_allocates_nothing_and_activation_uses_the_normal_capture_path() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut objects = rows(Some(2.0), 0.6);
    objects[1].base_transform.translation.x = 40.0;
    let mut scene = runtime(objects);
    let mut fixture = Fixture::new(&device, &queue);
    let cold = fixture.render(&device, &queue, &mut scene, wgpu::Color::TRANSPARENT);
    assert_eq!(
        (
            cold.source_passes,
            cold.blur_passes,
            cold.texture_allocations,
            cold.retained_texture_bytes
        ),
        (0, 0, 0, 0)
    );
    scene
        .apply_execution_patch(&ExecutionPatch::SetTransform {
            object: ObjectId::new(20),
            transform: Transform2D::IDENTITY,
        })
        .unwrap();
    let active = fixture.render(&device, &queue, &mut scene, wgpu::Color::TRANSPARENT);
    assert_eq!(active.source_passes, 3);
    assert_eq!(active.blur_passes, 2);
    assert!(active.retained_texture_bytes > 0);
}

fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let stride = (VIEW[0] * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(stride * VIEW[1]),
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
                rows_per_image: Some(VIEW[1]),
            },
        },
        wgpu::Extent3d {
            width: VIEW[0],
            height: VIEW[1],
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    mapped
        .chunks(stride as usize)
        .flat_map(|row| row[..VIEW[0] as usize * 4].to_vec())
        .collect()
}
fn ordinary(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rows: Vec<CompiledObject>,
    clear: wgpu::Color,
) -> Vec<u8> {
    let mut fixture = Fixture::new(device, queue);
    fixture.render(device, queue, &mut runtime(rows), clear);
    read(device, queue, &fixture.target)
}
// Deliberately slow full 2D f64 convolution, independent of the separable GPU
// Gaussian. Ordinary draw images qualify filter/composition, not source raster.
fn expected(prefix: &[u8], paint: &[u8], mask: &[u8], sigma: f64, opacity: f64) -> Vec<u8> {
    let support = (3.0 * sigma).ceil() as i32;
    let normalization = (-support..=support)
        .map(|x| (-0.5 * (f64::from(x) / sigma).powi(2)).exp())
        .sum::<f64>()
        .powi(2);
    let mut result = prefix.to_vec();
    for y in 0..VIEW[1] as i32 {
        for x in 0..VIEW[0] as i32 {
            let at = (y as usize * VIEW[0] as usize + x as usize) * 4;
            let m = f64::from(mask[at + 3]) / 255.0;
            if m == 0.0 {
                continue;
            }
            let pa = f64::from(paint[at + 3]) / 255.0;
            for c in 0..4 {
                let mut f = 0.0;
                for dy in -support..=support {
                    for dx in -support..=support {
                        let sx = x + dx;
                        let sy = y + dy;
                        assert!(
                            sx >= 0 && sy >= 0 && sx < VIEW[0] as i32 && sy < VIEW[1] as i32,
                            "reference fixture keeps expanded target in view"
                        );
                        let weight = (-0.5 * (f64::from(dx * dx + dy * dy) / sigma.powi(2))).exp()
                            / normalization;
                        f += f64::from(
                            prefix[(sy as usize * VIEW[0] as usize + sx as usize) * 4 + c],
                        ) * weight;
                    }
                }
                let p = f64::from(paint[at + c]) * if pa > m { m / pa } else { 1.0 };
                result[at + c] = (f64::from(prefix[at + c]) * (1.0 - opacity * m)
                    + (p + f * (m - pa.min(m))) * opacity)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        }
    }
    result
}
fn retain(name: &str, actual: &[u8], expected: &[u8], fields: &[(&str, &[u8])], metadata: String) {
    let Ok(root) = std::env::var("NOON_BACKDROP_COORDINATOR_ARTIFACT_DIR") else {
        return;
    };
    let path = std::path::Path::new(&root).join(name);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("actual.rgba"), actual).unwrap();
    std::fs::write(path.join("expected.rgba"), expected).unwrap();
    for (name, bytes) in fields {
        std::fs::write(path.join(format!("{name}.rgba")), bytes).unwrap();
    }
    std::fs::write(path.join("profile.json"), metadata).unwrap();
}
#[test]
#[ignore = "requires a real raster adapter; normal retained backdrop integration and independent composition reference"]
fn backdrop_normal_renderer_matches_independent_convolution_and_neutral_ordinary_output() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("backdrop coordinator adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut checked = 0;
    let mut maximum = 0;
    let mut profile = 0;
    for clear in [
        wgpu::Color::TRANSPARENT,
        wgpu::Color {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 0.5,
        },
        wgpu::Color {
            r: 0.2,
            g: 0.3,
            b: 0.4,
            a: 1.0,
        },
    ] {
        let base = rows(None, 1.0);
        let prefix = ordinary(&device, &queue, vec![base[0].clone()], clear);
        let paint = ordinary(
            &device,
            &queue,
            vec![base[1].clone()],
            wgpu::Color::TRANSPARENT,
        );
        let mut white = base[1].clone();
        white.base_style.fill = Some(Color::WHITE);
        let mask = ordinary(&device, &queue, vec![white], wgpu::Color::TRANSPARENT);
        for opacity in [0.2, 0.6, 1.0] {
            for sigma in [1.0, 2.0, 3.0] {
                let mut fixture = Fixture::new(&device, &queue);
                let mut scene = runtime(rows(Some(sigma), opacity));
                fixture.render(&device, &queue, &mut scene, clear);
                let actual = read(&device, &queue, &fixture.target);
                let reference = expected(&prefix, &paint, &mask, sigma, f64::from(opacity));
                retain(
                    &format!("case-{profile:03}"),
                    &actual,
                    &reference,
                    &[("prefix", &prefix), ("paint", &paint), ("coverage", &mask)],
                    format!(
                        r#"{{"width":80,"height":60,"sigma":{sigma},"opacity":{opacity},"clear":[{},{},{},{}],"bound":2,"source_raster":"ordinary renderer input; independent filter and composition oracle"}}"#,
                        clear.r, clear.g, clear.b, clear.a
                    ),
                );
                profile += 1;
                for (i, (&a, &b)) in actual.iter().zip(&reference).enumerate() {
                    let error = a.abs_diff(b);
                    maximum = maximum.max(error);
                    checked += 1;
                    assert!(error<=2,"clear={clear:?} opacity={opacity} sigma={sigma} channel={i} actual={a} expected={b}");
                }
                if opacity == 0.6 && sigma == 2.0 && clear == wgpu::Color::TRANSPARENT {
                    let erase = fixture
                        .renderer
                        .effect_captures
                        .as_deref_mut()
                        .unwrap()
                        .scopes
                        .get_mut(&(2, 1))
                        .unwrap()
                        .placement_erase_binding
                        .take();
                    fixture.render(&device, &queue, &mut scene, clear);
                    let wrong = read(&device, &queue, &fixture.target);
                    retain(
                        "negative-source-over",
                        &wrong,
                        &reference,
                        &[],
                        r#"{"width":80,"height":60,"negativeControl":true,"bound":2}"#.to_owned(),
                    );
                    let error = wrong
                        .iter()
                        .zip(&reference)
                        .map(|(&a, &b)| a.abs_diff(b))
                        .max()
                        .unwrap();
                    assert!(
                        error > 2,
                        "negative control must reject ordinary source-over placement"
                    );
                    eprintln!("actual placement negative control maximum error {error}");
                    fixture
                        .renderer
                        .effect_captures
                        .as_deref_mut()
                        .unwrap()
                        .scopes
                        .get_mut(&(2, 1))
                        .unwrap()
                        .placement_erase_binding = erase;
                }
                scene
                    .apply_execution_patch(&ExecutionPatch::SetEffects {
                        object: ObjectId::new(20),
                        effects: panel_effect(0.0),
                    })
                    .unwrap();
                fixture.render(&device, &queue, &mut scene, clear);
                assert_eq!(
                    read(&device, &queue, &fixture.target),
                    ordinary(&device, &queue, rows(None, opacity), clear),
                    "neutral bypass is exact"
                );
            }
        }
    }
    // A later backdrop consumes an earlier backdrop's replacement pair in
    // its four-sample prefix capture, before either is placed in the main view.
    let clear = wgpu::Color {
        r: 0.1,
        g: 0.2,
        b: 0.3,
        a: 0.5,
    };
    let mut base = rows(None, 1.0);
    let mut second = row(30, 0.7, [2.0, 1.4], Color::rgba(0.7, 0.3, 0.1, 0.18));
    let prefix = ordinary(&device, &queue, vec![base[0].clone()], clear);
    let paint_a = ordinary(
        &device,
        &queue,
        vec![base[1].clone()],
        wgpu::Color::TRANSPARENT,
    );
    let mut mask_a = base[1].clone();
    mask_a.base_style.fill = Some(Color::WHITE);
    let mask_a = ordinary(&device, &queue, vec![mask_a], wgpu::Color::TRANSPARENT);
    let paint_b = ordinary(
        &device,
        &queue,
        vec![second.clone()],
        wgpu::Color::TRANSPARENT,
    );
    let mut mask_b = second.clone();
    mask_b.base_style.fill = Some(Color::WHITE);
    let mask_b = ordinary(&device, &queue, vec![mask_b], wgpu::Color::TRANSPARENT);
    let first = expected(&prefix, &paint_a, &mask_a, 2.0, 0.6);
    let reference = expected(&first, &paint_b, &mask_b, 1.0, 0.8);
    base[1].effects = Some(panel_effect(2.0));
    base[1].base_style.opacity = 0.6;
    second.effects = Some(Arc::new(
        CompiledEffectStack::new([CompiledEffect {
            attachment: SemanticNodeId::new(84, 2),
            definition: BackdropBlur::new(EffectUpdate::default().radius(Pixels(1.0)))
                .unwrap()
                .into(),
        }])
        .unwrap(),
    ));
    second.base_style.opacity = 0.8;
    base.push(second);
    let mut fixture = Fixture::new(&device, &queue);
    fixture.render(&device, &queue, &mut runtime(base), clear);
    let actual = read(&device, &queue, &fixture.target);
    retain(
        "ordered-two-backdrops",
        &actual,
        &reference,
        &[("prefix", &prefix)],
        r#"{"width":80,"height":60,"bound":2,"scopes":2}"#.to_owned(),
    );
    let error = actual
        .iter()
        .zip(&reference)
        .map(|(&a, &b)| a.abs_diff(b))
        .max()
        .unwrap();
    assert!(
        error <= 2,
        "ordered backdrop replacement in a later prefix: maximum error {error}"
    );
    eprintln!("two ordered backdrops: maximum error {error}");
    eprintln!("backdrop coordinator: 27 profiles, {checked} independent channels, maximum error {maximum}, 27 exact neutral comparisons");
}

#[test]
#[ignore = "requires a real raster adapter; background-stroke input bounds and actual omitted-source control"]
fn backdrop_prefix_includes_screen_space_background_stroke_outside_its_geometry_box() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let path = noon_core::VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0));
    let source = CompiledObject::new(
        ObjectId::new(10),
        GeometryRef::path(path),
        Transform2D {
            scale: Vec2::new(0.1, 0.1),
            ..Transform2D::IDENTITY
        },
        Style {
            fill: None,
            stroke: None,
            stroke_width: 0.0,
            background_stroke: Some(Color::rgba(1.0, 0.0, 0.0, 1.0)),
            background_stroke_width: 3.0,
            stroke_width_mode: noon_core::StrokeWidthMode::ScreenSpace,
            ..Style::default()
        },
    );
    let mut panel = row(20, 1.3, [0.4, 0.8], Color::rgba(0.1, 0.6, 0.9, 0.27));
    let clear = wgpu::Color::TRANSPARENT;
    let prefix = ordinary(&device, &queue, vec![source.clone()], clear);
    let paint = ordinary(&device, &queue, vec![panel.clone()], clear);
    let mut white = panel.clone();
    white.base_style.fill = Some(Color::WHITE);
    let coverage = ordinary(&device, &queue, vec![white], clear);
    let reference = expected(&prefix, &paint, &coverage, 1.0, 0.6);
    panel.effects = Some(panel_effect(1.0));
    panel.base_style.opacity = 0.6;
    let objects = vec![source, panel];
    let mut fixture = Fixture::new(&device, &queue);
    fixture.render(&device, &queue, &mut runtime(objects.clone()), clear);
    let actual = read(&device, &queue, &fixture.target);
    let maximum = actual
        .iter()
        .zip(&reference)
        .map(|(&a, &b)| a.abs_diff(b))
        .max()
        .unwrap();
    retain("stroke-background-input",&actual,&reference,&[("prefix",&prefix),("paint",&paint),("coverage",&coverage)],
        r#"{"width":80,"height":60,"bound":2,"profile":"scene-width background stroke invariant under object scale"}"#.to_owned());
    assert!(
        maximum <= 2,
        "screen-space background stroke maximum error {maximum}"
    );
    // Run the production selection/capture/composite path with the exact old
    // geometry-only bounds: the target's expanded prefix window misses them.
    let mut broken = Fixture::new(&device, &queue);
    broken.source_bounds_override = Some(noon_core::Rect::new(
        Vec2::new(-0.1, -0.1),
        Vec2::new(0.1, 0.1),
    ));
    broken.render(&device, &queue, &mut runtime(objects.clone()), clear);
    let wrong = read(&device, &queue, &broken.target);
    let error = wrong
        .iter()
        .zip(&reference)
        .map(|(&a, &b)| a.abs_diff(b))
        .max()
        .unwrap();
    retain(
        "negative-geometry-only-stroke-bounds",
        &wrong,
        &reference,
        &[],
        r#"{"width":80,"height":60,"bound":2,"negativeControl":true}"#.to_owned(),
    );
    assert!(
        error > 2,
        "omitted background stroke must fail the same comparator"
    );
    eprintln!("stroked prefix: maximum byte error {maximum}; actual old-bounds negative control {error}, accepted bound 2");
    // A background-only target needs original geometric coverage independent
    // of its translucent background paint. This exercises the separate M stream.
    let mut target = objects[0].clone();
    target.id = ObjectId::new(20);
    target.base_style.background_stroke = Some(Color::rgba(0.1, 0.6, 0.9, 0.27));
    let source = row(10, -0.7, [2.0, 2.4], Color::rgba(0.9, 0.1, 0.2, 0.65));
    let prefix = ordinary(&device, &queue, vec![source.clone()], clear);
    let paint = ordinary(&device, &queue, vec![target.clone()], clear);
    let mut mask = target.clone();
    mask.base_style.background_stroke = Some(Color::WHITE);
    let coverage = ordinary(&device, &queue, vec![mask], clear);
    let reference = expected(&prefix, &paint, &coverage, 1.0, 0.6);
    let mut ordinary_target = target.clone();
    ordinary_target.base_style.opacity = 0.6;
    target.effects = Some(panel_effect(1.0));
    target.base_style.opacity = 0.6;
    let mut scene = runtime(vec![source.clone(), target]);
    let mut fixture = Fixture::new(&device, &queue);
    fixture.render(&device, &queue, &mut scene, clear);
    let actual = read(&device, &queue, &fixture.target);
    let error = actual
        .iter()
        .zip(&reference)
        .map(|(&a, &b)| a.abs_diff(b))
        .max()
        .unwrap();
    retain("stroke-background-target",&actual,&reference,&[("prefix",&prefix),("paint",&paint),("coverage",&coverage)],
        r#"{"width":80,"height":60,"bound":2,"profile":"original background-stroke coverage independent of translucent target paint"}"#.to_owned());
    assert!(
        error <= 2,
        "original background target coverage: maximum error {error}"
    );
    scene
        .apply_execution_patch(&ExecutionPatch::SetEffects {
            object: ObjectId::new(20),
            effects: panel_effect(0.0),
        })
        .unwrap();
    fixture.render(&device, &queue, &mut scene, clear);
    assert_eq!(
        read(&device, &queue, &fixture.target),
        ordinary(&device, &queue, vec![source, ordinary_target], clear),
        "background target neutral bypass remains exact"
    );
    eprintln!(
        "stroked target original coverage: maximum error {error}; neutral ordinary output exact"
    );
}
