//! Mixed painter-stream experiment, not automatic admission or a full Scene test.
//! Uses the existing image/glyph/geometry draw path and the parent's restore probe.
use std::collections::HashSet;

use super::{compare, Probe, FORMAT};
use crate::gpu::{
    Camera2D, GpuRenderer, PreparedRetainedGpuFrame, RetainedDrawStats, RetainedFramePreparer,
    RetainedTextGpuState,
};
use crate::text::TextDeviceMetrics;
use noon_core::{
    Color, FontResourceArena, GeometryRef, GeometryResourceArena, ObjectContentRef, ObjectId,
    RasterImageContentRef, RasterImageResourceArena, RasterImageSampling, SemanticImageContent,
    Style, TextResourceArena, Transform2D, Vec2, VectorPath,
};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};
use noon_typst::{compile_typst_resource, TypstMode};

struct Fixture {
    frame: FrameState,
    texts: TextResourceArena,
    fonts: FontResourceArena,
    geometries: GeometryResourceArena,
    images: RasterImageResourceArena,
    replacement: RasterImageContentRef,
}

impl Fixture {
    fn new(sampling: RasterImageSampling) -> Self {
        let mut images = RasterImageResourceArena::new();
        let mut image = |variant: u8| {
            let mut rgba = Vec::new();
            for y in 0..8_u8 {
                for x in 0..8_u8 {
                    // Straight RGBA includes colored zero-alpha and partial texels.
                    rgba.extend([
                        x * 31,
                        y * 29,
                        variant,
                        [0, 85, 170, 255][(x % 4) as usize],
                    ]);
                }
            }
            let handle = images.intern_rgba8(8, 8, rgba).unwrap();
            RasterImageContentRef::from_resource(
                SemanticImageContent::with_sampling(handle, sampling),
                images.get(handle).unwrap(),
            )
        };
        let original = image(41);
        let replacement = image(219);
        let artifact = compile_typst_resource(
            "#set text(font: \"DejaVu Sans Mono\")\nCACHE",
            TypstMode::Markup,
        )
        .unwrap();
        let bounds = artifact.resource.bounds;
        assert!(bounds.width().is_finite() && bounds.width() > 0.0);
        assert!(!artifact.resource.runs.is_empty());
        let scale = 1.5 / bounds.width();
        let mut texts = TextResourceArena::new();
        let text = texts.insert(artifact.resource).unwrap();
        let content = [
            ObjectContentRef::Image(original),
            ObjectContentRef::Geometry(GeometryRef::path(
                VectorPath::new()
                    .move_to(Vec2::new(-1.7, -0.7))
                    .line_to(Vec2::new(1.6, 0.8)),
            )),
            ObjectContentRef::Text(text),
            ObjectContentRef::Geometry(GeometryRef::rectangle(1.7, 0.85)),
            ObjectContentRef::Image(original),
            ObjectContentRef::Text(text),
        ];
        let mut objects: Vec<_> = content
            .into_iter()
            .enumerate()
            .map(|(index, content)| FrameObjectState {
                glow: None,
                spatial: None,
                z_index: 0.0,
                id: ObjectId::new(index as u64 + 1),
                content,
                text_bounds: None,
                transform: Transform2D::IDENTITY,
                style: Style {
                    stroke_width: 0.0,
                    ..Style::default()
                },
                appearance: 1.0,
            })
            .collect();
        objects[0].transform.scale = Vec2::new(0.48, 0.34);
        objects[0].style.opacity = 0.85;
        objects[1].style.fill = None;
        objects[1].style.stroke = Some(Color::rgba(1.0, 0.1, 0.2, 0.65));
        objects[1].style.stroke_width = 0.031;
        objects[2].text_bounds = Some(bounds);
        objects[2].transform.scale = Vec2::new(scale, scale);
        objects[2].transform.translation = Vec2::new(-0.25, 0.15) - bounds.center() * scale;
        objects[2].style.opacity = 0.8;
        objects[3].style.fill = Some(Color::rgba(0.1, 0.8, 0.9, 0.45));
        objects[4].transform.scale = Vec2::new(0.10, 0.075);
        objects[4].transform.translation = Vec2::new(0.4, -0.25);
        objects[4].style.opacity = 0.7;
        objects[5].text_bounds = Some(bounds);
        objects[5].transform.scale = Vec2::new(scale, scale);
        objects[5].transform.translation = Vec2::new(0.1, -0.32) - bounds.center() * scale;
        Self {
            frame: FrameState {
                family_animations: vec![],
                family_animation_plan_indices: vec![],
                time: 0.0,
                objects,
                presences: vec![true; 6],
                reveals: vec![1.0; 6],
                morphs: vec![0.0; 6],
                render_geometries: vec![None; 6],
                render_transforms: vec![None; 6],
            },
            texts,
            fonts: artifact.fonts,
            geometries: artifact.geometry,
            images,
            replacement,
        }
    }

