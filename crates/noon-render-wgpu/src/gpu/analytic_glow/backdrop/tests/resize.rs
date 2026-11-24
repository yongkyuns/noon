//! Resizing and viewport boundaries must retain the full earlier input support.
use super::*;
use noon_compile::CompiledResources;
use noon_core::{ComposedSource, EffectRadius, ObjectContentRef, Rect};
const SIZES: [[u32; 2]; 4] = [[80, 60], [160, 120], [120, 80], [80, 60]];
const MARGIN: u32 = 32;
struct Field {
    pixels: Vec<u8>,
    size: [u32; 2],
    origin: [i32; 2],
}
impl Field {
    fn pixel(&self, x: i32, y: i32, channel: usize) -> f64 {
        let x = x - self.origin[0];
        let y = y - self.origin[1];
        assert!(
            x >= 0 && y >= 0 && x < self.size[0] as i32 && y < self.size[1] as i32,
            "the independent source field must contain every needed sample"
        );
        f64::from(self.pixels[(y as usize * self.size[0] as usize + x as usize) * 4 + channel])
    }
}
fn reference(prefix: &Field, paint: &[u8], mask: &[u8], size: [u32; 2], sigma: f64) -> Vec<u8> {
    let support = (3.0 * sigma).ceil() as i32;
    let norm = (-support..=support)
        .map(|x| (-0.5 * (f64::from(x) / sigma).powi(2)).exp())
        .sum::<f64>()
        .powi(2);
    let kernel = (-support..=support)
        .flat_map(|y| {
            (-support..=support).map(move |x| {
                (
                    x,
                    y,
                    (-0.5 * f64::from(x * x + y * y) / sigma.powi(2)).exp() / norm,
                )
            })
        })
        .collect::<Vec<_>>();
    let mut out = vec![0; (size[0] * size[1] * 4) as usize];
    for y in 0..size[1] as i32 {
        for x in 0..size[0] as i32 {
            let at = ((y as u32 * size[0] + x as u32) * 4) as usize;
            let m = f64::from(mask[at + 3]) / 255.0;
            let pa = f64::from(paint[at + 3]) / 255.0;
            for c in 0..4 {
                let d = prefix.pixel(x, y, c);
                if m == 0.0 {
                    out[at + c] = d as u8;
                    continue;
                }
                let f = kernel
                    .iter()
                    .map(|&(dx, dy, w)| prefix.pixel(x + dx, y + dy, c) * w)
                    .sum::<f64>();
                let p = f64::from(paint[at + c]) * if pa > m { m / pa } else { 1.0 };
                out[at + c] = (d * (1.0 - 0.6 * m) + (p + f * (m - pa.min(m))) * 0.6)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}
fn effect(scene_units: bool) -> Arc<CompiledEffectStack> {
    Arc::new(
        CompiledEffectStack::new([CompiledEffect {
            attachment: SemanticNodeId::new(83, 2),
            definition: BackdropBlur::new(EffectUpdate::default().radius(if scene_units {
                EffectRadius::Scene(0.2)
            } else {
                EffectRadius::Pixels(2.0)
            }))
            .unwrap()
            .into(),
        }])
        .unwrap(),
    )
}
fn objects(
    case: usize,
    scene_units: bool,
) -> (
    CompiledResources,
    Vec<CompiledObject>,
    Vec<CompiledObject>,
    CompiledObject,
) {
    let (resources, prefix, members) = if case == 0 {
        mixed::inputs()
    } else {
        let (green, green_size, red, red_size, center) = match case {
            1 => (
                Vec2::new(-4.3, 0.0),
                [0.3, 1.4],
                Vec2::new(-3.8, 0.1),
                [1.4, 1.6],
                Vec2::new(-3.9, 0.0),
            ),
            2 => (
                Vec2::new(4.25, 3.25),
                [0.3, 0.3],
                Vec2::new(3.8, 2.8),
                [1.4, 1.6],
                Vec2::new(3.9, 2.9),
            ),
            3 => (
                Vec2::new(0.0, -3.3),
                [1.4, 0.3],
                Vec2::new(0.1, -2.8),
                [1.6, 1.4],
                Vec2::new(0.0, -2.9),
            ),
            _ => unreachable!(),
        };
        let mut a = row(10, green.x, green_size, Color::rgba(0.0, 1.0, 0.0, 1.0));
        a.base_transform.translation = green;
        let mut b = row(11, red.x, red_size, Color::rgba(0.9, 0.1, 0.2, 0.65));
        b.base_transform.translation = red;
        b.base_z_index = 1.0;
        let mut target = row(20, center.x, [3.0, 2.0], Color::rgba(0.1, 0.5, 0.9, 0.12));
        target.base_transform.translation = center;
        target.base_z_index = 2.0;
        (CompiledResources::default(), vec![a, b], vec![target])
    };
    let mut group = CompiledObject::new(
        ObjectId::new(90),
        ObjectContentRef::Composed(Arc::new(
            ComposedSource::new(members.iter().map(|row| row.id), None).unwrap(),
        )),
        Transform2D::IDENTITY,
        Style {
            fill: None,
            stroke: None,
            opacity: 0.6,
            ..Style::default()
        },
    );
    group.draw_enabled = false;
    group.effects = Some(effect(scene_units));
    (resources, prefix, members, group)
}
fn ordinary_sized(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    objects: Vec<CompiledObject>,
    resources: &CompiledResources,
    size: [u32; 2],
    camera: Camera2D,
    clear: wgpu::Color,
) -> Vec<u8> {
    let mut fixture = Fixture::new(device, queue);
    fixture.resize(device, queue, size);
    fixture.renderer.set_camera(queue, camera);
    fixture.render(device, queue, &mut mixed::scene(objects, resources), clear);
    read(device, queue, &fixture.target)
}
#[test]
#[ignore = "requires a real raster adapter; repeated physical resize, original offscreen prefix and cold/warm agreement"]
fn backdrop_viewport_resize_and_clipped_targets_preserve_full_prefix_and_warm_capacity() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let clear = wgpu::Color {
        r: 24.0 / 255.0,
        g: 48.0 / 255.0,
        b: 64.0 / 255.0,
        a: 128.0 / 255.0,
    };
    let mut maximum = 0;
    let mut rejected = 0;
    for scene_units in [false, true] {
        for case in 0..4 {
            let (resources, prefix_rows, members, group) = objects(case, scene_units);
            let mut objects = prefix_rows.clone();
            objects.extend(members.clone());
            objects.push(group);
            let mut scene = mixed::scene(objects, &resources);
            let mut white = members.clone();
            for row in &mut white {
                row.base_style.fill = Some(Color::WHITE);
                if row.base_style.stroke.is_some() {
                    row.base_style.stroke = Some(Color::WHITE);
                }
            }
            let mut fixture = Fixture::new(&device, &queue);
            let mut first = Vec::new();
            let mut plateau = 0;
            let mut references = Vec::new();
            for cycle in 0..2 {
                for (view, size) in SIZES.into_iter().enumerate() {
                    // Normal host resize preserves authored world height and
                    // follows the viewport aspect; it does not stretch a fixed
                    // logical camera into an unsupported anisotropic view.
                    let camera = Camera2D::new(
                        Vec2::ZERO,
                        Vec2::new(6.0 * size[0] as f32 / size[1] as f32, 6.0),
                    )
                    .unwrap();
                    if cycle == 0 {
                        let prefix_size = if case == 0 {
                            size
                        } else {
                            size.map(|n| n + 2 * MARGIN)
                        };
                        let prefix_camera = Camera2D::new(
                            Vec2::ZERO,
                            Vec2::new(
                                camera.world_size.x * prefix_size[0] as f32 / size[0] as f32,
                                6.0 * prefix_size[1] as f32 / size[1] as f32,
                            ),
                        )
                        .unwrap();
                        let prefix = Field {
                            pixels: ordinary_sized(
                                &device,
                                &queue,
                                prefix_rows.clone(),
                                &resources,
                                prefix_size,
                                prefix_camera,
                                clear,
                            ),
                            size: prefix_size,
                            origin: if case == 0 {
                                [0, 0]
                            } else {
                                [-(MARGIN as i32); 2]
                            },
                        };
                        if let Ok(root) = std::env::var("NOON_BACKDROP_COORDINATOR_ARTIFACT_DIR") {
                            let path = std::path::Path::new(&root).join(format!(
                                "resize-units-{}-case-{case}-view-{view}",
                                usize::from(scene_units)
                            ));
                            std::fs::create_dir_all(&path).unwrap();
                            std::fs::write(path.join("prefix.rgba"), &prefix.pixels).unwrap();
                            std::fs::write(
                                path.join("prefix-field.json"),
                                format!(
                                    r#"{{"width":{},"height":{},"origin":[{},{}]}}"#,
                                    prefix.size[0],
                                    prefix.size[1],
                                    prefix.origin[0],
                                    prefix.origin[1]
                                ),
                            )
                            .unwrap();
                        }
                        let paint = ordinary_sized(
                            &device,
                            &queue,
                            members.clone(),
                            &resources,
                            size,
                            camera,
                            wgpu::Color::TRANSPARENT,
                        );
                        let coverage = ordinary_sized(
                            &device,
                            &queue,
                            white.clone(),
                            &resources,
                            size,
                            camera,
                            wgpu::Color::TRANSPARENT,
                        );
                        let sigma = if scene_units {
                            0.2 * f64::from(size[1]) / 6.0
                        } else {
                            2.0
                        };
                        references.push((
                            reference(&prefix, &paint, &coverage, size, sigma),
                            paint,
                            coverage,
                        ));
                    }
                    fixture.resize(&device, &queue, size);
                    fixture.renderer.set_camera(&queue, camera);
                    let work = fixture.render(&device, &queue, &mut scene, clear);
                    let actual = read(&device, &queue, &fixture.target);
                    let (reference, paint, coverage) = &references[view];
                    if case == 0 && view == 1 && cycle == 0 {
                        let prefix = ordinary_sized(
                            &device,
                            &queue,
                            prefix_rows.clone(),
                            &resources,
                            size,
                            camera,
                            clear,
                        );
                        let cache = fixture.renderer.effect_captures.as_deref().unwrap();
                        let scope = &cache.scopes[&(2, 6)];
                        for (label, texture, expected) in [
                            (
                                "prefix",
                                cache.backdrops.scopes[&ObjectId::new(90)]
                                    .prefix
                                    .output()
                                    .unwrap(),
                                &prefix,
                            ),
                            ("paint", &scope.source.texture, paint),
                            (
                                "coverage",
                                &scope.silhouette.as_ref().unwrap().texture,
                                coverage,
                            ),
                        ] {
                            let pixels = read(&device, &queue, texture);
                            let mut worst = (0, 0, 0, 0, 0);
                            for y in 0..size[1] as i32 {
                                for x in 0..size[0] as i32 {
                                    let sx = x - scope.tile.origin[0];
                                    let sy = y - scope.tile.origin[1];
                                    if sx < 0
                                        || sy < 0
                                        || sx >= texture.width() as i32
                                        || sy >= texture.height() as i32
                                    {
                                        continue;
                                    }
                                    let at = ((y as u32 * size[0] + x as u32) * 4) as usize;
                                    let src =
                                        ((sy as u32 * texture.width() + sx as u32) * 4) as usize;
                                    for c in 0..4 {
                                        let e = pixels[src + c].abs_diff(expected[at + c]);
                                        if e > worst.0 {
                                            worst = (e, x, y, pixels[src + c], expected[at + c]);
                                        }
                                    }
                                }
                            }
                            assert_eq!(
                                worst.0, 0,
                                "resize units={scene_units} captured {label}: {worst:?}"
                            );
                        }
                    }
                    let error = actual
                        .iter()
                        .zip(reference)
                        .map(|(&a, &b)| a.abs_diff(b))
                        .max()
                        .unwrap();
                    maximum = maximum.max(error);
                    retain(
                        &format!(
                            "resize-units-{}-case-{case}-view-{view}",
                            usize::from(scene_units)
                        ),
                        &actual,
                        reference,
                        &[("paint", paint), ("coverage", coverage)],
                        format!(
                            r#"{{"width":{},"height":{},"bound":2,"sceneUnits":{scene_units},"case":{case},"prefixMargin":{},"profile":"physical resize and offscreen painter prefix with original target coverage"}}"#,
                            size[0],
                            size[1],
                            if case == 0 { 0 } else { MARGIN }
                        ),
                    );
                    assert!(
                        error <= 2,
                        "sceneUnits={scene_units} case={case} size={size:?}: max error {error}"
                    );
                    if cycle == 0 {
                        first.push(actual);
                    } else {
                        assert_eq!(
                            actual, first[view],
                            "same physical view must reproduce exact pixels"
                        );
                        assert_eq!(
                            work.texture_allocations, 0,
                            "all effect texture capacities are warm"
                        );
                        assert_eq!(work.retained_texture_bytes, plateau);
                    }
                    let clean = fixture.render(&device, &queue, &mut scene, clear);
                    assert_eq!(
                        (
                            clean.source_passes,
                            clean.blur_passes,
                            clean.composite_passes,
                            clean.texture_allocations
                        ),
                        (0, 0, 0, 0)
                    );
                }
                if cycle == 0 {
                    plateau = fixture.renderer.effect_capture_texture_bytes();
                }
            }
            let mut fresh = Fixture::new(&device, &queue);
            fresh.render(&device, &queue, &mut scene, clear);
            assert_eq!(
                read(&device, &queue, &fresh.target),
                first[3],
                "cold renderer and resized warm renderer agree exactly"
            );
            if !scene_units && case == 1 {
                fixture.source_bounds_override =
                    Some(Rect::new(Vec2::new(100.0, 100.0), Vec2::new(101.0, 101.0)));
                fixture.renderer.invalidate_effect_captures();
                fixture.render(&device, &queue, &mut scene, clear);
                let wrong = read(&device, &queue, &fixture.target);
                rejected = wrong
                    .iter()
                    .zip(&references[3].0)
                    .map(|(&a, &b)| a.abs_diff(b))
                    .max()
                    .unwrap();
                retain("negative-missing-offscreen-prefix",&wrong,&references[3].0,&[],
                    r#"{"width":80,"height":60,"bound":2,"negativeControl":true,"profile":"omit the entirely offscreen earlier green source from prefix selection; ordinary viewport source pixels are unchanged"}"#.to_owned());
                assert!(
                    rejected > 2,
                    "actual missing-offscreen-source control must fail the unchanged bound"
                );
                fixture.source_bounds_override = None;
                fixture.renderer.invalidate_effect_captures();
                let recovered = fixture.render(&device, &queue, &mut scene, clear);
                assert_eq!(recovered.texture_allocations, 0);
                assert_eq!(read(&device, &queue, &fixture.target), first[3]);
                assert_eq!(recovered.retained_texture_bytes, plateau);
            }
        }
    }
    eprintln!("backdrop resize: 32 profiles, maximum byte error {maximum}; repeated views and cold/warm exact, effect texture plateau stable; actual missing-offscreen-source error {rejected}");
}
