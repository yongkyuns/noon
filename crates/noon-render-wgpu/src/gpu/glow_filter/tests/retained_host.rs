//! Automatic host-facing retained preparation. No per-object effect requests.
//! These raster/resource tests complement authored Scene and worker qualification.
use super::analytic_scene::{expected, frame, semantic_runtime, target, SIGMA, VIEW};
use super::pixels::{device, readback};
use crate::text::TextDeviceMetrics;
use crate::{
    Camera2D, GpuRenderer, RetainedEffectStats, RetainedFramePreparer, RetainedTextGpuState,
};
use noon_compile::{
    CompiledEffect, CompiledObject, CompiledResources, CompiledScene, ExecutionPatch,
};
use noon_core::{
    Color, EffectUpdate, GeometryResourceArena, Glow, GlowSource, ObjectContentRef, ObjectId,
    Pixels, SemanticNodeId, Style, TextResourceArena, Transform2D, Vec2,
};
use noon_runtime::{FrameState, SceneInstance};
use std::sync::Arc;

fn text_objects(source: Option<&FrameState>, effect: Option<Glow>) -> SceneInstance {
    let mut objects = source.map_or_else(Vec::new, |frame| {
        frame
            .objects
            .iter()
            .take(3)
            .enumerate()
            .map(|(i, row)| {
                let mut object =
                    CompiledObject::new(row.id, row.content.clone(), row.transform, row.style);
                if i == 1 {
                    object.effects = effect.map(|definition| {
                        Arc::new(
                            noon_compile::CompiledEffectStack::new([CompiledEffect {
                                attachment: SemanticNodeId::new(900, 1),
                                definition: definition.into(),
                            }])
                            .unwrap(),
                        )
                    });
                }
                object
            })
            .collect()
    });
    let artifact = noon_typst::compile_typst_resource("A", noon_typst::TypstMode::Markup).unwrap();
    let mut texts = TextResourceArena::new();
    let handle = texts.insert(artifact.resource).unwrap();
    let mut resources = CompiledResources::default();
    let bounds = resources
        .capture_text_from_arenas(
            &texts,
            &artifact.fonts,
            &GeometryResourceArena::new(),
            handle,
        )
        .unwrap();
    let mut text = CompiledObject::new(
        ObjectId::new(1000),
        ObjectContentRef::Text(handle),
        Transform2D {
            translation: Vec2::new(-0.1, 0.25),
            ..Transform2D::IDENTITY
        },
        Style {
            fill: Some(Color::WHITE),
            stroke: None,
            ..Style::default()
        },
    );
    text.text_bounds = Some(bounds);
    text.base_z_index = 100.0;
    objects.push(text);
    let mut compiled = CompiledScene::compile_objects(objects, &[]).unwrap();
    compiled.merge_prepared_resources(resources);
    let mut runtime = SceneInstance::new(compiled);
    if effect.is_some() {
        // Mixed geometry/text must use real dirty motion publications too.
        let row = &runtime.frame().objects[1];
        runtime
            .apply_execution_patch(&ExecutionPatch::AddTrack(noon_core::TrackDefinition {
                id: noon_core::TrackId::new(1),
                object: row.id,
                property: noon_core::Property::Position,
                values: noon_core::TrackValues::Vec2 {
                    from: row.transform.translation,
                    to: row.transform.translation + Vec2::new(0.35, 0.0),
                },
                timing: noon_core::TrackTiming::new(0.0, 1.0, noon_core::RateFunction::Linear),
                time_map: Default::default(),
            }))
            .unwrap();
    }
    runtime
}

