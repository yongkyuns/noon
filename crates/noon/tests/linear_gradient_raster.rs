//! Public typed Rust authoring/animation -> retained renderer -> native pixels.
//! The reference below reads no Runtime paint, uniforms or production sampler.
use noon::{AnimationOptions, Color, GradientStop, LinearGradient, Mobject, RateFunction, Scene};
mod raster_support;
use raster_support::{Raster, SIZE};

fn gradient(vertical: bool) -> LinearGradient {
    LinearGradient::new(
        if vertical { [0.5, 0.0] } else { [0.0, 0.5] },
        if vertical { [0.5, 1.0] } else { [1.0, 0.5] },
        [
            GradientStop::new(
                0.0,
                if vertical {
                    Color::rgb(0.0, 1.0, 0.0)
                } else {
                    Color::rgb(1.0, 0.0, 0.0)
                },
            )
            .unwrap(),
            GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
        ],
    )
    .unwrap()
}

#[test]
#[ignore = "requires a real native adapter; absence fails qualification"]
fn authored_gradient_animation_matches_independent_native_pixel_oracle() {
    let mut comparisons = 0;
    for path in [false, true] {
        let mut raster = pollster::block_on(Raster::with_primary_dimensions(SIZE, SIZE));
        let mut scene = Scene::new();
        let mut source = if path {
            Mobject::manim_rectangle(std::rc::Rc::clone(scene.integration_store()), 4.0, 2.0)
                .unwrap()
        } else {
            scene.rectangle(4.0, 2.0).unwrap()
        };
        source.disable_stroke().unwrap();
        source
            .set_fill_gradient(gradient(false), Some(0.25))
            .unwrap();
        scene.add(&source).unwrap();
        let mut target = source.target_editor().unwrap();
        target
            .set_fill_gradient(gradient(true), Some(0.75))
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
        for time in [0.0, 0.25, 0.5, 0.75, 1.0] {
            scene
                .live(&mut session)
                .advance_segment_to(segment, time)
                .unwrap();
            let pixels = raster.capture(&session.take_renderer_publication());
            for x in [80, 112, 144, 176] {
                for y in [112, 144] {
                    let u = (f64::from(x) + 0.5 - 64.0) / 128.0;
                    let v = (159.5 - f64::from(y)) / 64.0;
                    let dx = 1.0 - time;
                    let dy = time;
                    let t = (((u - time / 2.0) * dx + (v - (1.0 - time) / 2.0) * dy)
                        / (dx * dx + dy * dy))
                        .clamp(0.0, 1.0);
                    let opacity = 0.25 + time / 2.0;
                    let quantize = |value: f64| {
                        ((value * opacity * 65535.0 + 0.5).floor() / 256.0).floor() as u8
                    };
                    let expected = [
                        quantize((1.0 - t) * (1.0 - time)),
                        quantize((1.0 - t) * time),
                        quantize(t),
                        255,
                    ];
                    let offset = ((y * SIZE + x) * 4) as usize;
                    let actual = &pixels[offset..offset + 4];
                    assert!(
                        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                        "path={path} time={time} ({x},{y}): {actual:?} != {expected:?}"
                    );
                    comparisons += 1;
                }
            }
            let repeated = raster.capture(&session.take_renderer_publication());
            assert_eq!(pixels, repeated);
            assert_eq!(
                raster.last_scene_upload_bytes, 0,
                "clean frame uploaded scene data"
            );
        }
    }
    assert_eq!(comparisons, 80);
}
