//! Native retained-work evidence for radial visibility over fractal paint.
//!
//! This deliberately requests software Vulkan. Timings are observations of this
//! offscreen test, not physical-device FPS or a performance admission threshold.
//! "Cold" means a new renderer on the same device; driver caches may be warm.
//! Existing counters cover retained textures and GPU resource work, not total
//! host heap allocations, in-flight submissions, or driver-owned memory.
use noon::integration::{PixelChannelOrder, PixelRowOrder, Rgba8ReadbackLayout};
use noon::{
    effects::{EffectUpdate, RadialEdge, RadialMask},
    AnimationOptions, Color, DeclaredAnimation, Mobject, NoisePaint, RateFunction, Scene,
    SceneExecution,
};
use noon_core::Vec2;
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, EffectProgramStats, GpuRenderer, RenderStats,
    RetainedEffectStats, RetainedFrameIncrementalStats, RetainedFramePreparer,
    RetainedTextGpuState, RetainedUploadStats, DEFAULT_EFFECT_TEXTURE_BUDGET,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const SIZE: [u32; 2] = [256, 192];
const COLD_SAMPLES: usize = 3;
const STEADY_SAMPLES: usize = 16;
const RETIREMENT_CYCLES: usize = 4;

#[derive(Clone, Copy, Debug)]
enum Authoring {
    Convenience,
    Explicit,
}

fn radial_value() -> RadialMask {
    RadialMask::new(
        EffectUpdate::default()
            .progress(0.25)
            .seed(7)
            .softness(0.025)
            .reveal_roughness(0.10)
            .reveal_rim_width(0.12),
    )
    .unwrap()
}

fn attach(scene: &mut Scene, target: &Mobject, authoring: Authoring) {
    match authoring {
        Authoring::Convenience => scene
            .set_reveal_with_edge(
                target,
                EffectUpdate::default().progress(0.25).seed(7),
                Some(RadialEdge::Burn),
            )
            .unwrap(),
        Authoring::Explicit => scene.add_effect(target, radial_value(), "iris").unwrap(),
    }
}

fn detach(scene: &mut Scene, target: &Mobject, authoring: Authoring) {
    match authoring {
        Authoring::Convenience => scene.remove_reveal(target).unwrap(),
        Authoring::Explicit => scene.remove_effect(target, "iris").unwrap(),
    }
}

fn paint(phase: [f64; 2]) -> NoisePaint {
    NoisePaint::fractal(Color::rgb(0.05, 0.1, 0.25), Color::rgb(0.5, 0.8, 1.0))
        .unwrap()
        .with_seed(7)
        .with_phase(phase)
        .unwrap()
}

