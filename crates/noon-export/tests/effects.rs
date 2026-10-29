//! Public semantic authoring -> shared camera/runtime -> native windowless
//! capture. References use literal source coverage and a full 2D convolution;
//! no engine projection, kernel, sampler, mask uniform or pass planner is used.
#![cfg(not(target_arch = "wasm32"))]

use std::{collections::BTreeMap, convert::Infallible};

use noon::{
    effects::{Blur, EffectUpdate, Pixels},
    integration::{ExportFrameOptions, ExportStop, FrameRate},
    AnimationOptions, Mobject, RateFunction, RustHostCallbackTable, Scene,
};
use noon_export::{capture_frames, Backends, CaptureOptions, SessionCapture};

const BACKGROUND: [u8; 4] = [24, 48, 64, 255];

#[cfg(feature = "wgsl-effects")]
#[test]
#[ignore = "requires native GPU; staged shader publication through the actual export host"]
fn shader_bearing_insertion_exports_the_same_exact_time_frames_as_cold_authoring() {
    let capture =
        |mut program: noon::LiveProgram<noon::example_scenes::shader_sweep::ShaderSweep>| {
            let mut frames = BTreeMap::new();
            capture_frames(
                &mut program,
                &mut RustHostCallbackTable::new(),
                ExportFrameOptions {
                    frame_rate: FrameRate::new(4, 1).unwrap(),
                    start_frame: 0,
                    stop: ExportStop::SourceEnd,
                    max_frames: 32,
                    max_transitions_per_sample: 64,
                    final_hold_seconds: 0.0,
                },
                options(320, 180),
                |frame| {
                    assert_eq!(
                        frame.observation.requested_time,
                        frame.frame.source_sample.index() as f64 / 4.0
                    );
                    frames.insert(frame.frame.source_sample.index(), frame.rgba.to_vec());
                    Ok::<_, Infallible>(())
                },
            )
            .unwrap();
            frames
        };
    let cold = capture(noon::example_scenes::shader_sweep::program().unwrap());
    let staged = capture(noon::example_scenes::shader_sweep::late_program().unwrap());
    let faded = capture(noon::example_scenes::shader_sweep::fade_in_program().unwrap());
    assert!(
        staged[&0] != cold[&0],
        "detached panels cannot appear early"
    );
    for index in 2..22 {
        assert_eq!(
            maximum_error(&staged[&index], &cold[&index]),
            0,
            "exact frame {index}/4"
        );
        assert_eq!(
            maximum_error(&faded[&(index + 4)], &cold[&index]),
            0,
            "FadeIn shifts the subsequent material animation by exactly one second"
        );
    }
    assert!(
        staged[&3].as_chunks::<4>().0.iter().any(|pixel| {
            i32::from(pixel[1]) > i32::from(pixel[0]) + 40
                && i32::from(pixel[2]) > i32::from(pixel[0]) + 30
        }),
        "the comparison must contain the shader treatment"
    );
    assert!(
        staged[&22] != cold[&21],
        "late retirement removes the shader"
    );
    // Interior of the scene-space panel at t=1: linear FadeIn is exactly halfway.
    // Literal authored coordinates/colors and straight-alpha composition provide
    // a separate witness against a fully-visible insertion followed by a wait.
    let stripes = 0.5 + 0.5 * (std::f64::consts::TAU * (-1.0 / 45.0) * 2.0).cos();
    let at = (90 * 320 + 227) * 4;
    for (channel, background) in [0.035_f64, 0.045, 0.09].into_iter().enumerate() {
        let expected = (255.0 * background).round() as u8;
        assert!(
            faded[&2][at + channel].abs_diff(expected) <= 2,
            "a newly prepared FadeIn starts invisible"
        );
    }
    for (channel, (low, high, background)) in
        [(0.09, 0.48, 0.035), (0.2, 0.94, 0.045), (0.42, 0.83, 0.09)]
            .into_iter()
            .enumerate()
    {
        let expected =
            (255.0 * (0.5 * (low + (high - low) * stripes) + 0.5 * background)).round() as u8;
        assert!(
            faded[&4][at + channel].abs_diff(expected) <= 2,
            "independent halfway FadeIn channel {channel}"
        );
    }
}

