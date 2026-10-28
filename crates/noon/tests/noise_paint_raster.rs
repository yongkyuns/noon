//! Public native authoring -> shared Fill -> retained GPU, compared with an
//! independent integer/quadrature specification at physical pixel centers.
use noon::{
    AnimationOptions, Color, ManimGeometryOptions, Mobject, NoisePaint, RateFunction, Scene,
};
mod noise_reference;
mod raster_support;
use raster_support::{Raster, SIZE};

#[test]
#[ignore = "requires an actual native GPU adapter; absence fails qualification"]
fn noise_phase_period_roundoff_cannot_retarget_other_axis() {
    let mut pictures = Vec::new();
    for phase in [[0.0; 2], [-1e-20, 0.0]] {
        let mut scene = Scene::new();
        let mut source = scene.rectangle(4.0, 2.0).unwrap();
        source.disable_stroke().unwrap();
        source
            .set_fill_noise(
                NoisePaint::new(7, [8.0; 2], phase, [Color::RED, Color::BLUE]).unwrap(),
                Some(1.0),
            )
            .unwrap();
        scene.add(&source).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut raster = pollster::block_on(Raster::with_primary_dimensions(SIZE, SIZE));
        pictures.push(raster.capture(&session.take_renderer_publication()));
    }
    let differences = pictures[0]
        .iter()
        .zip(&pictures[1])
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differences, 0,
        "a rounded periodic X phase changed the Y lattice"
    );
}

fn frequency(quality: u32, time: f64) -> [f64; 2] {
    match quality {
        0 => [8.0 + 24.0 * time, 8.0 + 4.0 * time],
        1 => [64.0 + 32.0 * time, 64.0 + 16.0 * time],
        _ => [256.0; 2],
    }
}
fn paint(quality: u32, second: bool) -> NoisePaint {
    NoisePaint::new(
        7,
        frequency(quality, if second { 1.0 } else { 0.0 }),
        if second {
            [0.531, -0.719]
        } else {
            [-0.173, 0.231]
        },
        [
            if second {
                Color::rgb(0.0, 1.0, 0.0)
            } else {
                Color::rgba(1.0, 0.0, 0.0, 0.5)
            },
            Color::rgba(0.0, 0.0, 1.0, if second { 0.0 } else { 1.0 }),
        ],
    )
    .unwrap()
}

