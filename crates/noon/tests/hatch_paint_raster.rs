//! Public native authoring -> ordinary Fill animation -> retained GPU pixels.
//! The reference enumerates band intersections using authored inputs and physical
//! pixel corners, without reading effective paints, uniforms or production samples.
use noon::{AnimationOptions, Color, HatchPaint, HatchPaintOptions, Mobject, RateFunction, Scene};
use noon_core::{Vec2, VectorPath};
mod hatch_reference;
mod raster_support;
use hatch_reference::Pattern;
use raster_support::{Raster, SIZE};

#[derive(Clone, Copy, Debug)]
struct Pose {
    scale: [f64; 2],
    angle: f64,
    translation: [f64; 2],
}

impl Pose {
    fn new(transformed: bool) -> Self {
        if transformed {
            Self {
                scale: [-0.8, 1.2],
                angle: 0.3,
                translation: [0.2, -0.1],
            }
        } else {
            Self {
                scale: [1.0; 2],
                angle: 0.0,
                translation: [0.0; 2],
            }
        }
    }

    fn apply(self, object: &mut Mobject) {
        object.set_scale(self.scale[0], self.scale[1]).unwrap();
        object.set_rotation(self.angle).unwrap();
        object
            .set_translation(self.translation[0], self.translation[1])
            .unwrap();
    }

    fn local(self, world: [f64; 2]) -> [f64; 2] {
        let x = world[0] - self.translation[0];
        let y = world[1] - self.translation[1];
        [
            (self.angle.cos() * x + self.angle.sin() * y) / self.scale[0],
            (-self.angle.sin() * x + self.angle.cos() * y) / self.scale[1],
        ]
    }
}

#[derive(Clone, Copy)]
struct Viewport {
    width: u32,
    height: u32,
}

impl Viewport {
    fn world(self, x: f64, y: f64) -> [f64; 2] {
        let density = f64::from(self.height) / 8.0;
        [
            (x - f64::from(self.width) / 2.0) / density,
            (f64::from(self.height) / 2.0 - y) / density,
        ]
    }

    fn center(self, pose: Pose, x: u32, y: u32) -> [f64; 2] {
        pose.local(self.world(f64::from(x) + 0.5, f64::from(y) + 0.5))
    }

