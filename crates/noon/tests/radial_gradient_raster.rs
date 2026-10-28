//! Independent elliptical-field and premultiplied-time reference over native pixels.
use noon::{AnimationOptions, Color, GradientStop, Mobject, RadialGradient, RateFunction, Scene};
mod raster_support;
use raster_support::{Raster, SIZE};

fn gradient(second: bool) -> RadialGradient {
    RadialGradient::new(
        if second { [0.65, 0.35] } else { [0.25, 0.5] },
        if second { [0.75, 0.4] } else { [0.5, 0.75] },
        [
            GradientStop::new(
                0.0,
                if second {
                    Color::rgb(0.0, 1.0, 0.0)
                } else {
                    Color::rgba(1.0, 0.0, 0.0, 0.5)
                },
            )
            .unwrap(),
            GradientStop::new(
                1.0,
                Color::rgba(0.0, 0.0, 1.0, if second { 0.0 } else { 1.0 }),
            )
            .unwrap(),
        ],
    )
    .unwrap()
}

#[test]
#[ignore = "requires an actual native GPU adapter; absence fails qualification"]
fn radial_pixels_match_independent_field_alpha_pose_time_and_seek_reference() {
    let mut comparisons = 0;
    let mut max_error = 0;
    for kind in 0..3 {
        for transformed in [false, true] {
            let mut raster = pollster::block_on(Raster::with_primary_dimensions(SIZE, SIZE));
            let mut scene = Scene::new();
            let mut source = match kind {
                0 => scene.rectangle(4.0, 2.0).unwrap(),
                1 => scene.circle(2.0).unwrap(),
                _ => Mobject::manim_rectangle(
                    std::rc::Rc::clone(scene.integration_store()),
                    4.0,
                    2.0,
                )
                .unwrap(),
            };
            source.disable_stroke().unwrap();
            source
                .set_fill_radial_gradient(gradient(false), Some(0.25))
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
                .set_fill_radial_gradient(gradient(true), Some(0.75))
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
            let mut first_quarter = None;
            for time in [0.0, 0.25, 0.5, 0.75, 1.0] {
                scene
                    .live(&mut session)
                    .advance_segment_to(segment, time)
                    .unwrap();
                let pixels = raster.capture(&session.take_renderer_publication());
                let center = [0.25 + 0.4 * time, 0.5 - 0.15 * time];
                let radii = [0.5 + 0.25 * time, 0.75 - 0.35 * time];
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
                            local[0].abs() < 1.8 && local[1].abs() < 0.8
                        };
                        if !inside {
                            continue;
                        }
                        let height = if kind == 1 { 4.0 } else { 2.0 };
                        let u = local[0] / 4.0 + 0.5;
                        let v = local[1] / height + 0.5;
                        // Analytic ellipse plus literal premultiplied stop/time vectors;
                        // no production sampler, effective paint or uniform is read.
                        let q = ((u - center[0]) / radii[0])
                            .hypot((v - center[1]) / radii[1])
                            .min(1.0);
                        let opacity = (0.25 + time * 0.5) * 0.5;
                        let rgb = [
                            0.5 * (1.0 - time) * (1.0 - q),
                            time * (1.0 - q),
                            (1.0 - time) * q,
                        ];
                        let quantize =
                            |v: f64| ((v * opacity * 65535.0 + 0.5).floor() / 256.0).floor() as u8;
                        let expected = [quantize(rgb[0]), quantize(rgb[1]), quantize(rgb[2]), 255];
                        let offset = ((y * SIZE + x) * 4) as usize;
                        for (&actual, expected) in pixels[offset..offset + 4].iter().zip(expected) {
                            let error = actual.abs_diff(expected);
                            max_error = max_error.max(error);
                            assert!(error <= 1, "kind={kind} pose={transformed} time={time} ({x},{y}): {:?} != {expected}", &pixels[offset..offset+4]);
                        }
                        comparisons += 1;
                    }
                }
                assert_eq!(pixels, raster.capture(&session.take_renderer_publication()));
                assert_eq!(raster.last_scene_upload_bytes, 0);
                if time == 0.25 {
                    first_quarter = Some(pixels);
                }
            }
            session.seek(0.25).unwrap();
            assert_eq!(
                first_quarter.unwrap(),
                raster.capture(&session.take_renderer_publication())
            );
            session.seek(1.0).unwrap();
            scene.live(&mut session).complete_segment(segment).unwrap();
        }
    }
    assert!(comparisons > 1_000);
    println!("radial independent field: 6 geometry/pose profiles, 5 times, {comparisons} pixels, maximum byte error {max_error}; clean upload zero; rewind exact");
}