struct Host {
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    target: wgpu::Texture,
    last_geometry_bytes: usize,
    last_geometry_misses: usize,
    effect_budget: u64,
}
impl Host {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::with_view(device, queue, VIEW, 1_000_000)
    }
    fn with_view(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: [u32; 2],
        effect_budget: u64,
    ) -> Self {
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(device, queue, view[0], view[1]);
        renderer.set_camera(
            queue,
            Camera2D::new(
                Vec2::ZERO,
                Vec2::new(6.0 * view[0] as f32 / view[1] as f32, 6.0),
            )
            .unwrap(),
        );
        let text = renderer.create_retained_text_state(device, queue);
        Self {
            renderer,
            text,
            preparer: RetainedFramePreparer::new(),
            target: target(device, view),
            last_geometry_bytes: 0,
            last_geometry_misses: 0,
            effect_budget,
        }
    }
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        runtime: &mut SceneInstance,
        omit_source: bool,
    ) -> (Vec<u8>, RetainedEffectStats) {
        let publication = runtime.take_renderer_publication();
        let visible = publication
            .painter_order()
            .iter()
            .map(|&i| i as usize)
            .filter(|&i| !omit_source || i != 1)
            .collect::<Vec<_>>();
        let expanded = self
            .renderer
            .effect_source_visibility(&publication, &visible)
            .unwrap();
        let metrics = TextDeviceMetrics::uniform(self.target.height() as f32 / 6.0)
            .unwrap()
            .with_world_origin_pixels(Vec2::new(
                self.target.width() as f32 / 2.0,
                self.target.height() as f32 / 2.0,
            ))
            .unwrap();
        let prepared = self
            .preparer
            .prepare_planned_publication_visible(device, &publication, expanded, metrics)
            .unwrap();
        self.last_geometry_misses = prepared.geometry_stats().geometry_cache_misses;
        self.last_geometry_bytes = self
            .renderer
            .upload_retained(device, queue, &prepared, &mut self.text)
            .geometry
            .bytes_uploaded;
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
                wgpu::Color::TRANSPARENT,
                self.effect_budget,
            )
            .unwrap();
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
        queue.submit([encoder.finish()]);
        (readback(device, queue, &self.target), stats)
    }
}