fn fixture(
    static_count: usize,
    target_count: usize,
    authoring: Authoring,
) -> (SceneExecution, Vec<Mobject>, DeclaredAnimation) {
    let mut scene = Scene::new();
    for index in 0..static_count {
        let mut dot = scene.circle(0.03).unwrap();
        dot.disable_stroke().unwrap();
        dot.set_fill(0.15, 0.4, 0.2, 1.0).unwrap();
        dot.set_translation(
            -3.875 + (index % 32) as f64 * 0.25,
            -2.90625 + (index / 32) as f64 * 0.1875,
        )
        .unwrap();
        scene.add(&dot).unwrap();
    }
    let mut targets = Vec::new();
    for index in 0..target_count {
        let mut panel = scene.rectangle(1.0, 0.75).unwrap();
        panel.disable_stroke().unwrap();
        panel.set_fill_noise(paint([0.0; 2]), Some(1.0)).unwrap();
        if target_count > 1 {
            panel
                .set_translation(
                    -2.25 + (index % 4) as f64 * 1.5,
                    -0.75 + (index / 4) as f64 * 1.5,
                )
                .unwrap();
        }
        attach(&mut scene, &panel, authoring);
        scene.add(&panel).unwrap();
        targets.push(panel);
    }
    let mut endpoint = targets[0].target_editor().unwrap();
    match authoring {
        Authoring::Convenience => endpoint
            .set_reveal(EffectUpdate::default().progress(0.75))
            .unwrap(),
        Authoring::Explicit => endpoint
            .set_effect("iris", EffectUpdate::default().progress(0.75))
            .unwrap(),
    }
    let animation = scene
        .declare_transform_to(
            &targets[0],
            &endpoint,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    (scene.into_execution().unwrap(), targets, animation)
}

#[derive(Clone, Copy, Debug)]
struct Work {
    changed_rows: usize,
    programs: EffectProgramStats,
    effects: RetainedEffectStats,
    geometry: RenderStats,
    upload: RetainedUploadStats,
    host_us: f64,
    synchronized_us: f64,
}
impl Work {
    fn assert_clean(self, retained_bytes: u64) {
        assert_eq!(self.changed_rows, 0);
        assert_eq!(self.programs, EffectProgramStats::default());
        assert_eq!(
            self.effects,
            RetainedEffectStats {
                retained_texture_bytes: retained_bytes,
                ..Default::default()
            }
        );
        assert_eq!(self.upload, RetainedUploadStats::default());
        self.assert_geometry_local(0);
    }

    fn assert_geometry_local(self, changed_rows: usize) {
        assert_eq!(self.geometry.full_rebuilds, 0);
        assert!(self.geometry.instances_repacked <= changed_rows);
        assert_eq!(self.geometry.capacity_growths, 0);
        assert_eq!(self.geometry.geometry_cache_misses, 0);
        assert_eq!(self.geometry.path_vertices_repacked, 0);
        assert_eq!(self.geometry.path_indices_repacked, 0);
        assert_eq!(self.geometry.render_order_positions_visited, 0);
        assert_eq!(self.upload.geometry.buffer_reallocations, 0);
    }
}

// The same public retained preparation/capture/encode path used by the native
// pixel fixtures. Visibility candidates and readback storage are allocated once;
// the timed loop adds no harness-owned walk over the unrelated static content.
struct Surface {
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    target: wgpu::Texture,
    readback: wgpu::Buffer,
    layout: Rgba8ReadbackLayout,
    candidates: Vec<usize>,
}
impl Surface {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, object_count: usize) -> Self {
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(device, queue, SIZE[0], SIZE[1]);
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        );
        let text = renderer.create_retained_text_state(device, queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("semantic visual efficiency output"),
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
        let layout = Rgba8ReadbackLayout::new(
            SIZE[0],
            SIZE[1],
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
            DEFAULT_EFFECT_TEXTURE_BUDGET,
        )
        .unwrap();
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("semantic visual efficiency retained readback"),
            size: layout.buffer_len() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            renderer,
            preparer: RetainedFramePreparer::new(),
            text,
            target,
            readback,
            layout,
            candidates: (0..object_count).collect(),
        }
    }

    fn capture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        running: &mut SceneExecution,
    ) -> (Vec<u8>, Work) {
        let start = Instant::now();
        let publication = running.session_mut().take_renderer_publication();
        let changed_rows = if publication.changes().is_all() {
            publication.frame().objects.len()
        } else {
            publication.changes().object_indices().len()
        };
        let programs =
            pollster::block_on(self.renderer.prepare_effect_programs(device, &publication))
                .unwrap();
        let visible = self
            .renderer
            .effect_source_visibility(&publication, &self.candidates)
            .unwrap();
        let prepared = self
            .preparer
            .prepare_planned_publication_visible(
                device,
                &publication,
                visible,
                TextDeviceMetrics::uniform(32.0).unwrap(),
            )
            .unwrap();
        let geometry = prepared.geometry_stats();
        let upload = self
            .renderer
            .upload_retained(device, queue, &prepared, &mut self.text);
        let mut encoder = device.create_command_encoder(&Default::default());
        let effects = self
            .renderer
            .prepare_retained_effects(
                device,
                queue,
                &mut encoder,
                &prepared,
                &self.text,
                &publication,
                wgpu::Color::BLACK,
                DEFAULT_EFFECT_TEXTURE_BUDGET,
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
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.layout.padded_bytes_per_row()),
                    rows_per_image: Some(SIZE[1]),
                },
            },
            self.target.size(),
        );
        queue.submit([encoder.finish()]);
        let host_us = start.elapsed().as_secs_f64() * 1e6;
        let (sender, receiver) = std::sync::mpsc::channel();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap()
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .unwrap();
        receiver
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let pixels = {
            let bytes = self.readback.slice(..).get_mapped_range().unwrap();
            self.layout
                .copy_rgba8(&bytes, PixelChannelOrder::Rgba, PixelRowOrder::TopToBottom)
                .unwrap()
        };
        self.readback.unmap();
        (
            pixels,
            Work {
                changed_rows,
                programs,
                effects,
                geometry,
                upload,
                host_us,
                synchronized_us: start.elapsed().as_secs_f64() * 1e6,
            },
        )
    }
}

