//! End-to-end native paint qualification against independent octave composition,
//! masked integer noise and piecewise quadrature. No production sampler is used.
use noon::integration::scene_execution;
use noon::{AnimationOptions, Color, GradientStop, NoisePaint, RateFunction, Scene};
mod noise_reference;
mod raster_support;
use raster_support::Raster;

fn field(
    uv: [f64; 2],
    frequency: [f64; 2],
    phase: [f64; 2],
    widths: [f64; 2],
    octaves: u8,
    lacunarity: f64,
    gain: f64,
) -> f64 {
    let mut value = 0.0;
    let mut normalizer = 0.0;
    for i in 0..octaves {
        let scale = lacunarity.powi(i32::from(i));
        let weight = gain.powi(i32::from(i));
        let seed = ((u64::from(u32::MAX) + u64::from(i) * 0x9e37_79b9) & 0xffff_ffff) as u32;
        value += weight
            * noise_reference::filtered(
                seed,
                uv,
                frequency.map(|v| v * scale),
                phase.map(|v| v * scale),
                widths.map(|v| v * scale),
            );
        normalizer += weight;
    }
    value / normalizer
}
fn paint(
    octaves: u8,
    lacunarity: f64,
    frequency: [f64; 2],
    phase: [f64; 2],
    gain: f64,
) -> NoisePaint {
    NoisePaint::fractal(Color::BLACK, Color::WHITE)
        .unwrap()
        .with_seed(u32::MAX)
        .with_frequency(frequency)
        .unwrap()
        .with_phase(phase)
        .unwrap()
        .with_detail(octaves, lacunarity, gain)
        .unwrap()
        .with_stops([
            GradientStop::new(0.0, Color::rgba(1.0, 0.0, 0.0, 0.0)).unwrap(),
            GradientStop::new(0.4, Color::rgb(0.0, 1.0, 0.0)).unwrap(),
            GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
        ])
        .unwrap()
}

#[test]
#[ignore = "requires an actual native GPU adapter; absence fails qualification"]
fn fractal_fields_ramps_pose_filter_animation_seek_and_clean_upload_match_reference() {
    let mut count = 0;
    let mut maximum_error = 0;
    for (octaves, frequency, lacunarity) in [
        (1, [8.0; 2], 2.0),
        (4, [8.0, 12.0], 2.0),
        (8, [1.0; 2], 2.0),
        (5, [3.0, 2.0], 1.5),
        (4, [4.0; 2], 4.0),
        (4, [8.0; 2], 1.0),
    ] {
        for transformed in [false, true] {
            for size in [[4.0, 2.0], [0.7, 0.5]] {
                let (sx, sy, angle): (f64, f64, f64) = if transformed {
                    (-0.8, 1.2, 0.3)
                } else {
                    (1.0, 1.0, 0.0)
                };
                let mut scene = Scene::new();
                let mut source = scene.rectangle(size[0], size[1]).unwrap();
                source.disable_stroke().unwrap();
                source.set_scale(sx, sy).unwrap();
                source.set_rotation(angle).unwrap();
                source
                    .set_fill_noise(
                        paint(octaves, lacunarity, frequency, [-0.173, 0.231], 0.3),
                        Some(0.7),
                    )
                    .unwrap();
                source.set_object_opacity(0.6).unwrap();
                scene.add(&source).unwrap();
                let mut target = source.target_editor().unwrap();
                target
                    .set_fill_noise(
                        paint(octaves, lacunarity, frequency, [0.531, -0.719], 0.7)
                            .with_stops([
                                GradientStop::new(0.0, Color::rgba(1.0, 0.0, 0.0, 0.4)).unwrap(),
                                GradientStop::new(0.6, Color::rgb(1.0, 1.0, 0.0)).unwrap(),
                                GradientStop::new(1.0, Color::rgba(0.0, 0.0, 1.0, 0.5)).unwrap(),
                            ])
                            .unwrap(),
                        None,
                    )
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
                let mut session = scene_execution::lower(&scene).unwrap();
                let segment = scene_execution::borrow_uninstalled(&scene, &mut session)
                    .play_animation(&animation)
                    .unwrap();
                let mut raster = pollster::block_on(Raster::with_primary_dimensions(256, 256));
                let mut quarter = None;
                for t in [0.0, 0.25, 0.5, 1.0] {
                    scene_execution::borrow_uninstalled(&scene, &mut session)
                        .advance_segment_to(segment, t)
                        .unwrap();
                    let pixels = raster.capture(&session.take_renderer_publication());
                    let phase = [-0.173 + 0.704 * t, 0.231 - 0.95 * t];
                    let widths = [
                        frequency[0] * (angle.cos().abs() + angle.sin().abs())
                            / sx.abs()
                            / size[0]
                            / 32.0,
                        frequency[1] * (angle.cos().abs() + angle.sin().abs())
                            / sy.abs()
                            / size[1]
                            / 32.0,
                    ];
                    for x in (50..206).step_by(3) {
                        for y in (50..206).step_by(3) {
                            let dx = (f64::from(x) + 0.5 - 128.0) / 32.0;
                            let dy = (127.5 - f64::from(y)) / 32.0;
                            let local = [
                                (angle.cos() * dx + angle.sin() * dy) / sx,
                                (-angle.sin() * dx + angle.cos() * dy) / sy,
                            ];
                            if local[0].abs() >= size[0] * 0.42 || local[1].abs() >= size[1] * 0.42
                            {
                                continue;
                            }
                            let uv = [local[0] / size[0] + 0.5, local[1] / size[1] + 0.5];
                            let q = field(
                                uv,
                                frequency,
                                phase,
                                widths,
                                octaves,
                                lacunarity,
                                0.3 + 0.4 * t,
                            );
                            let middle = 0.4 + 0.2 * t;
                            let rgb = if q < middle {
                                let u = q / middle;
                                [0.4 * t * (1.0 - u) + t * u, u, 0.0]
                            } else {
                                let u = (q - middle) / (1.0 - middle);
                                [t * (1.0 - u), 1.0 - u, (1.0 - 0.5 * t) * u]
                            };
                            let quantize =
                                |v: f64| ((v * 0.42 * 65535.0 + 0.5).floor() / 256.0).floor() as u8;
                            let expected =
                                [quantize(rgb[0]), quantize(rgb[1]), quantize(rgb[2]), 255];
                            let at = ((y * 256 + x) * 4) as usize;
                            for (&actual, expected) in pixels[at..at + 4].iter().zip(expected) {
                                let error = actual.abs_diff(expected);
                                maximum_error = maximum_error.max(error);
                                assert!(error<=1,"octaves={octaves} size={size:?} pose={transformed} time={t} ({x},{y}) q={q}: {:?} != {:?}",&pixels[at..at+4],expected);
                            }
                            count += 1;
                        }
                    }
                    assert_eq!(pixels, raster.capture(&session.take_renderer_publication()));
                    assert_eq!(raster.last_scene_upload_bytes, 0);
                    if t == 0.25 {
                        #[cfg(feature = "image-decode")]
                        if let Some(dir) = std::env::var_os("NOON_FRACTAL_NATIVE_ARTIFACTS") {
                            let dir = std::path::PathBuf::from(dir);
                            std::fs::create_dir_all(&dir).unwrap();
                            let path = dir.join(format!(
                                "octaves-{octaves}-lacunarity-{lacunarity}-pose-{transformed}-width-{}.png",
                                size[0]
                            ));
                            image::save_buffer(path, &pixels, 256, 256, image::ColorType::Rgba8)
                                .unwrap();
                        }
                        quarter = Some(pixels);
                    }
                }
                session.seek(0.25).unwrap();
                assert_eq!(
                    quarter.unwrap(),
                    raster.capture(&session.take_renderer_publication())
                );
                session.seek(1.0).unwrap();
                scene_execution::borrow_uninstalled(&scene, &mut session)
                    .complete_segment(segment)
                    .unwrap();
            }
        }
    }
    assert!(count > 5000);
    println!("fractal reference: {count} pixel comparisons, maximum error {maximum_error}/255, clean upload zero, seek exact");
}

