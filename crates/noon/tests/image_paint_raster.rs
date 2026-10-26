//! Public authoring -> shared execution -> actual retained GPU pixels.
//! This oracle uses explicit six-texel data, physical fit rectangles and direct
//! separable reconstruction weights. It reads no production mapping/sampler,
//! shader uniforms, retained paint values or runtime interpolation results.
use noon::{
    AnimationOptions, ImageAddressMode, ImageFit, ImageMapping, ImageUvTransform, Mobject,
    RasterImageSampling, RateFunction, Scene, Vec2, VectorPath,
};
mod raster_support;
use raster_support::{Raster, SIZE};

const PIXELS: [u8; 24] = [
    240, 16, 24, 255, 16, 216, 48, 128, 20, 50, 255, 0, 16, 32, 230, 255, 230, 180, 20, 64, 255,
    255, 255, 255,
];
const BACKGROUND: [f64; 3] = [20.0 / 255.0, 40.0 / 255.0, 60.0 / 255.0];

#[derive(Clone, Copy, Debug)]
struct Profile {
    shape: u8,
    fit: ImageFit,
    address: ImageAddressMode,
    sampling: RasterImageSampling,
    alignment: [f64; 2],
    translation: [f64; 2],
    scale: [f64; 2],
    rotation: f64,
    posed: bool,
    opacity: f64,
}

impl Profile {
    fn mapping(self) -> ImageMapping {
        ImageMapping::new(
            self.fit,
            self.alignment,
            self.address,
            ImageUvTransform::new(self.translation, self.scale, self.rotation).unwrap(),
        )
        .unwrap()
    }
    fn size(self) -> [f64; 2] {
        if self.shape == 2 {
            [3.0, 3.0]
        } else {
            [4.0, 2.0]
        }
    }
}

fn local_point(profile: Profile, x: u32, y: u32) -> [f64; 2] {
    let mut point = [
        (f64::from(x) + 0.5 - 128.0) / 32.0,
        (127.5 - f64::from(y)) / 32.0,
    ];
    if profile.posed {
        point[0] -= 0.23;
        point[1] += 0.17;
        let (sin, cos) = 0.41_f64.sin_cos();
        point = [
            (cos * point[0] + sin * point[1]) / -0.85,
            (-sin * point[0] + cos * point[1]) / 1.12,
        ];
    }
    point
}

fn texel(x: i32, y: i32, address: ImageAddressMode) -> [f64; 4] {
    let mut at = [x, y];
    for (axis, limit) in [3, 2].into_iter().enumerate() {
        match address {
            ImageAddressMode::Transparent if at[axis] < 0 || at[axis] >= limit => return [0.0; 4],
            ImageAddressMode::Transparent => {}
            ImageAddressMode::Clamp => at[axis] = at[axis].clamp(0, limit - 1),
            ImageAddressMode::Repeat => at[axis] = at[axis].rem_euclid(limit),
        }
    }
    let i = ((at[1] * 3 + at[0]) * 4) as usize;
    let alpha = f64::from(PIXELS[i + 3]) / 255.0;
    [
        f64::from(PIXELS[i]) / 255.0 * alpha,
        f64::from(PIXELS[i + 1]) / 255.0 * alpha,
        f64::from(PIXELS[i + 2]) / 255.0 * alpha,
        alpha,
    ]
}

fn reconstruct(profile: Profile, mut uv: [f64; 2]) -> [f64; 4] {
    for value in &mut uv {
        match profile.address {
            ImageAddressMode::Clamp => *value = value.clamp(0.0, 1.0),
            ImageAddressMode::Repeat => *value -= value.floor(),
            ImageAddressMode::Transparent => {}
        }
    }
    if profile.sampling == RasterImageSampling::Nearest {
        return texel(
            (uv[0] * 3.0).floor() as i32,
            (uv[1] * 2.0).floor() as i32,
            profile.address,
        );
    }
    let position = [uv[0] * 3.0 - 0.5, uv[1] * 2.0 - 0.5];
    let origin = position.map(|v| v.floor() as i32);
    let fractions = [
        position[0] - f64::from(origin[0]),
        position[1] - f64::from(origin[1]),
    ];
    // Explicit reconstruction weights in increasing tap order, independently
    // expanded from the coefficient -1 cubic polynomial.
    let weights = fractions.map(|t| {
        if profile.sampling == RasterImageSampling::Linear {
            [0.0, 1.0 - t, t, 0.0]
        } else {
            [
                -t + 2.0 * t * t - t * t * t,
                1.0 - 2.0 * t * t + t * t * t,
                t + t * t - t * t * t,
                -t * t + t * t * t,
            ]
        }
    });
    let mut result = [0.0; 4];
    for (y, wy) in weights[1].into_iter().enumerate() {
        for (x, wx) in weights[0].into_iter().enumerate() {
            let sample = texel(
                origin[0] + x as i32 - 1,
                origin[1] + y as i32 - 1,
                profile.address,
            );
            for c in 0..4 {
                result[c] += wx * wy * sample[c];
            }
        }
    }
    result[3] = result[3].clamp(0.0, 1.0);
    for c in 0..3 {
        result[c] = result[c].clamp(0.0, result[3]);
    }
    result
}