    fn prepare<'a>(
        &self,
        probe: &Probe,
        preparer: &'a mut RetainedFramePreparer,
        changes: &FrameChanges,
        metrics: TextDeviceMetrics,
    ) -> PreparedRetainedGpuFrame<'a> {
        preparer
            .prepare_with_image_resources(
                &probe.device,
                &self.frame,
                changes,
                &self.texts,
                &self.fonts,
                &self.geometries,
                &self.images,
                metrics,
            )
            .unwrap()
    }
}

fn excluded(ids: &[u64]) -> HashSet<ObjectId> {
    ids.iter().copied().map(ObjectId::new).collect()
}

fn draw<'a>(
    renderer: &'a GpuRenderer,
    pass: &mut wgpu::RenderPass<'a>,
    prepared: &'a PreparedRetainedGpuFrame<'_>,
    text: &'a RetainedTextGpuState,
    excluded: &HashSet<ObjectId>,
) -> RetainedDrawStats {
    renderer
        .draw_retained_items(
            pass,
            prepared,
            text,
            4,
            &renderer.camera_bind_group,
            None,
            excluded,
            false,
            &mut HashSet::new(),
        )
        .unwrap()
}

fn renderer(probe: &Probe, size: [u32; 2], center: Vec2) -> GpuRenderer {
    let mut renderer = GpuRenderer::new(&probe.device, &probe.queue, FORMAT);
    renderer.set_viewport(&probe.device, &probe.queue, size[0], size[1]);
    renderer.set_camera(
        &probe.queue,
        Camera2D::new(center, Vec2::new(4.0, 3.0)).unwrap(),
    );
    renderer
}

