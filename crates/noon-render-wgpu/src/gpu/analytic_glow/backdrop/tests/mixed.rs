//! Independent ordinary-input references for mixed backdrop captures.
use super::*;
use noon_compile::CompiledResources;
use noon_core::{
    ComposedSource, GeometryResourceArena, ObjectContentRef, RasterImageResourceArena,
    RasterImageSampling, SemanticImageContent, TextResourceArena, VectorPath,
};

fn inputs() -> (CompiledResources, Vec<CompiledObject>, Vec<CompiledObject>) {
    let mut resources = CompiledResources::default();
    let mut images = RasterImageResourceArena::new();
    let pixels: Vec<u8> = (0..12)
        .flat_map(|y| {
            (0..16).flat_map(move |x| {
                [
                    32 + 12 * x as u8,
                    192 - 8 * y as u8,
                    128,
                    [0, 64, 128, 255][(x + 2 * y) % 4],
                ]
            })
        })
        .collect();
    let image = images.intern_rgba8(16, 12, pixels).unwrap();
    let image = resources
        .capture_image_from_arena(
            &images,
            SemanticImageContent::with_sampling(image, RasterImageSampling::Nearest),
        )
        .unwrap();
    let artifact =
        noon_typst::compile_typst_resource("Noon", noon_typst::TypstMode::Markup).unwrap();
    let mut texts = TextResourceArena::new();
    let text = texts.insert(artifact.resource).unwrap();
    let bounds = resources
        .capture_text_from_arenas(&texts, &artifact.fonts, &GeometryResourceArena::new(), text)
        .unwrap();
    let text_scale = 1.6 / bounds.width();
    let text_center = bounds.center() * text_scale;
    let path = GeometryRef::path(
        VectorPath::new()
            .move_to(Vec2::new(-0.9, -0.5))
            .cubic_to(
                Vec2::new(-0.9, 0.8),
                Vec2::new(0.9, 0.8),
                Vec2::new(0.9, -0.5),
            )
            .line_to(Vec2::new(0.0, -0.9))
            .close(),
    );
    let make = |id, content, translation, scale, fill, opacity| {
        CompiledObject::new(
            ObjectId::new(id),
            content,
            Transform2D {
                translation,
                scale,
                ..Transform2D::IDENTITY
            },
            Style {
                fill: Some(fill),
                stroke: None,
                stroke_width: 0.0,
                opacity,
                ..Style::default()
            },
        )
    };
    let mut prefix = vec![
        make(
            10,
            ObjectContentRef::Image(image),
            Vec2::new(-0.5, 0.4),
            Vec2::new(0.1, 0.1),
            Color::rgba(1.0, 1.0, 1.0, 0.8),
            0.75,
        ),
        make(
            11,
            ObjectContentRef::Text(text),
            Vec2::new(0.4, -0.4) - text_center,
            Vec2::new(text_scale, text_scale),
            Color::rgba(0.3, 0.9, 0.2, 0.7),
            0.8,
        ),
        make(
            12,
            path.clone().into(),
            Vec2::new(-0.2, 0.1),
            Vec2::new(1.2, 1.2),
            Color::rgba(0.7, 0.1, 0.3, 0.6),
            0.65,
        ),
    ];
    let mut target = vec![
        make(
            20,
            ObjectContentRef::Image(image),
            Vec2::new(-0.3, 0.1),
            Vec2::new(0.1, 0.1),
            Color::rgba(1.0, 1.0, 1.0, 0.35),
            0.65,
        ),
        make(
            21,
            ObjectContentRef::Text(text),
            Vec2::new(0.5, 0.0) - text_center,
            Vec2::new(text_scale, text_scale),
            Color::rgba(0.9, 0.7, 0.1, 0.4),
            0.7,
        ),
        make(
            22,
            path.into(),
            Vec2::new(0.3, -0.1),
            Vec2::new(0.9, 0.9),
            Color::rgba(0.1, 0.4, 0.9, 0.25),
            0.6,
        ),
    ];
    for (rank, object) in prefix.iter_mut().chain(&mut target).enumerate() {
        object.base_z_index = rank as f64;
    }
    prefix[1].text_bounds = Some(bounds);
    target[1].text_bounds = Some(bounds);
    target[2].base_style.stroke = Some(Color::rgba(0.9, 0.5, 0.1, 0.4));
    target[2].base_style.stroke_width = 0.13;
    (resources, prefix, target)
}
fn scene(objects: Vec<CompiledObject>, resources: &CompiledResources) -> SceneInstance {
    let mut compiled = CompiledScene::compile_objects(objects, &[]).unwrap();
    compiled.merge_prepared_resources(resources.clone());
    SceneInstance::new(compiled)
}
fn ordinary_at(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rows: Vec<CompiledObject>,
    resources: &CompiledResources,
    clear: wgpu::Color,
    camera: Camera2D,
) -> Vec<u8> {
    let mut fixture = Fixture::new(device, queue);
    fixture.renderer.set_camera(queue, camera);
    fixture.render(device, queue, &mut scene(rows, resources), clear);
    read(device, queue, &fixture.target)
}
fn assert_reference(name: &str, actual: &[u8], reference: &[u8], fields: &[(&str, &[u8])]) -> u8 {
    let error = actual
        .iter()
        .zip(reference)
        .map(|(&a, &b)| a.abs_diff(b))
        .max()
        .unwrap();
    retain(name, actual, reference, fields,
        r#"{"width":80,"height":60,"bound":2,"profile":"mixed image/text/path backdrop input and target under camera pan/zoom"}"#.to_owned());
    assert!(error <= 2, "{name}: maximum byte error {error}");
    error
}
#[test]
#[ignore = "requires a real raster adapter; mixed inputs/targets, camera and intrinsic image coverage"]
fn backdrop_mixed_sources_targets_and_camera_reuse_match_independent_composition() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (resources, prefix_rows, target_rows) = inputs();
    let cameras = [
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        Camera2D::new(Vec2::new(0.3, -0.2), Vec2::new(10.0, 7.5)).unwrap(),
        Camera2D::new(Vec2::new(-0.4, 0.25), Vec2::new(6.4, 4.8)).unwrap(),
    ];
    let clear = wgpu::Color {
        r: 24.0 / 255.0,
        g: 48.0 / 255.0,
        b: 64.0 / 255.0,
        a: 128.0 / 255.0,
    };
    let mut maximum = 0;
    for mode in 0..4 {
        let mut members = if mode == 3 {
            target_rows.clone()
        } else {
            vec![target_rows[mode].clone()]
        };
        let opacity = 0.6;
        if mode != 3 {
            members[0].base_style.opacity = 1.0;
        }
        let mut white = members.clone();
        for row in &mut white {
            row.base_style.fill = Some(Color::WHITE);
            if row.base_style.stroke.is_some() {
                row.base_style.stroke = Some(Color::WHITE);
            }
        }
        let mut objects = prefix_rows.clone();
        if mode == 3 {
            objects.extend(members.clone());
            let mut group = CompiledObject::new(
                ObjectId::new(90),
                ObjectContentRef::Composed(Arc::new(
                    ComposedSource::new(members.iter().map(|row| row.id), None).unwrap(),
                )),
                Transform2D::IDENTITY,
                Style {
                    fill: None,
                    stroke: None,
                    opacity,
                    ..Style::default()
                },
            );
            group.draw_enabled = false;
            group.effects = Some(panel_effect(2.0));
            objects.push(group);
        } else {
            let mut target = members[0].clone();
            target.base_style.opacity = opacity;
            target.effects = Some(panel_effect(2.0));
            objects.push(target);
        }
        let mut runtime = scene(objects, &resources);
        let mut fixture = Fixture::new(&device, &queue);
        let mut first = Vec::new();
        let mut plateau = 0;
        for cycle in 0..2 {
            for (view, &camera) in cameras.iter().enumerate() {
                let prefix = ordinary_at(
                    &device,
                    &queue,
                    prefix_rows.clone(),
                    &resources,
                    clear,
                    camera,
                );
                let paint = ordinary_at(
                    &device,
                    &queue,
                    members.clone(),
                    &resources,
                    wgpu::Color::TRANSPARENT,
                    camera,
                );
                let coverage = ordinary_at(
                    &device,
                    &queue,
                    white.clone(),
                    &resources,
                    wgpu::Color::TRANSPARENT,
                    camera,
                );
                assert!(coverage.iter().skip(3).step_by(4).any(|&alpha| alpha > 0));
                let reference = expected(&prefix, &paint, &coverage, 2.0, f64::from(opacity));
                fixture.renderer.set_camera(&queue, camera);
                let work = fixture.render(&device, &queue, &mut runtime, clear);
                let actual = read(&device, &queue, &fixture.target);
                if mode == 0 && cycle == 0 {
                    let cache = fixture.renderer.effect_captures.as_deref().unwrap();
                    let scope = &cache.scopes[&(2, 3)];
                    let tile = scope.tile;
                    let captures = [
                        (
                            "prefix",
                            cache.backdrops.scopes[&ObjectId::new(20)]
                                .prefix
                                .output()
                                .unwrap(),
                            &prefix,
                        ),
                        ("paint", &scope.source.texture, &paint),
                        (
                            "coverage",
                            &scope.silhouette.as_ref().unwrap().texture,
                            &coverage,
                        ),
                    ];
                    for (label, texture, reference) in captures {
                        let pixels = read(&device, &queue, texture);
                        let mut worst = (0, 0, 0, 0, 0);
                        for y in 0..VIEW[1] as i32 {
                            for x in 0..VIEW[0] as i32 {
                                let sx = x - tile.origin[0];
                                let sy = y - tile.origin[1];
                                if sx < 0
                                    || sy < 0
                                    || sx >= texture.width() as i32
                                    || sy >= texture.height() as i32
                                {
                                    continue;
                                }
                                let at = ((y as u32 * VIEW[0] + x as u32) * 4) as usize;
                                let src = ((sy as u32 * texture.width() + sx as u32) * 4) as usize;
                                for c in 0..4 {
                                    let err = pixels[src + c].abs_diff(reference[at + c]);
                                    if err > worst.0 {
                                        worst = (err, x, y, pixels[src + c], reference[at + c]);
                                    }
                                }
                            }
                        }
                        assert_eq!(worst.0, 0, "view {view} captured {label}: {worst:?}");
                    }
                }
                maximum = maximum.max(assert_reference(
                    &format!("mixed-target-{mode}-camera-{view}"),
                    &actual,
                    &reference,
                    &[
                        ("prefix", &prefix),
                        ("paint", &paint),
                        ("coverage", &coverage),
                    ],
                ));
                if cycle == 0 {
                    first.push(actual);
                } else {
                    assert_eq!(
                        actual, first[view],
                        "same camera must reproduce exact pixels"
                    );
                    assert_eq!(
                        work.texture_allocations, 0,
                        "all camera capacities are warm"
                    );
                    assert_eq!(work.retained_texture_bytes, plateau);
                }
                let clean = fixture.render(&device, &queue, &mut runtime, clear);
                assert_eq!(clean.texture_allocations, 0);
                assert_eq!(clean.source_passes, 0);
                assert_eq!(clean.blur_passes, 0);
                assert_eq!(clean.composite_passes, 0);
            }
            if cycle == 0 {
                plateau = fixture.renderer.effect_capture_texture_bytes();
            }
        }
    }
    eprintln!("mixed backdrop: 12 image/text/path/group-camera profiles, maximum byte error {maximum}; exact repeated views and bounded warm texture storage");
}