fn composite(dst: &mut [u8], src: &[u8]) {
    for (dst, src) in dst
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(src.as_chunks::<4>().0)
    {
        let a = 1.0 - f64::from(src[3]) / 255.0;
        for (out, &front) in dst.iter_mut().zip(src.iter()) {
            *out = (f64::from(front) + a * f64::from(*out))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}
fn edit_intensity(runtime: &mut SceneInstance, intensity: f64) {
    let row = &runtime.frame().objects[1];
    let mut effect = super::only_effect(row.effects.as_ref().unwrap());
    effect.definition = EffectUpdate::default()
        .intensity(intensity)
        .apply_to(effect.definition)
        .unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::SetEffects {
            object: row.id,
            effects: Arc::new(noon_compile::CompiledEffectStack::new([effect]).unwrap()),
        })
        .unwrap();
}

#[test]
#[ignore = "requires native raster adapter; automatic retained host preparation and visibility"]
fn automatic_retained_host_glow_pixels() {
    let (device, queue) = device();
    let mut frames = 0;
    for mixed in [false, true] {
        for (rectangle, offscreen, transparent) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (true, true, true),
            (false, false, true),
        ] {
            let source = frame(rectangle, offscreen, transparent);
            let definition = Glow::new(
                EffectUpdate::default()
                    .radius(Pixels(SIGMA))
                    .intensity(2.4)
                    .color(Color::rgba(0.95, 0.35, 0.8, 0.8))
                    .source(if transparent {
                        GlowSource::Silhouette
                    } else {
                        GlowSource::Painted
                    }),
            )
            .unwrap();
            let mut runtime = if mixed {
                text_objects(Some(&source), Some(definition))
            } else {
                semantic_runtime(&source, definition)
            };
            let overlay = if mixed {
                let mut text_runtime = text_objects(None, None);
                let mut text_host = Host::new(&device, &queue);
                let image = text_host
                    .render(&device, &queue, &mut text_runtime, false)
                    .0;
                assert!(
                    image.as_chunks::<4>().0.iter().any(|p| p[3] > 200),
                    "text reference must contribute"
                );
                Some(image)
            } else {
                None
            };
            let mut host = Host::new(&device, &queue);
            let mut first = None;
            for time in [0.0, 0.37, 0.8, 0.0] {
                runtime.seek(time).unwrap();
                let mut expected = expected(&device, &queue, runtime.frame(), definition);
                if let Some(overlay) = &overlay {
                    composite(&mut expected, overlay);
                }
                let (actual, stats) = host.render(&device, &queue, &mut runtime, offscreen);
                let maximum = actual
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                eprintln!("automatic-host mixed={mixed} rectangle={rectangle} offscreen={offscreen} transparent={transparent} time={time} max_byte_error={maximum}");
                assert!(
                    maximum <= 2,
                    "automatic retained host pixel mismatch: {maximum}"
                );
                assert!(stats.retained_texture_bytes > 0);
                if time == 0.0 {
                    if let Some(first) = &first {
                        assert_eq!(&actual, first);
                    } else {
                        first = Some(actual);
                    }
                }
                frames += 1;
            }
            edit_intensity(&mut runtime, 0.5);
            let (edited_image, edit) = host.render(&device, &queue, &mut runtime, offscreen);
            let edited_definition =
                super::only_effect(runtime.frame().objects[1].effects.as_ref().unwrap()).definition;
            let mut edited_reference = expected(
                &device,
                &queue,
                runtime.frame(),
                (edited_definition).as_glow().unwrap(),
            );
            if let Some(overlay) = &overlay {
                composite(&mut edited_reference, overlay);
            }
            let maximum = edited_image
                .iter()
                .zip(&edited_reference)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            eprintln!("automatic-host intensity mixed={mixed} rectangle={rectangle} offscreen={offscreen} transparent={transparent} max_byte_error={maximum}");
            assert!(
                maximum <= 2,
                "automatic retained intensity image mismatch: {maximum}"
            );
            assert_eq!(
                (
                    edit.source_passes,
                    edit.blur_passes,
                    edit.composite_passes,
                    edit.bytes_uploaded
                ),
                (0, 0, 0, 32)
            );
            edit_intensity(&mut runtime, 0.0);
            let (neutral, _) = host.render(&device, &queue, &mut runtime, offscreen);
            let mut ordinary = if mixed {
                text_objects(Some(runtime.frame()), None)
            } else {
                let rows = runtime
                    .frame()
                    .objects
                    .iter()
                    .map(|row| {
                        CompiledObject::new(row.id, row.content.clone(), row.transform, row.style)
                    })
                    .collect();
                SceneInstance::new(CompiledScene::compile_objects(rows, &[]).unwrap())
            };
            let plain = Host::new(&device, &queue)
                .render(&device, &queue, &mut ordinary, false)
                .0;
            assert_eq!(
                neutral, plain,
                "neutral host output must equal ordinary retained output"
            );
            let halo_signal = first
                .as_ref()
                .unwrap()
                .iter()
                .zip(&neutral)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(
                halo_signal > 2,
                "automatic path must show a visible halo, mixed={mixed}"
            );
            frames += 2;
        }
    }
    eprintln!("automatic-host frames={frames}; full-image comparisons=50; neutral comparisons=10");
    g4_locality_and_retirement(&device, &queue);
    batched_host_matches_scalar(&device, &queue);
}

