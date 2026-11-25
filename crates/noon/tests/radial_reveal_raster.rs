//! Native Rust authoring -> shared visibility -> retained GPU, checked against
//! an independent field on non-square, transformed, image and composed coverage.
use noon::{effects::EffectUpdate, Color, ImageMobjectOptions, RasterImageSampling, Scene};
mod radial_reference;
mod raster_support;
use radial_reference::{sample, Profile};
use raster_support::{Raster, SIZE};

fn update(profile: Profile, progress: f64) -> EffectUpdate {
    let color = |c: [f64; 4]| Color::rgba(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32);
    EffectUpdate::default()
        .progress(progress)
        .softness(profile.softness)
        .seed(profile.seed)
        .reveal_center(profile.center)
        .reveal_axes(profile.axes)
        .reveal_hole(profile.hole)
        .reveal_roughness(profile.roughness)
        .reveal_frequency(profile.frequency)
        .reveal_rim_width(profile.rim_width)
        .reveal_inner_color(color(profile.inner))
        .reveal_outer_color(color(profile.outer))
}
#[test]
#[ignore = "requires a real native adapter; absence fails qualification"]
fn radial_pixels_match_independent_capture_bounds_alpha_transform_and_seek_reference() {
    let mut comparisons = 0usize;
    let mut maximum_error = 0u8;
    for kind in 0..3 {
        for transformed in [false, true] {
            for burn in [false, true] {
                for hole in [false, true] {
                    let mut scene = Scene::new();
                    let mut leaves = Vec::new();
                    if kind == 2 {
                        let mut pixels = Vec::new();
                        for y in 0..4 {
                            for x in 0..4 {
                                let center = (1..=2).contains(&x) && (1..=2).contains(&y);
                                pixels.extend(if center {
                                    [255, 0, 255, 0]
                                } else {
                                    [255, 255, 255, if (x + y) % 2 == 0 { 128 } else { 255 }]
                                });
                            }
                        }
                        let mut options = ImageMobjectOptions::rgba8(4, 4, pixels).unwrap();
                        options.set_height(2.0).unwrap();
                        options.set_sampling(RasterImageSampling::Nearest);
                        leaves.push(scene.image(options).unwrap());
                    } else {
                        for _ in 0..if kind == 1 { 2 } else { 1 } {
                            let mut panel = scene
                                .rectangle(
                                    if kind == 1 { 2.0 } else { 4.0 },
                                    if kind == 1 { 1.4 } else { 2.0 },
                                )
                                .unwrap();
                            panel.disable_stroke().unwrap();
                            panel.set_fill(1.0, 1.0, 1.0, 0.55).unwrap();
                            leaves.push(panel);
                        }
                    }
                    let (sx, sy, angle, tx, ty): (f64, f64, f64, f64, f64) = if transformed {
                        (0.8, 1.3, 0.31, 0.23, -0.17)
                    } else {
                        (1.0, 1.0, 0.0, 0.0, 0.0)
                    };
                    for (i, leaf) in leaves.iter_mut().enumerate() {
                        let local_x = if kind == 1 {
                            if i == 0 {
                                -0.4
                            } else {
                                0.4
                            }
                        } else {
                            0.0
                        };
                        // Image pixels have a constructor scale of 2 / 4;
                        // compose this test pose with the authored height.
                        let authored_scale = if kind == 2 { 0.5 } else { 1.0 };
                        leaf.set_scale(authored_scale * sx, authored_scale * sy)
                            .unwrap();
                        leaf.set_rotation(angle).unwrap();
                        leaf.set_translation(
                            tx + local_x * sx * angle.cos(),
                            ty + local_x * sx * angle.sin(),
                        )
                        .unwrap();
                    }
                    let mut family = (kind == 1).then(|| {
                        scene
                            .family(&leaves.iter().map(Into::into).collect::<Vec<_>>())
                            .unwrap()
                    });
                    let profile = Profile::new(burn, hole);
                    if let Some(family) = family.as_mut() {
                        scene.add_many(&[(&*family).into()]).unwrap();
                    } else {
                        scene.add(&leaves[0]).unwrap();
                    }
                    // An unrelated distant object must remain byte-identical across local edits.
                    let mut distant = scene.rectangle(0.3, 0.3).unwrap();
                    distant.set_fill(0.2, 0.8, 0.3, 1.0).unwrap();
                    distant.disable_stroke().unwrap();
                    distant.set_translation(-3.5, 3.5).unwrap();
                    scene.add(&distant).unwrap();
                    let mut execution = scene.into_execution().unwrap();
                    let mut raster =
                        pollster::block_on(Raster::with_primary_dimensions(SIZE, SIZE));
                    let source =
                        raster.capture(&execution.session_mut().take_renderer_publication());
                    let target = family
                        .as_ref()
                        .map_or_else(|| (&leaves[0]).into(), noon::MobjectTarget::from);
                    execution
                        .scene_mut()
                        .set_reveal(target, update(profile, 0.0))
                        .unwrap();
                    let (hx, hy) = match kind {
                        0 => (2.0, 1.0),
                        1 => (1.4, 0.7),
                        _ => (1.0, 1.0),
                    };
                    let half = [
                        hx * sx * angle.cos().abs() + hy * sy * angle.sin().abs(),
                        hx * sx * angle.sin().abs() + hy * sy * angle.cos().abs(),
                    ];
                    let min = [128.0 + 32.0 * (tx - half[0]), 128.0 - 32.0 * (ty + half[1])];
                    let dimensions = half.map(|h| 64.0 * h);
                    let mut saved = None;
                    for progress in [0.0, 0.25, 0.5, 0.75, 1.0, 0.25] {
                        let target = family
                            .as_ref()
                            .map_or_else(|| (&leaves[0]).into(), noon::MobjectTarget::from);
                        execution
                            .scene_mut()
                            .set_reveal(target, EffectUpdate::default().progress(progress))
                            .unwrap();
                        let pixels = raster
                            .capture_effects(&execution.session_mut().take_renderer_publication());
                        for y in 30..226 {
                            for x in 30..226 {
                                let at = ((y * SIZE + x) * 4) as usize;
                                let alpha = f64::from(source[at]) / 255.0;
                                let expected = sample(
                                    [alpha; 4],
                                    [
                                        (f64::from(x) + 0.5 - min[0]) / dimensions[0],
                                        1.0 - (f64::from(y) + 0.5 - min[1]) / dimensions[1],
                                    ],
                                    dimensions,
                                    progress,
                                    profile,
                                );
                                for (c, value) in expected.iter().take(3).enumerate() {
                                    let error =
                                        pixels[at + c].abs_diff((value * 255.0).round() as u8);
                                    maximum_error = maximum_error.max(error);
                                    assert!(error<=2,"kind={kind} transformed={transformed} burn={burn} hole={hole} progress={progress} ({x},{y}) channel{c}: {} != {}; alpha={alpha}",pixels[at+c],value*255.0);
                                }
                                comparisons += 1;
                            }
                        }
                        assert_eq!(
                            &pixels[((16 * SIZE + 16) * 4) as usize
                                ..((16 * SIZE + 16) * 4 + 4) as usize],
                            &source[((16 * SIZE + 16) * 4) as usize
                                ..((16 * SIZE + 16) * 4 + 4) as usize]
                        );
                        if progress == 0.25 {
                            if let Some(prior) = saved.as_ref() {
                                assert_eq!(
                                    prior, &pixels,
                                    "backward progress seek is deterministic"
                                );
                            } else {
                                saved = Some(pixels.clone());
                            }
                        }
                        assert_eq!(
                            pixels,
                            raster.capture_effects(
                                &execution.session_mut().take_renderer_publication()
                            )
                        );
                        assert_eq!(
                            raster.last_scene_upload_bytes, 0,
                            "clean frame uploads nothing"
                        );
                        assert_eq!(
                            raster.last_scene_path_batches, 0,
                            "clean frame retessellates nothing"
                        );
                        if progress == 0.5 && hole {
                            if let Ok(dir) = std::env::var("NOON_RADIAL_NATIVE_ARTIFACTS") {
                                #[cfg(feature = "image-decode")]
                                {
                                    std::fs::create_dir_all(&dir).unwrap();
                                    image::save_buffer(
                                        std::path::Path::new(&dir).join(format!(
                                            "radial-{kind}-{transformed}-{burn}.png"
                                        )),
                                        &pixels,
                                        SIZE,
                                        SIZE,
                                        image::ColorType::Rgba8,
                                    )
                                    .unwrap();
                                }
                                #[cfg(not(feature = "image-decode"))]
                                let _ = dir;
                            }
                        }
                    }
                }
            }
        }
    }
    println!("radial independent field:24 source/pose/edge/orientation profiles,6 times,{comparisons} pixel comparisons,max byte error{maximum_error};clean upload zero;parameter rewind exact");
}