fn reference(profile: Profile, x: u32, y: u32) -> Option<[u8; 4]> {
    let point = local_point(profile, x, y);
    let size = profile.size();
    let edge_distance = if profile.shape == 2 {
        1.5 - point[0].hypot(point[1])
    } else {
        (size[0] / 2.0 - point[0].abs()).min(size[1] / 2.0 - point[1].abs())
    };
    // Source geometry antialiasing is qualified elsewhere. Probe both its
    // interior and exact exterior, away from the raster edge transition.
    if edge_distance.abs() < 0.08 {
        return None;
    }
    let mut color = [0.0; 4];
    if edge_distance > 0.0 {
        let normalized = [point[0] / size[0] + 0.5, point[1] / size[1] + 0.5];
        let delta = [
            normalized[0] - 0.5 - profile.translation[0],
            normalized[1] - 0.5 - profile.translation[1],
        ];
        let (sin, cos) = profile.rotation.sin_cos();
        let mapped = [
            (cos * delta[0] + sin * delta[1]) / profile.scale[0] + 0.5,
            (-sin * delta[0] + cos * delta[1]) / profile.scale[1] + 0.5,
        ];
        // Fit in physical local units first, then convert to image coordinates.
        let sx = size[0] / 3.0;
        let sy = size[1] / 2.0;
        let fitted = match profile.fit {
            ImageFit::Stretch => size,
            ImageFit::Cover => [3.0 * sx.max(sy), 2.0 * sx.max(sy)],
            ImageFit::Contain => [3.0 * sx.min(sy), 2.0 * sx.min(sy)],
        };
        let lower = [
            (size[0] - fitted[0]) * profile.alignment[0],
            (size[1] - fitted[1]) * profile.alignment[1],
        ];
        let uv = [
            (mapped[0] * size[0] - lower[0]) / fitted[0],
            1.0 - (mapped[1] * size[1] - lower[1]) / fitted[1],
        ];
        if profile.fit != ImageFit::Contain || uv.into_iter().all(|v| (0.0..=1.0).contains(&v)) {
            color = reconstruct(profile, uv);
        }
    }
    let mut rgba = [0_u8; 4];
    for c in 0..3 {
        let linear =
            color[c] * profile.opacity + BACKGROUND[c] * (1.0 - color[3] * profile.opacity);
        rgba[c] = (linear * 255.0).round() as u8;
    }
    rgba[3] = 255;
    Some(rgba)
}

fn check_pixels(pixels: &[u8], profile: Profile) -> usize {
    let mut count = 0;
    for y in (13..244).step_by(9) {
        for x in (17..244).step_by(7) {
            let Some(expected) = reference(profile, x, y) else {
                continue;
            };
            let offset = ((y * SIZE + x) * 4) as usize;
            let actual = &pixels[offset..offset + 4];
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                "{profile:?} ({x},{y}): {actual:?} != {expected:?}"
            );
            count += 1;
        }
    }
    count
}

fn scene(profile: Profile) -> (Scene, Mobject, Mobject) {
    let mut scene = Scene::new();
    let mut background = scene.square(8.0).unwrap();
    background.disable_stroke().unwrap();
    background
        .set_fill(BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 1.0)
        .unwrap();
    scene.add(&background).unwrap();
    let image = scene.image_rgba8(3, 2, PIXELS.to_vec()).unwrap();
    let mut shape = match profile.shape {
        0 => scene.rectangle(4.0, 2.0).unwrap(),
        1 => scene
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
        _ => scene.circle(1.5).unwrap(),
    };
    shape.disable_stroke().unwrap();
    shape
        .set_fill_image(
            image
                .image_paint(profile.mapping(), Some(profile.sampling))
                .unwrap(),
            Some(profile.opacity / 0.75),
        )
        .unwrap();
    shape.set_object_opacity(0.75).unwrap();
    if profile.posed {
        shape.set_translation(0.23, -0.17).unwrap();
        shape.rotate(0.41).unwrap();
        shape.scale(-0.85, 1.12).unwrap();
    }
    scene.add(&shape).unwrap();
    (scene, image, shape)
}