    fn corners(self, pose: Pose, x: u32, y: u32) -> [[f64; 2]; 4] {
        [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
            .map(|(dx, dy)| pose.local(self.world(f64::from(x) + dx, f64::from(y) + dy)))
    }
}

fn pattern(quality: u32, time: f64, cross: bool) -> Pattern {
    let (spacing, width) = match quality {
        0 => (0.6 - 0.25 * time, 0.09 + 0.03 * time),
        1 => (0.04 - 0.015 * time, 0.013 - 0.007 * time),
        _ => (0.0001, 0.000035),
    };
    Pattern {
        spacing,
        width,
        angle: 0.2 + 0.65 * time,
        phase: [-0.173 + 0.704 * time, 0.231 - 0.95 * time],
        cross,
    }
}

fn paint(pattern: Pattern, second: bool) -> HatchPaint {
    HatchPaint::new(
        if second {
            Color::rgb(0.0, 1.0, 0.0)
        } else {
            Color::rgba(1.0, 0.0, 0.0, 0.5)
        },
        Color::rgba(0.0, 0.0, 1.0, if second { 0.0 } else { 1.0 }),
        HatchPaintOptions {
            spacing: pattern.spacing,
            width: pattern.width,
            angle: pattern.angle,
            phase: pattern.phase,
            cross: pattern.cross,
        },
    )
    .unwrap()
}

fn shape(scene: &mut Scene, kind: u32) -> Mobject {
    match kind {
        0 => scene.rectangle(4.0, 2.0).unwrap(),
        1 => scene.circle(2.0).unwrap(),
        _ => scene
            .path(
                VectorPath::new()
                    .move_to(Vec2::new(-2.0, -1.0))
                    .line_to(Vec2::new(2.0, -1.0))
                    .line_to(Vec2::new(2.0, 1.0))
                    .line_to(Vec2::new(-2.0, 1.0))
                    .close(),
                Default::default(),
            )
            .unwrap(),
    }
}

fn interior(kind: u32, local: [f64; 2]) -> bool {
    if kind == 1 {
        local[0].hypot(local[1]) < 1.8
    } else {
        local[0].abs() < 1.8 && local[1].abs() < 0.8
    }
}

fn quantize(value: f64) -> u8 {
    ((value * 65535.0 + 0.5).floor() / 256.0).floor() as u8
}

fn expected(coverage: f64, time: f64, opacity: f64) -> [u8; 4] {
    // Literal endpoint vectors, interpolated in premultiplied encoded-sRGB.
    // The blue endpoint retains hidden blue RGB while its alpha reaches zero.
    let rgb = [
        0.5 * (1.0 - time) * coverage,
        time * coverage,
        (1.0 - time) * (1.0 - coverage),
    ];
    [
        quantize(rgb[0] * opacity),
        quantize(rgb[1] * opacity),
        quantize(rgb[2] * opacity),
        255,
    ]
}

#[test]
#[ignore = "requires an actual native GPU adapter; absence fails qualification"]
fn hatch_pixels_match_independent_band_filter_pose_time_and_seek_reference() {
    let viewport = Viewport {
        width: SIZE,
        height: SIZE,
    };
    let mut comparisons = 0;
    let mut max_error = 0;
    for kind in 0..3 {
        for transformed in [false, true] {
            for quality in 0..3 {
                for cross in [false, true] {
                    let pose = Pose::new(transformed);
                    let mut raster =
                        pollster::block_on(Raster::with_primary_dimensions(SIZE, SIZE));
                    let mut scene = Scene::new();
                    let mut source = shape(&mut scene, kind);
                    source.disable_stroke().unwrap();
                    source
                        .set_fill_hatch(paint(pattern(quality, 0.0, cross), false), Some(0.25))
                        .unwrap();
                    source.set_object_opacity(0.5).unwrap();
                    pose.apply(&mut source);
                    scene.add(&source).unwrap();
                    let mut target = source.target_editor().unwrap();
                    target
                        .set_fill_hatch(paint(pattern(quality, 1.0, cross), true), Some(0.75))
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
                    let mut endpoint = None;
                    for time in [0.0, 0.25, 0.5, 0.75, 1.0] {
                        scene
                            .live(&mut session)
                            .advance_segment_to(segment, time)
                            .unwrap();
                        let pixels = raster.capture(&session.take_renderer_publication());
                        assert_eq!(
                            raster.last_scene_path_batches > 0,
                            kind == 2,
                            "fixture kind={kind} did not exercise its declared geometry route"
                        );
                        let definition = pattern(quality, time, cross);
                        for x in (48..208).step_by(7) {
                            for y in (48..208).step_by(7) {
                                if !interior(kind, viewport.center(pose, x, y)) {
                                    continue;
                                }
                                let coverage = hatch_reference::coverage(
                                    viewport.corners(pose, x, y),
                                    definition,
                                );
                                let expected = expected(coverage, time, (0.25 + 0.5 * time) * 0.5);
                                let offset = ((y * SIZE + x) * 4) as usize;
                                let actual = &pixels[offset..offset + 4];
                                for (&actual_channel, expected_channel) in
                                    actual.iter().zip(expected)
                                {
                                    let error = actual_channel.abs_diff(expected_channel);
                                    max_error = max_error.max(error);
                                    assert!(
                                        error <= 1,
                                        "kind={kind} pose={transformed} quality={quality} cross={cross} time={time} ({x},{y}): {actual:?} != {expected:?}; coverage={coverage} pattern={definition:?}"
                                    );
                                }
                                comparisons += 1;
                            }
                        }
                        assert_eq!(pixels, raster.capture(&session.take_renderer_publication()));
                        assert_eq!(
                            raster.last_scene_upload_bytes, 0,
                            "clean hatch frame uploaded scene data"
                        );
                        assert_eq!(
                            raster.last_scene_repacked, 0,
                            "clean hatch frame repacked geometry"
                        );
                        if time == 0.25 {
                            quarter = Some(pixels.clone());
                        }
                        if time == 1.0 {
                            endpoint = Some(pixels);
                        }
                    }
                    session.seek(0.25).unwrap();
                    assert_eq!(
                        quarter.unwrap(),
                        raster.capture(&session.take_renderer_publication()),
                        "backward seek changed the same authored hatch sample"
                    );
                    session.seek(1.0).unwrap();
                    scene.live(&mut session).complete_segment(segment).unwrap();
                    let completed = raster.capture(&session.take_renderer_publication());
                    assert_eq!(
                        endpoint.unwrap(),
                        completed,
                        "completion changed the exact hatch endpoint"
                    );
                    assert_eq!(
                        completed,
                        raster.capture(&session.take_renderer_publication())
                    );
                    assert_eq!(raster.last_scene_upload_bytes, 0);
                }
            }
        }
    }
    assert!(comparisons > 20_000);
    println!(
        "hatch independent bands: 36 geometry/pose/filter/topology profiles, 5 times, {comparisons} RGBA probes, maximum byte error {max_error}; true path route; clean uploads zero; rewind and completion exact"
    );
}

fn static_rectangle(viewport: Viewport, width: f64, definition: Pattern) -> Vec<u8> {
    let mut raster = pollster::block_on(Raster::with_primary_dimensions(
        viewport.width,
        viewport.height,
    ));
    let mut scene = Scene::new();
    let mut source = scene.rectangle(width, 2.0).unwrap();
    source.disable_stroke().unwrap();
    source
        .set_fill_hatch(paint(definition, false), Some(0.7))
        .unwrap();
    source.set_object_opacity(0.6).unwrap();
    scene.add(&source).unwrap();
    let mut session = scene.execution_session().unwrap();
    let pixels = raster.capture(&session.take_renderer_publication());
    assert_eq!(pixels, raster.capture(&session.take_renderer_publication()));
    assert_eq!(raster.last_scene_upload_bytes, 0);
    pixels
}

#[test]
#[ignore = "requires an actual native GPU adapter; absence fails qualification"]
fn hatch_local_units_resolution_phase_period_and_duty_endpoints() {
    let base = Pattern {
        spacing: 0.5,
        width: 0.125,
        angle: 0.37,
        phase: [0.125, -0.375],
        cross: true,
    };
    let mut comparisons = 0;
    let mut max_error = 0;
    for viewport in [
        Viewport {
            width: 256,
            height: 256,
        },
        Viewport {
            width: 384,
            height: 192,
        },
    ] {
        let wide = static_rectangle(viewport, 4.0, base);
        let narrow = static_rectangle(viewport, 2.0, base);
        let wrapped = static_rectangle(
            viewport,
            4.0,
            Pattern {
                phase: [4_294_967_296.125, -4_294_967_296.375],
                ..base
            },
        );
        assert_eq!(
            wide, wrapped,
            "whole-period phase changed the rendered field"
        );
        let zero_phase = static_rectangle(
            viewport,
            4.0,
            Pattern {
                phase: [0.0; 2],
                ..base
            },
        );
        let tiny_phase = static_rectangle(
            viewport,
            4.0,
            Pattern {
                phase: [-1e-20, 0.0],
                ..base
            },
        );
        assert_eq!(
            zero_phase, tiny_phase,
            "period-rounding changed another axis or the finite pixel filter"
        );
        for (definition, pixels) in [
            (base, wide),
            (
                Pattern { width: 0.0, ..base },
                static_rectangle(viewport, 4.0, Pattern { width: 0.0, ..base }),
            ),
            (
                Pattern {
                    width: base.spacing,
                    ..base
                },
                static_rectangle(
                    viewport,
                    4.0,
                    Pattern {
                        width: base.spacing,
                        ..base
                    },
                ),
            ),
        ] {
            for x in (0..viewport.width).step_by(5) {
                for y in (0..viewport.height).step_by(5) {
                    let local = viewport.center(Pose::new(false), x, y);
                    if local[0].abs() >= 0.8 || local[1].abs() >= 0.8 {
                        continue;
                    }
                    let offset = ((y * viewport.width + x) * 4) as usize;
                    let actual = &pixels[offset..offset + 4];
                    if definition.width == base.width {
                        assert_eq!(
                            actual,
                            &narrow[offset..offset + 4],
                            "hatch spacing changed with geometry bounds at ({x},{y})"
                        );
                    }
                    let coverage = hatch_reference::coverage(
                        viewport.corners(Pose::new(false), x, y),
                        definition,
                    );
                    let expected = expected(coverage, 0.0, 0.7 * 0.6);
                    for (&a, e) in actual.iter().zip(expected) {
                        let error = a.abs_diff(e);
                        max_error = max_error.max(error);
                        assert!(
                            error <= 1,
                            "resolution={}x{} duty={} ({x},{y}): {actual:?} != {expected:?}",
                            viewport.width,
                            viewport.height,
                            definition.width / definition.spacing
                        );
                    }
                    comparisons += 1;
                }
            }
        }
    }
    assert!(comparisons > 400);
    println!(
        "hatch local-unit/resolution/phase/duty oracle: {comparisons} RGBA probes, maximum byte error {max_error}; integer periods, tiny negative phase and geometry-size invariance exact"
    );
}