fn distribution(values: &[f64]) -> Value {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    json!({
        "samples": values.len(), "min": values[0], "p50": values[values.len() / 2],
        "p95": values[(values.len() * 95).div_ceil(100) - 1], "max": values[values.len() - 1],
    })
}

fn timings(samples: &[Work]) -> Value {
    json!({
        "host_prepare_encode_submit_us": distribution(&samples.iter().map(|s| s.host_us).collect::<Vec<_>>()),
        "synchronized_capture_readback_us": distribution(&samples.iter().map(|s| s.synchronized_us).collect::<Vec<_>>()),
    })
}

fn assert_no_global_rebuild(
    before: RetainedFrameIncrementalStats,
    after: RetainedFrameIncrementalStats,
) {
    assert_eq!(after.scratch_rebuilds, before.scratch_rebuilds);
    assert_eq!(after.mixed_order_rebuilds, before.mixed_order_rebuilds);
    assert_eq!(after.text_snapshot_copies, before.text_snapshot_copies);
    assert_eq!(
        after.image_painter_order_rekeys,
        before.image_painter_order_rekeys
    );
}

struct CaseResult {
    initial_pixels: Vec<u8>,
    local_filter_bytes: usize,
    local_paint_bytes: usize,
    retained_bytes: u64,
}