fn batched_host_matches_scalar(device: &wgpu::Device, queue: &wgpu::Queue) {
    let effects = |index: usize, phase: usize, scalar: bool| {
        let mut effects = vec![CompiledEffect {
            attachment: SemanticNodeId::new(100 + index as u32, 1),
            definition: Glow::new(
                EffectUpdate::default()
                    .radius(Pixels(2.1 + index as f64 * 0.1 + phase as f64 * 0.15))
                    .intensity(2.4)
                    .color(Color::rgba(0.2 + index as f32 * 0.15, 0.4, 0.8, 0.8)),
            )
            .unwrap()
            .into(),
        }];
        if scalar {
            // An identity suffix uses the established materialized path.
            effects.push(CompiledEffect {
                attachment: SemanticNodeId::new(200 + index as u32, 1),
                definition: noon_core::Blur::new(EffectUpdate::default().radius(Pixels(0.0)))
                    .unwrap()
                    .into(),
            });
        }
        Arc::new(noon_compile::CompiledEffectStack::new(effects).unwrap())
    };
    let scene = |scalar| {
        SceneInstance::new(
            CompiledScene::compile_objects(
                (0..5)
                    .map(|i| {
                        let mut object = CompiledObject::new(
                            ObjectId::new(i as u64 + 1),
                            noon_core::GeometryRef::circle(0.2),
                            Transform2D {
                                translation: Vec2::new(-2.8 + i as f32 * 1.4, 0.0),
                                ..Transform2D::IDENTITY
                            },
                            Style {
                                fill: Some(Color::rgba(0.9, 0.2, 0.4, 0.3 + i as f32 * 0.1)),
                                stroke: None,
                                opacity: 0.75,
                                ..Style::default()
                            },
                        );
                        object.effects = Some(effects(i, 0, scalar));
                        object
                    })
                    .collect(),
                &[],
            )
            .unwrap(),
        )
    };
    let mut batched = scene(false);
    let mut scalar = scene(true);
    let mut host = Host::new(device, queue);
    let mut reference = Host::new(device, queue);
    let mut previous = Vec::new();
    for step in 0..4 {
        if step == 1 || step == 2 {
            for (runtime, scalar) in [(&mut batched, false), (&mut scalar, true)] {
                for i in if step == 1 { 0..5 } else { 2..3 } {
                    runtime
                        .apply_execution_patch(&ExecutionPatch::SetEffects {
                            object: ObjectId::new(i as u64 + 1),
                            effects: effects(i, step, scalar),
                        })
                        .unwrap();
                }
            }
        }
        let (actual, work) = host.render(device, queue, &mut batched, false);
        let (expected, scalar_work) = reference.render(device, queue, &mut scalar, false);
        assert_eq!(
            work.blur_passes,
            match step {
                0 | 1 => 4,
                2 => 2,
                _ => 0,
            }
        );
        assert_eq!(
            scalar_work.blur_passes,
            match step {
                0 | 1 => 10,
                2 => 2,
                _ => 0,
            }
        );
        let error = actual
            .iter()
            .zip(&expected)
            .map(|(&a, &b)| a.abs_diff(b))
            .max()
            .unwrap();
        assert!(
            error <= 2,
            "retained batched/scalar image mismatch at step {step}: {error}"
        );
        if step == 1 || step == 2 {
            assert_ne!(actual, previous, "radius changes must reach pixels");
        }
        if step == 2 {
            assert_eq!(
                (
                    work.rows_examined,
                    work.sources_prepared,
                    work.texture_allocations,
                    work.pipeline_compiles
                ),
                (1, 1, 0, 0)
            );
        }
        if step == 3 {
            assert_eq!(actual, previous);
        }
        previous = actual;
    }
}