#[test]
#[ignore = "requires an actual native GPU adapter; absence fails qualification"]
fn noise_pixels_match_independent_seed_filter_pose_time_and_seek_reference() {
    let mut comparisons = 0;
    let mut max_error = 0;
    for kind in 0..4 {
        for transformed in [false, true] {
            for quality in 0..3 {
                let mut raster = pollster::block_on(Raster::with_primary_dimensions(SIZE, SIZE));
                let mut scene = Scene::new();
                let mut source = match kind {
                    0 => scene.rectangle(4.0, 2.0).unwrap(),
                    1 => scene.circle(2.0).unwrap(),
                    2 => Mobject::manim_rectangle(
                        std::rc::Rc::clone(scene.integration_store()),
                        4.0,
                        2.0,
                    )
                    .unwrap(),
                    _ => scene
                        .geometry(
                            ManimGeometryOptions::polygon(&[
                                (-2.0, -1.0),
                                (2.0, -1.0),
                                (2.0, -0.1),
                                (0.3, -0.1),
                                (0.3, 1.0),
                                (-2.0, 1.0),
                            ])
                            .unwrap(),
                        )
                        .unwrap(),
                };
                source.disable_stroke().unwrap();
                source
                    .set_fill_noise(paint(quality, false), Some(0.25))
                    .unwrap();
                source.set_object_opacity(0.5).unwrap();
                let (sx, sy, angle, tx, ty): (f64, f64, f64, f64, f64) = if transformed {
                    (-0.8, 1.2, 0.3, 0.2, -0.1)
                } else {
                    (1.0, 1.0, 0.0, 0.0, 0.0)
                };
                source.set_scale(sx, sy).unwrap();
                source.set_rotation(angle).unwrap();
                source.set_translation(tx, ty).unwrap();
                scene.add(&source).unwrap();
                let mut target = source.target_editor().unwrap();
                target
                    .set_fill_noise(paint(quality, true), Some(0.75))
                    .unwrap();
                let animation = scene
                    .declare_transform_to(
                        &source,
                        &target,
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    )
                    .unwrap();
                let mut session = scene.execution_session().unwrap();
                let segment = scene.live(&mut session).play_animation(&animation).unwrap();
                let mut quarter = None;
                for time in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    scene
                        .live(&mut session)
                        .advance_segment_to(segment, time)
                        .unwrap();
                    let pixels = raster.capture(&session.take_renderer_publication());
                    let frequency = frequency(quality, time);
                    let phase = [-0.173 + 0.704 * time, 0.231 - 0.95 * time];
                    let height = if kind == 1 { 4.0 } else { 2.0 };
                    let widths = [
                        frequency[0] * (angle.cos().abs() + angle.sin().abs())
                            / sx.abs()
                            / 4.0
                            / 32.0,
                        frequency[1] * (angle.cos().abs() + angle.sin().abs())
                            / sy.abs()
                            / height
                            / 32.0,
                    ];
                    for x in (56..200).step_by(12) {
                        for y in (56..200).step_by(12) {
                            let dx = (f64::from(x) + 0.5 - 128.0) / 32.0 - tx;
                            let dy = (127.5 - f64::from(y)) / 32.0 - ty;
                            let local = [
                                (angle.cos() * dx + angle.sin() * dy) / sx,
                                (-angle.sin() * dx + angle.cos() * dy) / sy,
                            ];
                            let inside = if kind == 1 {
                                local[0].hypot(local[1]) < 1.8
                            } else {
                                local[0].abs() < 1.8
                                    && local[1].abs() < 0.8
                                    && (kind != 3 || local[0] < 0.1 || local[1] < -0.3)
                            };
                            if !inside {
                                continue;
                            }
                            let q = noise_reference::filtered(
                                7,
                                [local[0] / 4.0 + 0.5, local[1] / height + 0.5],
                                frequency,
                                phase,
                                widths,
                            );
                            let opacity = (0.25 + time * 0.5) * 0.5;
                            let rgb = [
                                0.5 * (1.0 - time) * (1.0 - q),
                                time * (1.0 - q),
                                (1.0 - time) * q,
                            ];
                            let quantize = |v: f64| {
                                ((v * opacity * 65535.0 + 0.5).floor() / 256.0).floor() as u8
                            };
                            let expected =
                                [quantize(rgb[0]), quantize(rgb[1]), quantize(rgb[2]), 255];
                            let offset = ((y * SIZE + x) * 4) as usize;
                            for (&a, e) in pixels[offset..offset + 4].iter().zip(expected) {
                                let error = a.abs_diff(e);
                                max_error = max_error.max(error);
                                assert!(error<=1,"kind={kind} pose={transformed} quality={quality} time={time} ({x},{y}): {:?} != {expected:?}; field={q} widths={widths:?}",&pixels[offset..offset+4]);
                            }
                            comparisons += 1;
                        }
                    }
                    assert_eq!(pixels, raster.capture(&session.take_renderer_publication()));
                    assert_eq!(raster.last_scene_upload_bytes, 0);
                    if time == 0.25 {
                        quarter = Some(pixels);
                    }
                }
                session.seek(0.25).unwrap();
                assert_eq!(
                    quarter.unwrap(),
                    raster.capture(&session.take_renderer_publication())
                );
                session.seek(1.0).unwrap();
                scene.live(&mut session).complete_segment(segment).unwrap();
            }
        }
    }
    assert!(comparisons > 4000);
    println!("noise independent field: 24 profiles, 5 times, {comparisons} pixels, maximum byte error {max_error}; clean upload zero; rewind exact");
}
