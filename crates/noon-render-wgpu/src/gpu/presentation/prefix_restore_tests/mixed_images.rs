//! #2094: test-only mixed image/vector prefix restoration. No automatic cache.
//! Uses the ordinary image residency and mixed painter implementation, not an
//! alternate texture compositor. Empty text resources deliberately exclude glyphs.
use std::collections::HashSet;

use super::*;
use crate::gpu::{
    PreparedRetainedGpuFrame, RetainedDrawStats, RetainedFramePreparer, RetainedTextGpuState,
};
use crate::text::TextDeviceMetrics;
use noon_core::{
    FontResourceArena, GeometryResourceArena, RasterImageContentRef, RasterImageResourceArena,
    RasterImageResourceHandle, RasterImageSampling, SemanticImageContent, TextResourceArena,
};

const BACKDROP: u64 = 100;
const MIDDLE: u64 = 101;
const TOP: u64 = 102;
const PREFIX_IDS: [u64; 3] = [BACKDROP, 1, 2];
const SUFFIX_IDS: [u64; 5] = [3, MIDDLE, 4, 5, TOP];

fn ids(values: &[u64]) -> HashSet<ObjectId> {
    values.iter().copied().map(ObjectId::new).collect()
}

struct Resources {
    texts: TextResourceArena,
    fonts: FontResourceArena,
    geometries: GeometryResourceArena,
    images: RasterImageResourceArena,
    original: RasterImageResourceHandle,
    replacement: RasterImageResourceHandle,
}

impl Resources {
    fn new() -> Self {
        let mut images = RasterImageResourceArena::new();
        // Straight RGBA includes hidden colored transparent texels and several
        // nontrivial alpha values. This is a sampling fixture, not the wood asset.
        let pixels = [
            [255, 0, 127, 0],
            [180, 90, 35, 255],
            [20, 190, 60, 128],
            [40, 100, 240, 192],
            [230, 170, 80, 255],
            [150, 20, 200, 64],
        ]
        .concat();
        let changed = [
            [0, 255, 80, 255],
            [30, 90, 220, 96],
            [220, 40, 110, 255],
            [90, 200, 35, 128],
            [10, 30, 80, 255],
            [250, 220, 10, 32],
        ]
        .concat();
        let original = images.intern_rgba8(3, 2, pixels).unwrap();
        let replacement = images.intern_rgba8(3, 2, changed).unwrap();
        Self {
            texts: TextResourceArena::new(),
            fonts: FontResourceArena::new(),
            geometries: GeometryResourceArena::new(),
            images,
            original,
            replacement,
        }
    }

    fn content(
        &self,
        handle: RasterImageResourceHandle,
        sampling: RasterImageSampling,
    ) -> ObjectContentRef {
        ObjectContentRef::Image(RasterImageContentRef::from_resource(
            SemanticImageContent::with_sampling(handle, sampling),
            self.images.get(handle).unwrap(),
        ))
    }

    fn prepare<'a>(
        &self,
        probe: &Probe,
        preparer: &'a mut RetainedFramePreparer,
        frame: &FrameState,
        changes: &FrameChanges,
        size: [u32; 2],
    ) -> PreparedRetainedGpuFrame<'a> {
        preparer
            .prepare_with_image_resources(
                &probe.device,
                frame,
                changes,
                &self.texts,
                &self.fonts,
                &self.geometries,
                &self.images,
                TextDeviceMetrics::uniform(size[1] as f32 / 3.0).unwrap(),
            )
            .unwrap()
    }
}

fn image(
    resources: &Resources,
    id: u64,
    sampling: RasterImageSampling,
    scale: Vec2,
    center: Vec2,
) -> FrameObjectState {
    FrameObjectState {
        glow: None,
        spatial: None,
        id: ObjectId::new(id),
        z_index: 0.0,
        content: resources.content(resources.original, sampling),
        text_bounds: None,
        transform: Transform2D {
            translation: center,
            rotation: 0.13,
            scale,
        },
        style: Style {
            fill: Some(Color::WHITE),
            stroke: None,
            stroke_width: 0.0,
            opacity: 0.75,
            ..Style::default()
        },
        appearance: 1.0,
    }
}