fn qualify_case(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    static_count: usize,
    target_count: usize,
    authoring: Authoring,
) -> CaseResult {
    let (mut running, targets, animation) = fixture(static_count, target_count, authoring);
    let static_rows = running.session().frame().objects[..static_count].to_vec();
    let mut setup_us = Vec::new();
    let mut cold = Vec::new();
    let mut surface = None;
    let mut initial_pixels = None;
    for _ in 0..COLD_SAMPLES {
        let start = Instant::now();
        let mut candidate = Surface::new(device, queue, static_count + target_count);
        setup_us.push(start.elapsed().as_secs_f64() * 1e6);
        let (pixels, work) = candidate.capture(device, queue, &mut running);
        assert_eq!(
            &pixels,
            initial_pixels.get_or_insert_with(|| pixels.clone())
        );
        assert_eq!(work.effects.source_passes, target_count);
        assert_eq!(
            work.effects.pipeline_compiles, 1,
            "one shared radial operator pipeline"
        );
        assert_eq!(
            work.programs.pipeline_compiles, 0,
            "no authored WGSL program"
        );
        assert!(work.effects.retained_texture_bytes > 0);
        cold.push(work);
        surface = Some(candidate);
    }
    let mut surface = surface.unwrap();
    let initial_pixels = initial_pixels.unwrap();
    let retained_bytes = cold.last().unwrap().effects.retained_texture_bytes;
    let before = surface.preparer.incremental_stats();
    let mut clean = Vec::new();
    for _ in 0..STEADY_SAMPLES {
        let (pixels, work) = surface.capture(device, queue, &mut running);
        assert_eq!(pixels, initial_pixels);
        work.assert_clean(retained_bytes);
        clean.push(work);
    }
    assert_no_global_rebuild(before, surface.preparer.incremental_stats());

    let segment = running.live().play_animation(&animation).unwrap();
    surface.capture(device, queue, &mut running); // Drain animation enrollment.
    let context = running.session().publication_context();
    let mut epoch = context.frame_epoch();
    let before = surface.preparer.incremental_stats();
    let mut animated = Vec::new();
    let mut advance_us = Vec::new();
    let mut last_pixels = initial_pixels.clone();
    for step in 1..=STEADY_SAMPLES {
        let start = Instant::now();
        running
            .live()
            .advance_segment_to(segment, step as f64 / STEADY_SAMPLES as f64)
            .unwrap();
        advance_us.push(start.elapsed().as_secs_f64() * 1e6);
        let current = running.session().publication_context();
        assert_eq!(current.scene_revision(), context.scene_revision());
        assert_eq!(current.execution_revision(), context.execution_revision());
        assert_ne!(current.frame_epoch(), epoch);
        epoch = current.frame_epoch();
        let (pixels, work) = surface.capture(device, queue, &mut running);
        assert_eq!(work.changed_rows, 1);
        assert_eq!(work.programs.rows_examined, 1);
        assert_eq!(work.programs.pipeline_compiles, 0);
        assert_eq!(work.effects.rows_examined, 1);
        assert_eq!(work.effects.sources_prepared, 1);
        assert_eq!(
            work.effects.source_passes, 0,
            "radial progress reuses its source capture"
        );
        assert_eq!(work.effects.pipeline_compiles, 0);
        assert_eq!(work.effects.texture_allocations, 0);
        assert_eq!(work.effects.retained_texture_bytes, retained_bytes);
        assert!(work.effects.bytes_uploaded > 0);
        assert_eq!(work.upload, RetainedUploadStats::default());
        work.assert_geometry_local(1);
        animated.push(work);
        last_pixels = pixels;
        let (paused, clean_work) = surface.capture(device, queue, &mut running);
        assert_eq!(paused, last_pixels);
        clean_work.assert_clean(retained_bytes);
    }
    assert_ne!(
        last_pixels, initial_pixels,
        "the timed mask must actually change pixels"
    );
    assert_no_global_rebuild(before, surface.preparer.incremental_stats());
    assert_eq!(
        &running.session().frame().objects[..static_count],
        static_rows
    );
    let local_filter_bytes = animated[0].effects.bytes_uploaded;
    assert!(animated
        .iter()
        .all(|work| work.effects.bytes_uploaded == local_filter_bytes));

    running.live().complete_segment(segment).unwrap();
    surface.capture(device, queue, &mut running); // Drain authored endpoint publication.
    running
        .scene_mut()
        .set_fill_noise(&targets[0], paint([0.41, -0.13]), Some(1.0))
        .unwrap();
    let (changed_paint, paint_work) = surface.capture(device, queue, &mut running);
    assert_ne!(changed_paint, last_pixels);
    assert_eq!(paint_work.changed_rows, 1);
    assert_eq!(paint_work.effects.rows_examined, 1);
    assert_eq!(paint_work.effects.sources_prepared, 1);
    assert_eq!(
        paint_work.effects.source_passes, 1,
        "only the edited paint recaptures"
    );
    assert_eq!(paint_work.effects.pipeline_compiles, 0);
    assert_eq!(paint_work.effects.texture_allocations, 0);
    assert_eq!(paint_work.effects.retained_texture_bytes, retained_bytes);
    assert!(paint_work.upload.geometry.bytes_uploaded > 0);
    paint_work.assert_geometry_local(1);
    let (paused, work) = surface.capture(device, queue, &mut running);
    assert_eq!(paused, changed_paint);
    work.assert_clean(retained_bytes);
    assert_eq!(
        &running.session().frame().objects[..static_count],
        static_rows
    );

    for target in &targets {
        detach(running.scene_mut(), target, authoring);
    }
    let (ordinary, retired) = surface.capture(device, queue, &mut running);
    assert_eq!(retired.changed_rows, target_count);
    assert_eq!(retired.effects.rows_examined, target_count);
    assert_eq!(retired.effects.retained_texture_bytes, 0);
    retired.assert_geometry_local(target_count);
    assert!(!surface.renderer.has_retained_effect_sources());
    let mut cycle_allocations = 0;
    for _ in 0..RETIREMENT_CYCLES {
        for target in &targets {
            attach(running.scene_mut(), target, authoring);
        }
        let (_, attached) = surface.capture(device, queue, &mut running);
        assert_eq!(attached.effects.retained_texture_bytes, retained_bytes);
        assert_eq!(
            attached.effects.pipeline_compiles, 0,
            "retirement keeps the operator warm"
        );
        assert_eq!(attached.upload.geometry.bytes_uploaded, 0);
        cycle_allocations += attached.effects.texture_allocations;
        for target in &targets {
            detach(running.scene_mut(), target, authoring);
        }
        let (restored, retired) = surface.capture(device, queue, &mut running);
        assert_eq!(restored, ordinary);
        assert_eq!(retired.effects.retained_texture_bytes, 0);
        assert_eq!(retired.upload.geometry.bytes_uploaded, 0);
        assert!(!surface.renderer.has_retained_effect_sources());
    }
    let (_, clean_retired) = surface.capture(device, queue, &mut running);
    clean_retired.assert_clean(0);
    println!(
        "SEMANTIC_VISUAL_EFFICIENCY {}",
        json!({
            "scope": "software Vulkan offscreen observation; no physical FPS claim",
            "authoring": format!("{authoring:?}"), "static_objects": static_count,
            "active_radial_masks": target_count, "resolution": SIZE,
            "cold_renderer_setup_us": distribution(&setup_us), "cold_capture": timings(&cold),
            "clean_capture": timings(&clean), "animated_capture": timings(&animated),
            "timeline_advance_us": distribution(&advance_us),
            "cold_source_captures": cold[0].effects.source_passes,
            "cold_texture_allocations": cold[0].effects.texture_allocations,
            "cold_filter_pipeline_compiles": cold[0].effects.pipeline_compiles,
            "clean_scene_upload_bytes_max": clean.iter().map(|s| s.upload.bytes_uploaded()).max().unwrap(),
            "clean_effect_upload_bytes_max": clean.iter().map(|s| s.effects.bytes_uploaded).max().unwrap(),
            "clean_effect_rows_max": clean.iter().map(|s| s.effects.rows_examined).max().unwrap(),
            "changed_rows_per_animated_frame": animated[0].changed_rows,
            "source_recaptures_per_progress_frame": animated[0].effects.source_passes,
            "filter_bytes_per_progress_frame": local_filter_bytes,
            "geometry_bytes_per_progress_frame": animated[0].upload.geometry.bytes_uploaded,
            "warm_pipeline_compiles": animated.iter().map(|s| s.programs.pipeline_compiles + s.effects.pipeline_compiles).sum::<usize>(),
            "warm_texture_allocations": animated.iter().map(|s| s.effects.texture_allocations).sum::<usize>(),
            "paint_edit_bytes": paint_work.upload.geometry.bytes_uploaded,
            "paint_edit_source_recaptures": paint_work.effects.source_passes,
            "retained_effect_texture_bytes": retained_bytes,
            "retired_effect_texture_bytes": clean_retired.effects.retained_texture_bytes,
            "retirement_cycles": RETIREMENT_CYCLES, "reattach_texture_allocations": cycle_allocations,
            "unmeasured": ["host heap allocation totals", "effect uniform buffer allocations", "in-flight and driver memory"],
        })
    );
    CaseResult {
        initial_pixels,
        local_filter_bytes,
        local_paint_bytes: paint_work.upload.geometry.bytes_uploaded,
        retained_bytes,
    }
}