fn options(width: u32, height: u32) -> CaptureOptions {
    let mut options = CaptureOptions::new(width, height);
    options.backends = if cfg!(target_os = "macos") {
        Backends::METAL
    } else {
        Backends::VULKAN
    };
    options.background = std::array::from_fn(|index| f64::from(BACKGROUND[index]) / 255.0);
    options
}

fn over(pixel: &mut [u8], source: [f64; 4]) {
    for channel in 0..4 {
        pixel[channel] = (255.0 * source[channel] + f64::from(pixel[channel]) * (1.0 - source[3]))
            .round()
            .clamp(0.0, 255.0) as u8;
    }
}

fn rectangle(image: &mut [u8], size: [usize; 2], bounds: [f64; 4], color: [f64; 4]) {
    for y in 0..size[1] {
        for x in 0..size[0] {
            let coverage = ((x as f64 + 1.0).min(bounds[2]) - (x as f64).max(bounds[0]))
                .clamp(0.0, 1.0)
                * ((y as f64 + 1.0).min(bounds[3]) - (y as f64).max(bounds[1])).clamp(0.0, 1.0);
            if coverage > 0.0 {
                over(
                    &mut image[(y * size[0] + x) * 4..][..4],
                    color.map(|value| value * coverage),
                );
            }
        }
    }
}

fn gaussian(source: &[u8], size: [usize; 2], sigma: f64) -> Vec<u8> {
    let radius = (3.0 * sigma).ceil() as i32;
    let taps: Vec<_> = (-radius..=radius)
        .flat_map(|y| {
            (-radius..=radius).map(move |x| {
                (
                    x,
                    y,
                    (-f64::from(x * x + y * y) / (2.0 * sigma * sigma)).exp(),
                )
            })
        })
        .collect();
    let mass: f64 = taps.iter().map(|(_, _, weight)| weight).sum();
    let mut result = vec![0; source.len()];
    for y in 0..size[1] as i32 {
        for x in 0..size[0] as i32 {
            let mut value = [0.0; 4];
            for &(dx, dy, weight) in &taps {
                let (xx, yy) = (x + dx, y + dy);
                if xx >= 0 && yy >= 0 && xx < size[0] as i32 && yy < size[1] as i32 {
                    let at = (yy as usize * size[0] + xx as usize) * 4;
                    for channel in 0..4 {
                        value[channel] += f64::from(source[at + channel]) * weight / mass;
                    }
                }
            }
            for channel in 0..4 {
                result[(y as usize * size[0] + x as usize) * 4 + channel] =
                    value[channel].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    result
}

fn reference(width: u32, height: u32, time: f64, pixels: bool, shift: f64) -> Vec<u8> {
    // These are the authored camera endpoints and Linear interpolation, not
    // values read from the engine's effective camera or screen bounds.
    let density = f64::from(height) / (6.0 - 4.0 * time);
    let sigma = if pixels { 1.0 } else { 0.1 * density };
    let padding = (3.0 * sigma).ceil() as usize + 5;
    let size = [width as usize + 2 * padding, height as usize + 2 * padding];
    let bounds = |left: f64, bottom: f64, right: f64, top: f64| {
        [
            f64::from(width) * 0.5 + (left + shift - 0.4 * time) * density + padding as f64,
            f64::from(height) * 0.5 - (top + 0.4 * time) * density + padding as f64,
            f64::from(width) * 0.5 + (right + shift - 0.4 * time) * density + padding as f64,
            f64::from(height) * 0.5 - (bottom + 0.4 * time) * density + padding as f64,
        ]
    };
    let mut image = vec![0; size[0] * size[1] * 4];
    // RGBA(192,48,16,128), image opacity .5 and object opacity .25.
    let alpha = 128.0 / 255.0 * 0.125;
    rectangle(
        &mut image,
        size,
        bounds(-1.2, -0.6, 0.4, 0.6),
        [
            192.0 / 255.0 * alpha,
            48.0 / 255.0 * alpha,
            16.0 / 255.0 * alpha,
            alpha,
        ],
    );
    let mut source = gaussian(&image, size, 1.0);
    rectangle(
        &mut source,
        size,
        bounds(-0.2, -0.4, 1.0, 0.4),
        [0.0, 0.0, 0.375, 0.375],
    );
    let filtered = gaussian(&source, size, sigma);
    let mut output = BACKGROUND.repeat(width as usize * height as usize);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let at = ((y + padding) * size[0] + x + padding) * 4;
            over(
                &mut output[(y * width as usize + x) * 4..][..4],
                std::array::from_fn(|channel| f64::from(filtered[at + channel]) / 255.0),
            );
        }
    }
    output
}

fn fixture(pixels: bool, shift: f64) -> (Scene, Mobject, Mobject) {
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame().unwrap();
    camera.set_scale(0.75, 0.75).unwrap();
    let mut target = camera.target_editor().unwrap();
    target.set_scale(0.25, 0.25).unwrap();
    target.set_translation(0.4, -0.4).unwrap();
    let mut image_options = noon::ImageMobjectOptions::rgba8(1, 1, [192, 48, 16, 128]).unwrap();
    image_options.set_height(1.2).unwrap();
    image_options.set_opacity(0.5).unwrap();
    image_options.set_sampling(noon::RasterImageSampling::Nearest);
    let mut image = scene.image(image_options).unwrap();
    image.scale(4.0 / 3.0, 1.0).unwrap();
    image.set_translation(-0.4 + shift, 0.0).unwrap();
    image.set_object_opacity(0.25).unwrap();
    image
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(1.0))).unwrap(),
            "image-softness",
        )
        .unwrap();
    let mut shape = scene.rectangle(1.2, 0.8).unwrap();
    shape.disable_stroke().unwrap();
    shape.set_fill(0.0, 0.0, 1.0, 0.5).unwrap();
    shape.set_translation(0.4 + shift, 0.0).unwrap();
    shape.set_object_opacity(0.75).unwrap();
    let mut group = scene.family(&[(&image).into(), (&shape).into()]).unwrap();
    let update = if pixels {
        EffectUpdate::default().radius(Pixels(1.0))
    } else {
        EffectUpdate::default().radius(0.1)
    };
    group
        .add_effect(Blur::new(update).unwrap(), "group-softness")
        .unwrap();
    scene.add_many(&[(&group).into()]).unwrap();
    (scene, camera, target)
}

