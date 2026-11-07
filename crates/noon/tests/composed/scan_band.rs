//! Independent polygon-area oracle for the external ScanBand shader.
use super::super::*;
use noon::effects::{ShaderEffect, ShaderValue, WgslEffectProgram};
use noon::example_scenes::scan_band;

// Clip a unit pixel square by two band half-planes. This does not use the
// shader's projected-box CDF. Coordinates are relative to the pixel center.
fn coverage(center: f64, gradient: [f64; 2], width: f64, phase: f64) -> f64 {
    let mut polygon = vec![[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
    for sign in [-1.0, 1.0] {
        let distance = |p: [f64; 2]| {
            sign * (center + gradient[0] * p[0] + gradient[1] * p[1] - phase) - width / 2.0
        };
        let mut clipped = Vec::new();
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            let da = distance(a);
            let db = distance(b);
            if da <= 0.0 {
                clipped.push(a);
            }
            if (da <= 0.0) != (db <= 0.0) {
                let t = da / (da - db);
                clipped.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
            }
        }
        polygon = clipped;
    }
    (0..polygon.len())
        .map(|i| {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

#[test]
fn scan_band_schema_and_independent_edge_reference() {
    assert!((coverage(0.5, [1.0, 0.0], 0.2, 0.5) - 0.2).abs() < 1e-12);
    assert!((coverage(0.5, [0.5, 0.5], 0.2, 0.5) - 0.36).abs() < 1e-12);
    for x in [0.0, 0.5, 1.0] {
        assert_eq!(
            coverage(x, [0.0; 2], 0.2, 0.5),
            if x == 0.5 { 1.0 } else { 0.0 }
        );
    }
    let samples = 256;
    let mut hits = 0;
    for y in 0..samples {
        for x in 0..samples {
            let p = [
                f64::from(x + 1) / f64::from(samples) - 0.5,
                f64::from(y + 1) / f64::from(samples) - 0.5,
            ];
            hits += usize::from((0.5 * p[0] + 0.5 * p[1]).abs() <= 0.1);
        }
    }
    assert!((hits as f64 / f64::from(samples * samples) - 0.36).abs() < 0.005);
    let program = scan_band::material().unwrap();
    for (name, invalid) in [
        ("width", 0.0),
        ("width", -0.1),
        ("width", 1.1),
        ("phase", -0.1),
        ("phase", 1.1),
        ("phase", f64::NAN),
    ] {
        assert!(ShaderEffect::new(program.clone())
            .with_parameter(
                &program.parameter(name).unwrap(),
                ShaderValue::Scalar(invalid)
            )
            .is_err());
    }
}

fn panel(program: Option<WgslEffectProgram>) -> Scene {
    let mut scene = Scene::new();
    let mut panel = scene.rectangle(4.0, 2.0).unwrap();
    panel.disable_stroke().unwrap();
    panel.set_fill(1.0, 1.0, 1.0, 0.5).unwrap();
    panel.set_rotation(0.37).unwrap();
    if let Some(program) = program {
        let definition = ShaderEffect::new(program.clone())
            .with_parameter(
                &program.parameter("phase").unwrap(),
                ShaderValue::Scalar(0.5),
            )
            .unwrap();
        panel.add_effect(definition, "scan").unwrap();
    }
    scene.add(&panel).unwrap();
    scene
}

fn reference(input: &[u8], size: [u32; 2], camera: Camera2D, phase: f64) -> Vec<u8> {
    let (sin, cos) = 0.37_f64.sin_cos();
    let gradient = [
        cos * f64::from(camera.world_size.x) / f64::from(size[0]) / 4.0,
        -sin * f64::from(camera.world_size.y) / f64::from(size[1]) / 4.0,
    ];
    let mut output = vec![0; input.len()];
    for y in 0..size[1] {
        for x in 0..size[0] {
            let world_x = f64::from(camera.center.x)
                + ((f64::from(x) + 0.5) / f64::from(size[0]) - 0.5)
                    * f64::from(camera.world_size.x);
            let world_y = f64::from(camera.center.y)
                + (0.5 - (f64::from(y) + 0.5) / f64::from(size[1]))
                    * f64::from(camera.world_size.y);
            let mask = coverage(
                0.5 + (cos * world_x + sin * world_y) / 4.0,
                gradient,
                0.2,
                phase,
            );
            let at = ((y * size[0] + x) * 4) as usize;
            for c in [0, 1, 3] {
                output[at + c] = (f64::from(input[at + c]) * mask).round() as u8;
            }
        }
    }
    output
}

#[test]
#[ignore = "requires a real adapter; external ScanBand pixels and authored seek"]
fn scan_band_pixels_and_rewind_match_independent_area() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("ScanBand adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut maximum = 0;
    let mut samples = 0;
    for size in [[80, 60], [160, 120], [317, 179]] {
        for (center, world) in [
            (Vec2::ZERO, Vec2::new(8.0, 6.0)),
            (Vec2::new(0.6, -0.25), Vec2::new(6.4, 4.8)),
        ] {
            let camera = Camera2D::new(center, world).unwrap();
            let mut execution = scan_band::execution().unwrap();
            let mut baseline = panel(None).execution_session().unwrap();
            let mut surface =
                Surface::with_target_size(&device, &queue, Vec2::new(80.0, 60.0), size);
            let mut plain = Surface::with_target_size(&device, &queue, Vec2::new(80.0, 60.0), size);
            surface.renderer.set_camera(&queue, camera);
            plain.renderer.set_camera(&queue, camera);
            let input = plain.render_on(&device, &queue, &mut baseline, wgpu::Color::TRANSPARENT);
            let mut midpoint = None;
            for time in [0.0, 0.5, 1.0, 1.5, 2.0, 0.0, 1.0] {
                execution.session_mut().seek(time).unwrap();
                let actual = surface.render_on(
                    &device,
                    &queue,
                    execution.session_mut(),
                    wgpu::Color::TRANSPARENT,
                );
                let expected = reference(&input, size, camera, time / 2.0);
                let error = actual
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                maximum = maximum.max(error);
                samples += 1;
                if error > 2 {
                    let at = actual
                        .iter()
                        .zip(&expected)
                        .position(|(a, b)| a.abs_diff(*b) == error)
                        .unwrap()
                        / 4
                        * 4;
                    eprintln!(
                        "worst pixel {:?}: input={:?} actual={:?} expected={:?}",
                        [(at / 4) % size[0] as usize, (at / 4) / size[0] as usize],
                        &input[at..at + 4],
                        &actual[at..at + 4],
                        &expected[at..at + 4]
                    );
                }
                assert!(
                    error <= 2,
                    "size={size:?} camera={camera:?} time={time}: max RGBA error {error}"
                );
                if time == 1.0 {
                    if let Some(previous) = &midpoint {
                        assert_eq!(&actual, previous);
                    }
                    midpoint = Some(actual);
                }
            }
            // A valid, genuinely different GPU program must fail this oracle.
            let wrong = WgslEffectProgram::prepare(
                scan_band::material().unwrap().schema().clone(),
                scan_band::SOURCE.replace("1.0, 1.0, 0.0,", "0.0, 0.0, 1.0,"),
            )
            .unwrap();
            let mut wrong = panel(Some(wrong)).execution_session().unwrap();
            let mut wrong_surface =
                Surface::with_target_size(&device, &queue, Vec2::new(80.0, 60.0), size);
            wrong_surface.renderer.set_camera(&queue, camera);
            let actual =
                wrong_surface.render_on(&device, &queue, &mut wrong, wgpu::Color::TRANSPARENT);
            let expected = reference(&input, size, camera, 0.5);
            assert!(actual.iter().zip(expected).any(|(a, b)| a.abs_diff(b) > 32));
        }
    }
    eprintln!("ScanBand: {samples} images, maximum RGBA error {maximum}; 6 wrong-program controls rejected");
}