#[test]
#[ignore = "requires a real native adapter; absence fails qualification"]
fn under_resolved_roughness_fades_without_seed_flicker_or_geometry_uploads() {
    let mut scene = Scene::new();
    let mut panel = scene.rectangle(4.0, 2.0).unwrap();
    panel.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    panel.disable_stroke().unwrap();
    panel.set_scale(0.05, 0.05).unwrap();
    scene.add(&panel).unwrap();
    let mut execution = scene.into_execution().unwrap();
    let mut raster = pollster::block_on(Raster::with_primary_dimensions(SIZE, SIZE));
    let original = raster.capture(&execution.session_mut().take_renderer_publication());
    let mut profile = Profile::new(true, false);
    profile.center = [0.5; 2];
    profile.axes = [0.5; 2];
    profile.softness = 0.0;
    profile.frequency = 64.0;
    profile.roughness = 1.0;
    execution
        .scene_mut()
        .set_reveal(&panel, update(profile, 0.5))
        .unwrap();
    let first = raster.capture_effects(&execution.session_mut().take_renderer_publication());
    assert_ne!(first, original);
    for seed in [0, u32::MAX, 123] {
        execution
            .scene_mut()
            .set_reveal(&panel, EffectUpdate::default().seed(seed))
            .unwrap();
        let pixels = raster.capture_effects(&execution.session_mut().take_renderer_publication());
        assert_eq!(
            pixels, first,
            "subpixel roughness must converge to its zero-mean field"
        );
        assert_eq!(raster.last_scene_upload_bytes, 0);
    }
    for y in 120..136 {
        for x in 120..136 {
            let at = ((y * SIZE + x) * 4) as usize;
            let alpha = f64::from(original[at]) / 255.0;
            let expected = sample(
                [alpha; 4],
                [
                    (f64::from(x) + 0.5 - 124.8) / 6.4,
                    1.0 - (f64::from(y) + 0.5 - 126.4) / 3.2,
                ],
                [6.4, 3.2],
                0.5,
                profile,
            );
            for (c, value) in expected.iter().take(3).enumerate() {
                assert!(first[at + c].abs_diff((value * 255.0).round() as u8) <= 2);
            }
        }
    }
}