// Renderer/resource qualification, not Scene lowering or physical pacing evidence.
fn g4_locality_and_retirement(device: &wgpu::Device, queue: &wgpu::Queue) {
    let glow = |index: usize, generation: u32, intensity: f64| {
        Arc::new(
            noon_compile::CompiledEffectStack::new([CompiledEffect {
                attachment: SemanticNodeId::new(600 + index as u32, generation),
                definition: Glow::new(
                    EffectUpdate::default()
                        .radius(Pixels(3.0))
                        .intensity(intensity),
                )
                .unwrap()
                .into(),
            }])
            .unwrap(),
        )
    };
    let scene = |active: usize| {
        let objects = (0..600)
            .map(|index| {
                let mut object = CompiledObject::new(
                    ObjectId::new(index as u64),
                    noon_core::GeometryRef::circle(0.05),
                    Transform2D {
                        translation: Vec2::new(
                            -3.75 + (index % 30) as f32 * 0.25,
                            -2.375 + (index / 30) as f32 * 0.25,
                        ),
                        ..Transform2D::IDENTITY
                    },
                    Style {
                        fill: Some(Color::WHITE),
                        stroke: None,
                        ..Style::default()
                    },
                );
                if index < active {
                    object.effects = Some(glow(index, 1, 1.0));
                }
                object
            })
            .collect();
        SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap())
    };
    for view in [[1152, 648], [2304, 1296], [1920, 1080]] {
        for active in [1, 100] {
            let mut runtime = scene(active);
            let budget = if view == [1152, 648] { 32 } else { 64 } * 1024 * 1024;
            let mut host = Host::with_view(device, queue, view, budget);
            let (initial, warm) = host.render(device, queue, &mut runtime, false);
            assert_eq!(warm.rows_examined, 600);
            assert_eq!(
                warm.blur_passes,
                if active == 1 { 2 } else { 50 },
                "all active G4 masks must be encoded"
            );
            assert!(warm.retained_texture_bytes > 0 && warm.retained_texture_bytes <= budget);
            let (_, clean) = host.render(device, queue, &mut runtime, false);
            assert_eq!(
                clean,
                RetainedEffectStats {
                    retained_texture_bytes: warm.retained_texture_bytes,
                    ..Default::default()
                }
            );
            for index in 0..active {
                runtime
                    .apply_execution_patch(&ExecutionPatch::SetEffects {
                        object: ObjectId::new(index as u64),
                        effects: glow(index, 1, 0.4),
                    })
                    .unwrap();
            }
            assert_eq!(runtime.frame_changes().object_indices().len(), active);
            let (edited, work) = host.render(device, queue, &mut runtime, false);
            assert!(edited != initial, "intensity edit must reach actual pixels");
            assert_eq!(
                (host.last_geometry_bytes, host.last_geometry_misses),
                (0, 0)
            );
            assert_eq!(
                (
                    work.rows_examined,
                    work.sources_prepared,
                    work.source_passes,
                    work.blur_passes,
                    work.composite_passes,
                    work.bytes_uploaded,
                    work.texture_allocations,
                    work.pipeline_compiles
                ),
                (active, active, 0, 0, 0, 32 * active, 0, 0)
            );
            assert_eq!(work.retained_texture_bytes, warm.retained_texture_bytes);
            eprintln!("G4 view={view:?} active={active}/600 resident={} intensity_bytes={} geometry_bytes=0 allocations=0 compiles=0", work.retained_texture_bytes, work.bytes_uploaded);
        }
    }
    let mut runtime = scene(0);
    let mut host = Host::new(device, queue);
    let (ordinary, absent) = host.render(device, queue, &mut runtime, false);
    assert_eq!(
        (absent.sources_prepared, absent.retained_texture_bytes),
        (0, 0)
    );
    assert_eq!(
        host.render(device, queue, &mut runtime, false).1,
        RetainedEffectStats::default()
    );
    let mut plateau = None;
    let mut compiles = 0;
    let mut allocations = 0;
    for generation in 1..=1000 {
        runtime
            .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                object: ObjectId::new(0),
                expected: Box::new([]),
                effects: Some(glow(0, generation, 1.0)),
            })
            .unwrap();
        let (_, attached) = host.render(device, queue, &mut runtime, false);
        compiles += attached.pipeline_compiles;
        allocations += attached.texture_allocations;
        if generation > 1 {
            assert_eq!(
                attached.pipeline_compiles, 0,
                "retired attachments reuse filter programs"
            );
        }
        assert_eq!(attached.rows_examined, 1);
        assert_eq!(
            attached.retained_texture_bytes,
            *plateau.get_or_insert(attached.retained_texture_bytes)
        );
        runtime
            .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                object: ObjectId::new(0),
                expected: Box::new([SemanticNodeId::new(600, generation)]),
                effects: None,
            })
            .unwrap();
        let (removed, work) = host.render(device, queue, &mut runtime, false);
        assert!(
            removed == ordinary,
            "retirement must restore ordinary pixels"
        );
        assert_eq!((work.rows_examined, work.retained_texture_bytes), (1, 0));
        assert!(!host.renderer.has_retained_effect_sources());
    }
    assert_eq!(
        host.render(device, queue, &mut runtime, false).1,
        RetainedEffectStats::default()
    );
    eprintln!("G4 attach/remove cycles=1000 resident_plateau={} retired_bytes=0 pipeline_compiles={compiles} texture_allocations={allocations}", plateau.unwrap());
}