#[test]
#[ignore = "requires a raster adapter; explicit mixed image/glyph/vector cache qualification"]
fn mixed_image_glyph_prefix_sample_restore() {
    let directory = std::env::var_os("NOON_CACHE_PROBE_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "target/cache-mixed-text-probe".into());
    run(&directory);
}

fn run(directory: &std::path::Path) {
    std::fs::create_dir_all(directory).unwrap();
    let marker = directory.join("PASS.txt");
    if marker.exists() {
        std::fs::remove_file(&marker).unwrap();
    }
    let setup = directory.join("setup");
    std::fs::create_dir_all(&setup).unwrap();
    let probe = Probe::new(&setup);
    let provenance = format!(
        "#2094 mixed image/glyph/vector probe\nadapter={:?}\nformat=Rgba8Unorm samples=4 transfer=Direct\nprofiles=Nearest/96x64,Linear/127x81,Bicubic/127x81\nExplicit test prefix; not automatic caching, full triangle/wood, public Scene or performance\n",
        probe.device.adapter_info(),
    );
    std::fs::write(directory.join("profile.txt"), provenance).unwrap();
    let prefix_ids = excluded(&[1, 2, 3]);
    let suffix_ids = excluded(&[4, 5, 6]);
    for (profile, sampling) in [
        RasterImageSampling::Nearest,
        RasterImageSampling::Linear,
        RasterImageSampling::Bicubic,
    ]
    .into_iter()
    .enumerate()
    {
        let size = if profile == 0 { [96, 64] } else { [127, 81] };
        let center = Vec2::new(0.013, -0.027);
        let clear = if profile == 0 {
            wgpu::Color::TRANSPARENT
        } else {
            wgpu::Color {
                r: 0.125,
                g: 0.25,
                b: 0.375,
                a: 0.5,
            }
        };
        let metrics = TextDeviceMetrics::uniform(size[0] as f32 / 4.0).unwrap();
        let mut fixture = Fixture::new(sampling);
        let mut gpu = renderer(&probe, size, center);
        let mut text = gpu.create_retained_text_state(&probe.device, &probe.queue);
        let mut preparer = RetainedFramePreparer::new();
        preparer.set_painter_order(&[0, 1, 2, 3, 4, 5]);
        let prefix = probe.target(size, 4);
        let unsplit = probe.target(size, 4);
        let work = probe.target(size, 4);
        let ordinary = probe.target(size, 1);
        let reference = probe.target(size, 1);
        let candidate = probe.target(size, 1);
        let restore = probe.binding(&probe.restore, &prefix);
        // Normal encode performs all pending glyph atlas uploads before the
        // experiment reads those textures. A fresh renderer does the same.
        for step in 0..6 {
            let changes = match step {
                0 => FrameChanges::all(),
                1 => {
                    fixture.frame.objects[3].transform.translation = Vec2::new(0.37, -0.2);
                    FrameChanges::objects(vec![3])
                }
                2 | 3 => {
                    fixture.frame.objects[3].transform.translation = Vec2::ZERO;
                    FrameChanges::objects(vec![3])
                }
                4 => {
                    fixture.frame.objects[0].content = ObjectContentRef::Image(fixture.replacement);
                    FrameChanges::objects(vec![0])
                }
                _ => {
                    fixture.frame.objects[2].style.opacity = 0.2;
                    FrameChanges::objects(vec![2])
                }
            };
            let prepared = fixture.prepare(&probe, &mut preparer, &changes, metrics);
            gpu.upload_retained(&probe.device, &probe.queue, &prepared, &mut text);
            let mut encoder = probe.device.create_command_encoder(&Default::default());
            let full = gpu
                .encode_retained(
                    &mut encoder,
                    &ordinary.create_view(&Default::default()),
                    &prepared,
                    &text,
                    clear,
                    None,
                )
                .unwrap();
            probe.queue.submit([encoder.finish()]);
            assert_eq!(full.images, 2, "both image occurrences must draw");
            assert!(full.text.instances_drawn > 0 && full.geometry.instances_drawn > 0);
            let pixels = probe.readback(&ordinary);
            if step == 0 {
                render_pass!(
                    &probe,
                    &prefix,
                    None,
                    wgpu::LoadOp::Clear(clear),
                    |pass| {
                        let stats = draw(&gpu, pass, &prepared, &text, &suffix_ids);
                        assert_eq!(stats.images, 1);
                        assert!(stats.text.instances_drawn > 0);
                    }
                );
            }
            let render_candidate = || {
                render_pass!(
                    &probe,
                    &work,
                    Some(&candidate),
                    wgpu::LoadOp::Clear(clear),
                    |pass| {
                        pass.set_pipeline(&probe.restore);
                        pass.set_bind_group(0, &restore, &[]);
                        pass.draw(0..3, 0..1);
                        let tail = draw(&gpu, pass, &prepared, &text, &prefix_ids);
                        assert_eq!(tail.images, 1);
                        assert!(tail.text.instances_drawn > 0);
                        assert!(tail.instances_drawn() < full.instances_drawn());
                    }
                );
            };
            render_candidate();
            if step >= 4 {
                let name = if step == 4 { "stale-image" } else { "stale-text" };
                compare(
                    directory,
                    &format!("p{profile}-{name}"),
                    &pixels,
                    &probe.readback(&candidate),
                    false,
                );
                // Deliberate explicit rebuild; not automatic invalidation policy.
                render_pass!(
                    &probe,
                    &prefix,
                    None,
                    wgpu::LoadOp::Clear(clear),
                    |pass| {
                        draw(&gpu, pass, &prepared, &text, &suffix_ids);
                    }
                );
                render_candidate();
            }
            render_pass!(
                &probe,
                &unsplit,
                Some(&reference),
                wgpu::LoadOp::Clear(clear),
                |pass| {
                    draw(&gpu, pass, &prepared, &text, &HashSet::new());
                }
            );
            let mut fresh = renderer(&probe, size, center);
            let mut fresh_text = fresh.create_retained_text_state(&probe.device, &probe.queue);
            let mut fresh_preparer = RetainedFramePreparer::new();
            fresh_preparer.set_painter_order(&[0, 1, 2, 3, 4, 5]);
            let fresh_frame =
                fixture.prepare(&probe, &mut fresh_preparer, &FrameChanges::all(), metrics);
            fresh.upload_retained(&probe.device, &probe.queue, &fresh_frame, &mut fresh_text);
            let fresh_target = probe.target(size, 1);
            let mut encoder = probe.device.create_command_encoder(&Default::default());
            fresh
                .encode_retained(
                    &mut encoder,
                    &fresh_target.create_view(&Default::default()),
                    &fresh_frame,
                    &fresh_text,
                    clear,
                    None,
                )
                .unwrap();
            probe.queue.submit([encoder.finish()]);
            for (kind, actual) in [
                ("ordinary", probe.readback(&reference)),
                ("resolved", probe.readback(&candidate)),
                ("fresh", probe.readback(&fresh_target)),
            ] {
                compare(
                    directory,
                    &format!("p{profile}-s{step}-{kind}"),
                    &pixels,
                    &actual,
                    true,
                );
            }
            compare(
                directory,
                &format!("p{profile}-s{step}-samples"),
                &probe.samples(&unsplit),
                &probe.samples(&work),
                true,
            );
            if step == 3 {
                render_pass!(
                    &probe,
                    &work,
                    Some(&candidate),
                    wgpu::LoadOp::Clear(clear),
                    |pass| {
                        pass.set_pipeline(&probe.restore);
                        pass.set_bind_group(0, &restore, &[]);
                        pass.draw(0..3, 0..1);
                        // Forbidden gather: late static image/text below the highlight.
                        draw(&gpu, pass, &prepared, &text, &excluded(&[1, 2, 3, 4]));
                        draw(&gpu, pass, &prepared, &text, &excluded(&[1, 2, 3, 5, 6]));
                    }
                );
                compare(
                    directory,
                    &format!("p{profile}-reordered"),
                    &pixels,
                    &probe.readback(&candidate),
                    false,
                );
                for (name, ids) in [("no-image", excluded(&[1])), ("no-text", excluded(&[3]))] {
                    render_pass!(
                        &probe,
                        &work,
                        Some(&candidate),
                        wgpu::LoadOp::Clear(clear),
                        |pass| {
                            draw(&gpu, pass, &prepared, &text, &ids);
                        }
                    );
                    compare(
                        directory,
                        &format!("p{profile}-{name}"),
                        &pixels,
                        &probe.readback(&candidate),
                        false,
                    );
                }
                render_candidate();
                render_pass!(
                    &probe,
                    &work,
                    Some(&candidate),
                    wgpu::LoadOp::Load,
                    |pass| {
                        draw(&gpu, pass, &prepared, &text, &prefix_ids);
                    }
                );
                compare(
                    directory,
                    &format!("p{profile}-accumulated"),
                    &pixels,
                    &probe.readback(&candidate),
                    false,
                );
            }
        }
    }
    std::fs::write(
        directory.join("PASS.txt"),
        "CACHE_MIXED_PROBE_PASS: real mixed draws; explicit prefix\n",
    )
    .unwrap();
    eprintln!("CACHE_MIXED_PROBE_PASS");
}
