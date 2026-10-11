//! Automatic host-facing retained preparation. No per-object effect requests.
//! Public Scene activation and cross-worker transport remain separately gated.
use super::analytic_scene::{expected, frame, semantic_runtime, target, SIGMA, VIEW};
use super::pixels::{device, readback};
use crate::text::TextDeviceMetrics;
use crate::{
    Camera2D, GpuRenderer, RetainedFramePreparer, RetainedGlowStats, RetainedTextGpuState,
};
use noon_compile::{
    CompiledGlow, CompiledObject, CompiledResources, CompiledScene, ExecutionPatch,
};
use noon_core::{
    Color, GeometryResourceArena, Glow, GlowSource, GlowUpdate, ObjectContentRef, ObjectId, Pixels,
    SemanticNodeId, Style, TextResourceArena, Transform2D, Vec2,
};
use noon_runtime::{FrameState, SceneInstance};
use std::sync::Arc;

fn text_objects(source: Option<&FrameState>, glow: Option<Glow>) -> SceneInstance {
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
                    object.glow = glow.map(|definition| {
                        Arc::new(CompiledGlow {
                            attachment: SemanticNodeId::new(900, 1),
                            definition,
                        })
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
    if glow.is_some() {
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
}
impl Host {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(device, queue, VIEW[0], VIEW[1]);
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        );
        let text = renderer.create_retained_text_state(device, queue);
        Self {
            renderer,
            text,
            preparer: RetainedFramePreparer::new(),
            target: target(device, VIEW),
        }
    }
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        runtime: &mut SceneInstance,
        omit_source: bool,
    ) -> (Vec<u8>, RetainedGlowStats) {
        let publication = runtime.take_renderer_publication();
        let visible = publication
            .painter_order()
            .iter()
            .map(|&i| i as usize)
            .filter(|&i| !omit_source || i != 1)
            .collect::<Vec<_>>();
        let expanded = self
            .renderer
            .glow_source_visibility(&publication, &visible)
            .unwrap();
        let metrics = TextDeviceMetrics::uniform(10.0)
            .unwrap()
            .with_world_origin_pixels(Vec2::new(40.0, 30.0))
            .unwrap();
        let prepared = self
            .preparer
            .prepare_planned_publication_visible(device, &publication, expanded, metrics)
            .unwrap();
        self.renderer
            .upload_retained(device, queue, &prepared, &mut self.text);
        let mut encoder = device.create_command_encoder(&Default::default());
        let stats = self
            .renderer
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
    let mut glow = **row.glow.as_ref().unwrap();
    glow.definition = GlowUpdate::default()
        .intensity(intensity)
        .apply_to(glow.definition)
        .unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::SetGlow {
            object: row.id,
            glow: Arc::new(glow),
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
                GlowUpdate::default()
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
            let edited_definition = runtime.frame().objects[1].glow.as_ref().unwrap().definition;
            let mut edited_reference =
                expected(&device, &queue, runtime.frame(), edited_definition);
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
                (0, 0, 1, 32)
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
}