#[test]
#[ignore = "requires an actual native GPU adapter; absence fails qualification"]
fn fractal_one_octave_and_zero_gain_preserve_the_original_single_scale_picture() {
    let colors = [Color::rgba(1.0, 0.0, 0.0, 0.5), Color::BLUE];
    let base = NoisePaint::new(u32::MAX, [8.0; 2], [-0.173, 0.231], colors).unwrap();
    let variants = [
        base.clone(),
        base.clone().with_detail(1, 2.0, 0.7).unwrap(),
        base.with_detail(4, 2.0, 0.0).unwrap(),
    ];
    let mut pictures = Vec::new();
    let mut raster = pollster::block_on(Raster::with_primary_dimensions(256, 256));
    for value in variants {
        let mut scene = Scene::new();
        let mut source = scene.rectangle(4.0, 2.0).unwrap();
        source.disable_stroke().unwrap();
        source.set_fill_noise(value, Some(0.7)).unwrap();
        scene.add(&source).unwrap();
        let mut session = scene_execution::lower(&scene).unwrap();
        pictures.push(raster.capture(&session.take_renderer_publication()));
    }
    assert_eq!(pictures[0], pictures[1]);
    assert_eq!(pictures[0], pictures[2]);
}

#[test]
#[ignore = "requires an actual native GPU adapter; absence fails qualification"]
fn fractal_matches_correctly_weighted_native_layers_with_one_quarter_the_phase_uploads() {
    // For the opaque affine black/white ramp, layer i at weight_i/cumulative_i
    // produces precisely the normalized field under source-over. Applying the
    // same arbitrary alpha to every layer would be a different visual effect.
    // Nonlinear color ramps are excluded: their scalar mapping happens after
    // accumulation and cannot be reproduced by layering individually mapped colors.
    let seed = 7u32;
    let single = |octave: u8, phase: [f64; 2]| {
        let scale = 2.0_f64.powi(i32::from(octave));
        NoisePaint::new(
            seed.wrapping_add(u32::from(octave).wrapping_mul(0x9e37_79b9)),
            [8.0 * scale; 2],
            phase.map(|value| value * scale),
            [Color::BLACK, Color::WHITE],
        )
        .unwrap()
    };
    let fractal = |phase| {
        NoisePaint::fractal(Color::BLACK, Color::WHITE)
            .unwrap()
            .with_seed(seed)
            .with_phase(phase)
            .unwrap()
    };
    let mut images = Vec::new();
    let mut changed_bytes = Vec::new();
    for layered in [false, true] {
        let mut scene = Scene::new();
        let mut objects = Vec::new();
        let mut cumulative = 0.0;
        for octave in 0..if layered { 4 } else { 1 } {
            let mut object = scene.rectangle(4.0, 2.0).unwrap();
            object.disable_stroke().unwrap();
            let weight = 0.5_f64.powi(i32::from(octave));
            cumulative += weight;
            object
                .set_fill_noise(
                    if layered {
                        single(octave, [-0.173, 0.231])
                    } else {
                        fractal([-0.173, 0.231])
                    },
                    Some(weight / cumulative),
                )
                .unwrap();
            scene.add(&object).unwrap();
            objects.push(object);
        }
        let mut running = scene.into_execution().unwrap();
        let mut raster = pollster::block_on(Raster::with_primary_dimensions(256, 256));
        let initial = raster.capture(&running.session_mut().take_renderer_publication());
        assert_eq!(
            initial,
            raster.capture(&running.session_mut().take_renderer_publication())
        );
        assert_eq!(raster.last_scene_upload_bytes, 0);
        for (octave, object) in objects.iter().enumerate() {
            running
                .scene_mut()
                .set_fill_noise(
                    object,
                    if layered {
                        single(octave as u8, [0.531, -0.719])
                    } else {
                        fractal([0.531, -0.719])
                    },
                    None,
                )
                .unwrap();
        }
        let changed = raster.capture(&running.session_mut().take_renderer_publication());
        changed_bytes.push(raster.last_scene_upload_bytes);
        assert_eq!(raster.last_scene_repacked, objects.len());
        assert_eq!(
            changed,
            raster.capture(&running.session_mut().take_renderer_publication())
        );
        assert_eq!(raster.last_scene_upload_bytes, 0);
        images.push([initial, changed]);
    }
    let mut maximum_error = 0;
    let mut maximum_reference_error = 0;
    let mut comparisons = 0;
    for (time, (fractal_image, layered_image)) in images[0].iter().zip(&images[1]).enumerate() {
        let phase = if time == 0 {
            [-0.173, 0.231]
        } else {
            [0.531, -0.719]
        };
        for y in 99..157 {
            for x in 67..189 {
                let offset = (y * 256 + x) * 4;
                let uv = [
                    (x as f64 + 0.5 - 128.0) / 128.0 + 0.5,
                    (127.5 - y as f64) / 64.0 + 0.5,
                ];
                let quantize = |v: f64| ((v * 65535.0 + 0.5).floor() / 256.0).floor();
                let mut scalar = 0.0;
                let mut cumulative = 0.0;
                let mut layer_byte = 0.0;
                for octave in 0..4 {
                    let scale = 2.0_f64.powi(octave);
                    let weight = 0.5_f64.powi(octave);
                    let octave_seed =
                        ((u64::from(seed) + octave as u64 * 0x9e37_79b9) & 0xffff_ffff) as u32;
                    let q = noise_reference::filtered(
                        octave_seed,
                        uv,
                        [8.0 * scale; 2],
                        phase.map(|v| v * scale),
                        [scale / 16.0, scale / 8.0],
                    );
                    scalar += weight * q;
                    cumulative += weight;
                    // The ordinary LDR source path quantizes premultiplied
                    // RGB and alpha before each fixed-function source-over.
                    // Repeated layers therefore need their own byte oracle;
                    // they are not byte-identical to one final quantization.
                    let opacity = weight / cumulative;
                    layer_byte = (quantize(q * opacity)
                        + layer_byte * (1.0 - quantize(opacity) / 255.0))
                        .round();
                }
                let expected = [quantize(scalar / cumulative) as u8, layer_byte as u8];
                for channel in 0..4 {
                    let error =
                        fractal_image[offset + channel].abs_diff(layered_image[offset + channel]);
                    maximum_error = maximum_error.max(error);
                    for (route, pixels) in [fractal_image, layered_image].into_iter().enumerate() {
                        let expected = if channel == 3 { 255 } else { expected[route] };
                        let error = pixels[offset + channel].abs_diff(expected);
                        maximum_reference_error = maximum_reference_error.max(error);
                        assert!(
                            error <= 1,
                            "route={route} quantized reference differs by {error} at ({x},{y})"
                        );
                    }
                }
                comparisons += 1;
            }
        }
    }
    assert!(changed_bytes[0] > 0);
    assert_eq!(changed_bytes[1], 4 * changed_bytes[0]);
    println!(
        "fractal/layered native: {comparisons} interior pixels, respective quantized references max {maximum_reference_error}/255, between-route quantization difference {maximum_error}/255; phase upload {} vs {} bytes; clean upload zero; resources one vs four painted rows; no timing claim",
        changed_bytes[0], changed_bytes[1]
    );
}