fn maximum_error(actual: &[u8], expected: &[u8]) -> u8 {
    assert_eq!(actual.len(), expected.len());
    actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap()
}

fn proof(name: &str, actual: &[u8], expected: &[u8], metadata: serde_json::Value) {
    let Some(directory) = std::env::var_os("NOON_EFFECT_EXPORT_PROOF_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join(format!("{name}-actual.rgba")), actual).unwrap();
    std::fs::write(directory.join(format!("{name}-reference.rgba")), expected).unwrap();
    std::fs::write(
        directory.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "requires native GPU; independent composed-effect camera/export oracle"]
fn native_composed_effects_match_literal_pixels_through_camera_units_and_offscreen_queries() {
    let mut samples = 0;
    let mut maximum = 0;
    for (width, height) in [(80, 60), (160, 120), (120, 60), (60, 120)] {
        for pixels in [false, true] {
            for offscreen in [false, true] {
                let shift = if offscreen {
                    // Keep the rightmost source edge half a physical pixel
                    // outside the viewport at each output resolution.
                    -(3.0 * f64::from(width) / f64::from(height) + 1.0 + 3.0 / f64::from(height))
                } else {
                    0.0
                };
                let (scene, camera, target) = fixture(pixels, shift);
                let mut session = scene.execution_session().unwrap();
                let segment = scene
                    .live(&mut session)
                    .declare_and_activate_transform_to(
                        &camera,
                        &target,
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    )
                    .unwrap();
                let mut capture = SessionCapture::new(&session, options(width, height)).unwrap();
                let mut seen = BTreeMap::new();
                let source_rows = session.frame().objects[1..].to_vec();
                for (index, time) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                    session.advance_segment_to(segment, time).unwrap();
                    assert_eq!(&session.frame().objects[1..], source_rows.as_slice());
                    let frame = capture.capture(&mut session).unwrap();
                    assert_eq!(frame.receipt.published_time, time);
                    let expected = reference(width, height, time, pixels, shift);
                    let error = maximum_error(frame.rgba, &expected);
                    maximum = maximum.max(error);
                    proof(
                        &format!(
                            "camera-{width}x{height}-pixels-{pixels}-outside-{offscreen}-{index}"
                        ),
                        frame.rgba,
                        &expected,
                        serde_json::json!({"width":width,"height":height,"time":time,"pixel_radius":pixels,"source_shift":shift,
                            "camera_height":6.0-4.0*time,"camera_center":[0.4*time,-0.4*time],"maximum_byte_error":error,
                            "oracle":"literal premultiplied image/vector coverage; independent full 2D Gaussian per stage; crop after both filters"}),
                    );
                    assert!(error <= 2, "{width}x{height} pixels={pixels} offscreen={offscreen} t={time}: max RGBA error {error}");
                    if index > 0 {
                        assert_eq!(
                            frame.receipt.work.geometry_instances_repacked, 1,
                            "only the camera's ordinary retained proxy may repack; source geometry must stay resident"
                        );
                    }
                    if offscreen && time == 0.0 {
                        assert!(
                            maximum_error(
                                &expected,
                                &BACKGROUND.repeat(width as usize * height as usize)
                            ) > 4,
                            "offscreen halo must distinguish incorrect source culling"
                        );
                    }
                    if !offscreen && time == 1.0 {
                        assert!(
                            maximum_error(
                                &expected,
                                &reference(width, height, time, !pixels, shift)
                            ) > 4,
                            "wrong radius units must fail the oracle"
                        );
                    }
                    seen.insert(index, frame.rgba.to_vec());
                    samples += 1;
                    let held = capture.capture(&mut session).unwrap();
                    assert_eq!(held.rgba, seen[&index]);
                    assert_eq!(held.receipt.work.geometry_instances_repacked, 0);
                    assert_eq!(held.receipt.work.retained_upload_bytes, 0);
                    samples += 1;
                }
                for (index, time) in [(1, 0.5), (0, 0.0), (2, 1.0)] {
                    session.seek(time).unwrap();
                    assert_eq!(capture.capture(&mut session).unwrap().rgba, seen[&index]);
                    samples += 1;
                }
                let mut fresh = SessionCapture::new(&session, options(width, height)).unwrap();
                assert_eq!(fresh.capture(&mut session).unwrap().rgba, seen[&2]);
                samples += 1;
                scene.live(&mut session).complete_segment(segment).unwrap();
                assert_eq!(capture.capture(&mut session).unwrap().rgba, seen[&2]);
                samples += 1;
                eprintln!("native effects {width}x{height} pixels={pixels} offscreen={offscreen}: exact seek/fresh/held/completion; max error {maximum}");
            }
        }
    }
    eprintln!("EXPORTED_EFFECTS_PASS profiles=16 images={samples} maximum_byte_error={maximum} independent_2d_convolution=true units_witness=true offscreen_halo_witness=true");
}

fn lattice(x: i32, y: i32) -> f64 {
    let mut hash = (x as u32).wrapping_mul(0x9e3779b9) ^ (y as u32).wrapping_mul(0x85ebca6b) ^ 7;
    hash = (hash ^ (hash >> 16)).wrapping_mul(0x7feb352d);
    hash = (hash ^ (hash >> 15)).wrapping_mul(0x846ca68b);
    f64::from((hash ^ (hash >> 16)) >> 8) / 16777216.0
}
fn coverage(x: f64, y: f64, progress: f64) -> f64 {
    if progress <= 0.0 {
        return 1.0;
    }
    if progress >= 1.0 {
        return 0.0;
    }
    let (x, y) = (x * 16.0, y * 16.0);
    let (bx, by) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (wx, wy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let mix = |a: f64, b: f64, t: f64| a * (1.0 - t) + b * t;
    let noise = mix(
        mix(lattice(bx, by), lattice(bx + 1, by), wx),
        mix(lattice(bx, by + 1), lattice(bx + 1, by + 1), wx),
        wy,
    );
    let value = ((progress - noise) / 0.2 + 0.5).clamp(0.0, 1.0);
    1.0 - value * value * (3.0 - 2.0 * value)
}
fn dissolve_progress(time: f64) -> f64 {
    if time < 0.5 || (3.0..3.5).contains(&time) || time >= 6.0 {
        0.0
    } else if time <= 1.5 {
        time - 0.5
    } else if time < 2.0 || (4.5..5.0).contains(&time) {
        1.0
    } else if time < 3.0 {
        3.0 - time
    } else if time <= 4.5 {
        time - 3.5
    } else {
        6.0 - time
    }
}

#[test]
#[ignore = "requires native GPU; high-level dissolve export/continuation oracle"]
fn native_dissolve_export_uses_the_shared_clock_membership_and_final_halo_mask() {
    let mut program = noon::example_scenes::dissolve::program().unwrap();
    let mut samples = 0;
    let mut maximum = 0;
    let summary = capture_frames(&mut program, &mut RustHostCallbackTable::new(), ExportFrameOptions {
        frame_rate: FrameRate::new(4, 1).unwrap(), start_frame: 0, stop: ExportStop::SourceEnd,
        // Output intervals are half-open. Request a final quarter-second hold
        // so the terminal source publication is also an emitted video sample.
        max_frames: 32, max_transitions_per_sample: 64, final_hold_seconds: 0.25,
    }, CaptureOptions::new(640, 360), |frame| {
        let time = frame.frame.source_sample.authored_time();
        assert_eq!(time, samples as f64 / 4.0);
        let progress = dissolve_progress(time);
        let mut halo_witness = 0;
        let mut actual_region = Vec::new();
        let mut expected_region = Vec::new();
        for y in 140..220 {
            for x in 344..456 {
                let source = frame.rgba[(y * 640 + x - 180) * 4];
                if !(374..446).contains(&x) || !(162..198).contains(&y) { halo_witness = halo_witness.max(source); }
                let mask = coverage((x as f64 + 0.5 - 374.0) / 72.0, (y as f64 + 0.5 - 162.0) / 36.0, progress);
                let expected = (f64::from(source) * mask).round() as u8;
                actual_region.extend_from_slice(&frame.rgba[(y * 640 + x) * 4..][..4]);
                expected_region.extend_from_slice(&[expected, expected, expected, 255]);
                assert_eq!(frame.rgba[(y * 640 + x) * 4 + 3], 255);
                for channel in 0..3 {
                    let error = frame.rgba[(y * 640 + x) * 4 + channel].abs_diff(expected);
                    maximum = maximum.max(error);
                    assert!(error <= 2, "exported dissolve t={time} ({x},{y}) channel={channel}: byte error {error}");
                    if progress == 0.0 || progress == 1.0 { assert_eq!(error, 0, "exact visibility endpoint includes the halo"); }
                }
            }
        }
        assert!(halo_witness > 3);
        proof(&format!("dissolve-{}", frame.frame.pts), &actual_region, &expected_region,
            serde_json::json!({"width":112,"height":80,"time":time,"progress":progress,"held":frame.frame.held,
                "source_region":[344,140,456,220],"seed":7,"softness":0.2,"halo_witness":halo_witness,
                "oracle":"independent seeded field; ordinary left composition supplies qualified source coverage; filter/composition proof"}));
        samples += 1;
        Ok::<_, Infallible>(())
    }).unwrap();
    assert_eq!(samples, 27);
    assert_eq!(summary.readbacks, samples);
    assert!(summary.rendered_publications >= samples);
    assert_ne!(summary.adapter.backend, wgpu::Backend::Noop);
    eprintln!("EXPORTED_DISSOLVE_PASS frames={samples} maximum_byte_error={maximum} adapter={:?} canonical_clock=true endpoints_exact=true", summary.adapter);
}