#[test]
#[ignore = "requires an actual software Vulkan adapter; absence fails qualification"]
fn radial_and_fractal_retained_work_stays_local_and_resources_retire() {
    assert_eq!(
        radial_value(),
        RadialMask::burn()
            .with_update(EffectUpdate::default().progress(0.25).seed(7), None)
            .unwrap()
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: true,
        ..Default::default()
    }))
    .expect("efficiency fixture requires a software Vulkan adapter");
    let info = adapter.get_info();
    assert_eq!(info.device_type, wgpu::DeviceType::Cpu);
    println!("SEMANTIC_VISUAL_EFFICIENCY_ADAPTER {info:?}");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let convenience = qualify_case(&device, &queue, 0, 1, Authoring::Convenience);
    let explicit = qualify_case(&device, &queue, 0, 1, Authoring::Explicit);
    assert_eq!(convenience.initial_pixels, explicit.initial_pixels);
    assert_eq!(convenience.retained_bytes, explicit.retained_bytes);
    let multiple = qualify_case(&device, &queue, 0, 8, Authoring::Convenience);
    let large = qualify_case(&device, &queue, 1024, 8, Authoring::Convenience);
    for case in [&explicit, &multiple, &large] {
        assert_eq!(case.local_filter_bytes, convenience.local_filter_bytes);
        assert_eq!(case.local_paint_bytes, convenience.local_paint_bytes);
    }
    assert_eq!(multiple.retained_bytes, large.retained_bytes);
}