fn mixed_frame(resources: &Resources, sampling: RasterImageSampling) -> FrameState {
    let mut frame = super::frame();
    frame.objects.insert(
        0,
        image(
            resources,
            BACKDROP,
            sampling,
            Vec2::new(1.15, 1.2),
            Vec2::new(0.017, -0.023),
        ),
    );
    frame.objects.insert(
        4,
        image(
            resources,
            MIDDLE,
            sampling,
            Vec2::new(0.45, 0.4),
            Vec2::new(0.14, 0.09),
        ),
    );
    frame.objects.push(image(
        resources,
        TOP,
        sampling,
        Vec2::new(0.3, 0.3),
        Vec2::new(-0.2, -0.05),
    ));
    let count = frame.objects.len();
    frame.presences = vec![true; count];
    frame.reveals = vec![1.0; count];
    frame.morphs = vec![0.0; count];
    frame.render_geometries = vec![None; count];
    frame.render_transforms = vec![None; count];
    frame
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
            PATH_SAMPLE_COUNT,
            &renderer.camera_bind_group,
            None,
            excluded,
            false,
            &mut HashSet::new(),
        )
        .unwrap()
}

fn ordinary(
    probe: &Probe,
    renderer: &GpuRenderer,
    prepared: &PreparedRetainedGpuFrame<'_>,
    text: &RetainedTextGpuState,
    output: &wgpu::Texture,
    clear: wgpu::Color,
) -> Vec<u8> {
    let mut encoder = probe.device.create_command_encoder(&Default::default());
    let stats = renderer
        .encode_retained(
            &mut encoder,
            &output.create_view(&Default::default()),
            prepared,
            text,
            clear,
            None,
        )
        .unwrap();
    assert_eq!(stats.images, 3);
    assert!(stats.geometry.instances_drawn >= 5);
    assert_eq!(stats.text.draw_calls, 0);
    probe.queue.submit([encoder.finish()]);
    probe.readback(output)
}

fn fresh(
    probe: &Probe,
    resources: &Resources,
    frame: &FrameState,
    size: [u32; 2],
    center: Vec2,
    clear: wgpu::Color,
) -> Vec<u8> {
    let mut renderer = GpuRenderer::new(&probe.device, &probe.queue, FORMAT);
    renderer.set_viewport(&probe.device, &probe.queue, size[0], size[1]);
    renderer.set_camera(
        &probe.queue,
        Camera2D::new(center, Vec2::new(4.0, 3.0)).unwrap(),
    );
    let mut text = renderer.create_retained_text_state(&probe.device, &probe.queue);
    let mut preparer = RetainedFramePreparer::new();
    let prepared = resources.prepare(probe, &mut preparer, frame, &FrameChanges::all(), size);
    renderer.upload_retained(&probe.device, &probe.queue, &prepared, &mut text);
    ordinary(
        probe,
        &renderer,
        &prepared,
        &text,
        &probe.target(size, 1),
        clear,
    )
}