#[test]
#[ignore = "requires a native GPU adapter; explicitly selected in raster CI"]
fn image_paint_fit_filter_address_pose_and_alpha_match_independent_pixels() {
    let mut raster = pollster::block_on(Raster::with_primary_dimensions(SIZE, SIZE));
    let mut profiles = 0;
    let mut samples = 0;
    for shape in 0..3 {
        for fit in [ImageFit::Cover, ImageFit::Contain, ImageFit::Stretch] {
            for address in [
                ImageAddressMode::Clamp,
                ImageAddressMode::Repeat,
                ImageAddressMode::Transparent,
            ] {
                for sampling in [
                    RasterImageSampling::Nearest,
                    RasterImageSampling::Linear,
                    RasterImageSampling::Bicubic,
                ] {
                    for posed in [false, true] {
                        let profile = Profile {
                            shape,
                            fit,
                            address,
                            sampling,
                            posed,
                            alignment: if posed { [0.25, 0.75] } else { [0.5; 2] },
                            translation: if posed { [0.28, -0.19] } else { [0.0; 2] },
                            scale: if posed { [-0.6, 0.75] } else { [1.0; 2] },
                            rotation: if posed { 0.37 } else { 0.0 },
                            opacity: if posed { 0.3 } else { 0.6 },
                        };
                        raster.reset_scene();
                        let (scene, _image, _shape) = scene(profile);
                        let mut session = scene.execution_session().unwrap();
                        let pixels = raster.capture(&session.take_renderer_publication());
                        assert_eq!(
                            raster.last_scene_path_batches > 0,
                            shape == 1,
                            "the explicit vector fixture must reach the path draw route"
                        );
                        samples += check_pixels(&pixels, profile);
                        assert_eq!(pixels, raster.capture(&session.take_renderer_publication()));
                        assert_eq!(
                            raster.last_scene_upload_bytes, 0,
                            "clean image-fill frame uploads"
                        );
                        profiles += 1;
                    }
                }
            }
        }
    }
    assert_eq!(profiles, 162);
    eprintln!(
        "image paint: {profiles} profiles, {samples} independent RGBA probes, tolerance 1 byte"
    );
}

#[test]
#[ignore = "requires a native GPU adapter; explicitly selected in raster CI"]
fn image_mapping_animation_seek_and_fill_removal_preserve_pixels() {
    let mut raster = pollster::block_on(Raster::with_primary_dimensions(SIZE, SIZE));
    let base = Profile {
        shape: 1,
        fit: ImageFit::Stretch,
        address: ImageAddressMode::Repeat,
        sampling: RasterImageSampling::Linear,
        alignment: [0.5; 2],
        translation: [-0.1, 0.1],
        scale: [1.0; 2],
        rotation: 0.0,
        posed: false,
        opacity: 0.3,
    };
    let (scene, image, shape) = scene(base);
    let mut target = shape.target_editor().unwrap();
    let final_profile = Profile {
        translation: [0.4, -0.2],
        scale: [0.5, 0.8],
        rotation: std::f64::consts::FRAC_PI_2,
        opacity: 0.675,
        ..base
    };
    target
        .set_fill_image(
            image
                .image_paint(final_profile.mapping(), Some(base.sampling))
                .unwrap(),
            Some(0.9),
        )
        .unwrap();
    let animation = scene
        .declare_transform_to(
            &shape,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    let expected_at = |t: f64| Profile {
        translation: [-0.1 + 0.5 * t, 0.1 - 0.3 * t],
        scale: [1.0 - 0.5 * t, 1.0 - 0.2 * t],
        rotation: std::f64::consts::FRAC_PI_2 * t,
        opacity: 0.3 + 0.375 * t,
        ..base
    };
    for time in [0.0, 0.25, 0.5, 0.75, 1.0] {
        scene
            .live(&mut session)
            .advance_segment_to(segment, time)
            .unwrap();
        check_pixels(
            &raster.capture(&session.take_renderer_publication()),
            expected_at(time),
        );
    }
    scene.live(&mut session).complete_segment(segment).unwrap();
    for time in [0.75, 0.25, 1.0, 0.0] {
        session.seek(time).unwrap();
        check_pixels(
            &raster.capture(&session.take_renderer_publication()),
            expected_at(time),
        );
    }
    // Live authored edits apply at the completed endpoint. Earlier timeline
    // positions intentionally replay their historical captured paint values.
    session.seek(1.0).unwrap();
    scene
        .live(&mut session)
        .set_fill(&shape, 1.0, 0.0, 0.0, 1.0)
        .unwrap();
    assert!(session
        .frame()
        .objects
        .iter()
        .all(|row| row.fill_image.is_none()));
    let pixels = raster.capture(&session.take_renderer_publication());
    let center = ((128 * SIZE + 128) * 4) as usize;
    let expected = [196, 10, 15, 255]; // Red at final scope opacity 0.75 over background.
    let actual = &pixels[center..center + 4];
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
        "solid replacement: {actual:?} != {expected:?}"
    );
    assert_eq!(pixels, raster.capture(&session.take_renderer_publication()));
    assert_eq!(raster.last_scene_upload_bytes, 0);
}