#[test]
#[ignore = "explicit native image-prefix raster qualification; no passing adapter skip"]
fn mixed_image_prefix_sample_restore() {
    let directory = std::env::var_os("NOON_CACHE_PROBE_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .expect("fresh artifact directory required");
    fs::create_dir_all(&directory).unwrap();
    let marker = directory.join("PASS.txt");
    if marker.exists() {
        fs::remove_file(&marker).unwrap();
    }
    let probe = Probe::new(&directory);
    let profile = fs::read_to_string(directory.join("profile.txt")).unwrap();
    let adapter = profile
        .lines()
        .find(|line| line.starts_with("adapter="))
        .unwrap();
    fs::write(
        directory.join("profile.txt"),
        format!(
            "#2094 mixed image/vector prefix probe\n{adapter}\nformat=Rgba8Unorm samples=4 transfer=Direct\nprofiles=96x64,127x81 per sampling; sampling=Nearest,Linear,Bicubic\nNOT glyph/text, wood-scene, public Scene, automatic cache, lifetime or performance qualification\n"
        ),
    )
    .unwrap();
    let resources = Resources::new();
    let prefix = ids(&PREFIX_IDS);
    let suffix = ids(&SUFFIX_IDS);
    let all: HashSet<_> = prefix.union(&suffix).copied().collect();
    let empty = HashSet::new();
    for (mode, sampling) in [
        RasterImageSampling::Nearest,
        RasterImageSampling::Linear,
        RasterImageSampling::Bicubic,
    ]
    .into_iter()
    .enumerate()
    {
        for view in 0..2 {
            let profile = mode * 2 + view;
            let size = if view == 0 { [96, 64] } else { [127, 81] };
            let center = if view == 0 {
                Vec2::ZERO
            } else {
                Vec2::new(0.013, -0.027)
            };
            let clear = if view == 0 {
                wgpu::Color::TRANSPARENT
            } else {
                wgpu::Color {
                    r: 0.125,
                    g: 0.25,
                    b: 0.375,
                    a: 0.5,
                }
            };
            let mut frame = mixed_frame(&resources, sampling);
            let mut renderer = GpuRenderer::new(&probe.device, &probe.queue, FORMAT);
            renderer.set_viewport(&probe.device, &probe.queue, size[0], size[1]);
            renderer.set_camera(
                &probe.queue,
                Camera2D::new(center, Vec2::new(4.0, 3.0)).unwrap(),
            );
            let mut text = renderer.create_retained_text_state(&probe.device, &probe.queue);
            let mut preparer = RetainedFramePreparer::new();
            let stored = probe.target(size, 4);
            let work = probe.target(size, 4);
            let reference_samples = probe.target(size, 4);
            let output = probe.target(size, 1);
            let candidate = probe.target(size, 1);
            let binding = probe.binding(&probe.restore, &stored);
            let transparent = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
            let mut prefix_samples = Vec::new();
            let mut last_reference = Vec::new();
            for step in 0..4 {
                let changed_state = step == 1;
                frame.objects[3].transform.translation.x = if changed_state { 0.23 } else { 0.0 };
                frame.objects[4].style.opacity = if changed_state { 0.3 } else { 0.75 };
                frame.objects[4].transform.translation.y = if changed_state { -0.19 } else { 0.09 };
                frame.reveals[6] = if changed_state { 0.4 } else { 1.0 };
                let changes = if step == 0 {
                    FrameChanges::all()
                } else {
                    FrameChanges::objects(vec![3, 4, 6])
                };
                let prepared = resources.prepare(&probe, &mut preparer, &frame, &changes, size);
                assert!(!prepared.geometry.path_batches.is_empty());
                let upload =
                    renderer.upload_retained(&probe.device, &probe.queue, &prepared, &mut text);
                if step > 0 {
                    assert_eq!(upload.images.pixel_bytes_uploaded, 0);
                }
                let ordinary_pixels = ordinary(&probe, &renderer, &prepared, &text, &output, clear);
                if step == 0 {
                    render_pass!(&probe, &stored, None, wgpu::LoadOp::Clear(clear), |pass| {
                        let stats = draw(&renderer, pass, &prepared, &text, &suffix);
                        assert_eq!(stats.images, 1);
                    });
                    prefix_samples = probe.samples(&stored);
                }
                render_pass!(
                    &probe,
                    &reference_samples,
                    Some(&candidate),
                    wgpu::LoadOp::Clear(clear),
                    |pass| {
                        assert_eq!(draw(&renderer, pass, &prepared, &text, &empty).images, 3);
                    }
                );
                compare(
                    &directory,
                    &format!("i{profile}-s{step}-ordinary"),
                    &ordinary_pixels,
                    &probe.readback(&candidate),
                    true,
                );
                render_pass!(&probe, &work, Some(&candidate), transparent, |pass| {
                    pass.set_pipeline(&probe.restore);
                    pass.set_bind_group(0, &binding, &[]);
                    pass.draw(0..3, 0..1);
                    assert_eq!(draw(&renderer, pass, &prepared, &text, &prefix).images, 2);
                });
                compare(
                    &directory,
                    &format!("i{profile}-s{step}-resolved"),
                    &ordinary_pixels,
                    &probe.readback(&candidate),
                    true,
                );
                compare(
                    &directory,
                    &format!("i{profile}-s{step}-fresh"),
                    &ordinary_pixels,
                    &fresh(&probe, &resources, &frame, size, center, clear),
                    true,
                );
                compare(
                    &directory,
                    &format!("i{profile}-s{step}-samples"),
                    &probe.samples(&reference_samples),
                    &probe.samples(&work),
                    true,
                );
                last_reference = ordinary_pixels;
            }
            compare(
                &directory,
                &format!("i{profile}-preserved-samples"),
                &prefix_samples,
                &probe.samples(&stored),
                true,
            );
            {
                let prepared = resources.prepare(
                    &probe,
                    &mut preparer,
                    &frame,
                    &FrameChanges::default(),
                    size,
                );
                render_pass!(
                    &probe,
                    &work,
                    Some(&candidate),
                    wgpu::LoadOp::Load,
                    |pass| {
                        draw(&renderer, pass, &prepared, &text, &prefix);
                    }
                );
                compare(
                    &directory,
                    &format!("i{profile}-accumulated"),
                    &last_reference,
                    &probe.readback(&candidate),
                    false,
                );
                let mut without_top = prefix.clone();
                without_top.insert(ObjectId::new(TOP));
                let mut only_top = all.clone();
                only_top.remove(&ObjectId::new(TOP));
                render_pass!(&probe, &work, Some(&candidate), transparent, |pass| {
                    pass.set_pipeline(&probe.restore);
                    pass.set_bind_group(0, &binding, &[]);
                    pass.draw(0..3, 0..1);
                    draw(&renderer, pass, &prepared, &text, &only_top);
                    draw(&renderer, pass, &prepared, &text, &without_top);
                });
                compare(
                    &directory,
                    &format!("i{profile}-reordered"),
                    &last_reference,
                    &probe.readback(&candidate),
                    false,
                );
                render_pass!(
                    &probe,
                    &work,
                    Some(&candidate),
                    wgpu::LoadOp::Clear(clear),
                    |pass| {
                        draw(&renderer, pass, &prepared, &text, &ids(&[BACKDROP]));
                    }
                );
                compare(
                    &directory,
                    &format!("i{profile}-missing-image"),
                    &last_reference,
                    &probe.readback(&candidate),
                    false,
                );
            }
            frame.objects[0].content = resources.content(resources.replacement, sampling);
            let prepared = resources.prepare(
                &probe,
                &mut preparer,
                &frame,
                &FrameChanges::objects(vec![0]),
                size,
            );
            let upload =
                renderer.upload_retained(&probe.device, &probe.queue, &prepared, &mut text);
            assert!(upload.images.pixel_bytes_uploaded > 0);
            let changed = ordinary(&probe, &renderer, &prepared, &text, &output, clear);
            render_pass!(&probe, &work, Some(&candidate), transparent, |pass| {
                pass.set_pipeline(&probe.restore);
                pass.set_bind_group(0, &binding, &[]);
                pass.draw(0..3, 0..1);
                draw(&renderer, pass, &prepared, &text, &prefix);
            });
            compare(
                &directory,
                &format!("i{profile}-stale-resource"),
                &changed,
                &probe.readback(&candidate),
                false,
            );
            render_pass!(&probe, &stored, None, wgpu::LoadOp::Clear(clear), |pass| {
                draw(&renderer, pass, &prepared, &text, &suffix);
            });
            render_pass!(
                &probe,
                &reference_samples,
                Some(&output),
                wgpu::LoadOp::Clear(clear),
                |pass| {
                    draw(&renderer, pass, &prepared, &text, &empty);
                }
            );
            render_pass!(&probe, &work, Some(&candidate), transparent, |pass| {
                pass.set_pipeline(&probe.restore);
                pass.set_bind_group(0, &binding, &[]);
                pass.draw(0..3, 0..1);
                draw(&renderer, pass, &prepared, &text, &prefix);
            });
            compare(
                &directory,
                &format!("i{profile}-rebuilt-resolved"),
                &changed,
                &probe.readback(&candidate),
                true,
            );
            compare(
                &directory,
                &format!("i{profile}-rebuilt-samples"),
                &probe.samples(&reference_samples),
                &probe.samples(&work),
                true,
            );
        }
    }
    fs::write(
        marker,
        "CACHE_MIXED_IMAGE_PROBE_PASS: image/vector; exact RGBA and samples; 6 profiles; 4 defects each\n",
    )
    .unwrap();
    eprintln!("CACHE_MIXED_IMAGE_PROBE_PASS");
}
