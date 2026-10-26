use super::*;
use std::sync::Arc;

fn mapping(fit: ImageFit, alignment: [f64; 2], address: ImageAddressMode) -> ImageMapping {
    ImageMapping::new(fit, alignment, address, ImageUvTransform::default()).unwrap()
}

fn near<const N: usize>(actual: [f64; N], expected: [f64; N]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() <= 2e-14, "{actual} != {expected}");
    }
}

#[test]
fn fit_and_alignment_have_literal_asymmetric_witnesses() {
    let cover = mapping(ImageFit::Cover, [0.5; 2], ImageAddressMode::Clamp);
    // A 4:1 image covers a square: only its middle quarter is visible.
    near(
        cover
            .image_uv([0.0, 0.25], [2.0; 2], [4, 1])
            .unwrap()
            .unwrap(),
        [0.375, 0.75],
    );
    near(
        cover
            .image_uv([1.0, 0.25], [2.0; 2], [4, 1])
            .unwrap()
            .unwrap(),
        [0.625, 0.75],
    );
    let left = mapping(ImageFit::Cover, [0.0; 2], ImageAddressMode::Clamp);
    near(
        left.image_uv([1.0, 0.25], [2.0; 2], [4, 1])
            .unwrap()
            .unwrap(),
        [0.25, 0.75],
    );
    let right = mapping(ImageFit::Cover, [1.0; 2], ImageAddressMode::Clamp);
    near(
        right
            .image_uv([0.0, 0.25], [2.0; 2], [4, 1])
            .unwrap()
            .unwrap(),
        [0.75, 0.75],
    );

    let contain = mapping(ImageFit::Contain, [0.5; 2], ImageAddressMode::Repeat);
    assert_eq!(
        contain.image_uv([0.5, 0.25], [2.0; 2], [4, 1]).unwrap(),
        None
    );
    near(
        contain
            .image_uv([0.25, 0.5], [2.0; 2], [4, 1])
            .unwrap()
            .unwrap(),
        [0.25, 0.5],
    );
    let bottom = mapping(ImageFit::Contain, [0.0; 2], ImageAddressMode::Clamp);
    near(
        bottom
            .image_uv([0.25, 0.125], [2.0; 2], [4, 1])
            .unwrap()
            .unwrap(),
        [0.25, 0.5],
    );
    assert_eq!(
        bottom.image_uv([0.25, 0.5], [2.0; 2], [4, 1]).unwrap(),
        None
    );
    // A tall image exercises the opposite fit branch and top-first row order.
    near(
        cover
            .image_uv([0.25, 0.0], [2.0; 2], [1, 4])
            .unwrap()
            .unwrap(),
        [0.25, 0.625],
    );
    near(
        cover
            .image_uv([0.25, 1.0], [2.0; 2], [1, 4])
            .unwrap()
            .unwrap(),
        [0.25, 0.375],
    );
    let stretch = mapping(ImageFit::Stretch, [0.0; 2], ImageAddressMode::Clamp);
    near(
        stretch
            .image_uv([0.25, 0.75], [3.0, 9.0], [7, 2])
            .unwrap()
            .unwrap(),
        [0.25, 0.25],
    );
}

#[test]
fn inverse_uv_pose_preserves_rotation_order_and_reflection() {
    let transform =
        ImageUvTransform::new([0.125, -0.25], [-2.0, 0.5], std::f64::consts::FRAC_PI_2).unwrap();
    let paint = ImageMapping::new(
        ImageFit::Stretch,
        [0.5; 2],
        ImageAddressMode::Clamp,
        transform,
    )
    .unwrap();
    // Original q=(.75,.25). Scale about center -> (-.5,-.125),
    // rotate -> (.125,-.5), then translate center -> (.75,-.25).
    near(
        paint
            .image_uv([0.75, -0.25], [3.0, 7.0], [2, 3])
            .unwrap()
            .unwrap(),
        [0.75, 0.75],
    );
    let unchanged = paint;
    for angle in [-2.7, -0.3, 0.0, 0.2, 1.9] {
        let pose = ImageUvTransform::new([0.21, -0.13], [-1.3, 0.7], angle).unwrap();
        let map =
            ImageMapping::new(ImageFit::Stretch, [0.5; 2], ImageAddressMode::Clamp, pose).unwrap();
        for q in [[0.1, 0.8], [0.5; 2], [1.2, -0.7]] {
            // Independent forward placement; the sampler must invert it.
            let x = (q[0] - 0.5) * -1.3;
            let y = (q[1] - 0.5) * 0.7;
            let point = [
                0.71 + angle.cos() * x - angle.sin() * y,
                0.37 + angle.sin() * x + angle.cos() * y,
            ];
            near(
                map.image_uv(point, [2.0, 4.0], [3, 9]).unwrap().unwrap(),
                [q[0], 1.0 - q[1]],
            );
        }
    }
    assert_eq!(paint, unchanged);
}

#[test]
fn nearest_and_linear_filter_premultiplied_top_first_pixels() {
    let pixels = RasterImageResource::from_rgba8(
        2,
        2,
        [
            255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 128, 255, 255, 255, 64,
        ]
        .to_vec(),
    )
    .unwrap();
    let paint = mapping(ImageFit::Stretch, [0.5; 2], ImageAddressMode::Clamp);
    near(
        paint
            .sample_premultiplied(
                &pixels,
                RasterImageSampling::Nearest,
                [1.0; 2],
                [0.25, 0.75],
            )
            .unwrap(),
        [1.0, 0.0, 0.0, 1.0],
    );
    near(
        paint
            .sample_premultiplied(
                &pixels,
                RasterImageSampling::Nearest,
                [1.0; 2],
                [0.75, 0.75],
            )
            .unwrap(),
        [0.0; 4],
    );
    near(
        paint
            .sample_premultiplied(&pixels, RasterImageSampling::Linear, [1.0; 2], [0.5; 2])
            .unwrap(),
        [
            319.0 / 1020.0,
            64.0 / 1020.0,
            192.0 / 1020.0,
            447.0 / 1020.0,
        ],
    );
    // Hidden green from alpha-zero top-right contributes exactly zero.
    let color = paint
        .sample_premultiplied(&pixels, RasterImageSampling::Linear, [1.0; 2], [0.5, 0.75])
        .unwrap();
    near(color, [0.5, 0.0, 0.0, 0.5]);
    assert_ne!(
        color,
        [0.25, 0.25, 0.0, 0.5],
        "straight-channel interpolation is not the contract"
    );
}

#[test]
fn address_modes_apply_to_each_tap_and_repeat_across_negative_seams() {
    let pixels =
        RasterImageResource::from_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
    for (address, expected) in [
        (ImageAddressMode::Clamp, [1.0, 0.0, 0.0, 1.0]),
        (ImageAddressMode::Transparent, [0.5, 0.0, 0.0, 0.5]),
        (ImageAddressMode::Repeat, [0.5, 0.0, 0.5, 1.0]),
    ] {
        let paint = mapping(ImageFit::Stretch, [0.5; 2], address);
        near(
            paint
                .sample_premultiplied(&pixels, RasterImageSampling::Linear, [1.0; 2], [0.0, 0.5])
                .unwrap(),
            expected,
        );
    }
    let repeat = mapping(ImageFit::Stretch, [0.5; 2], ImageAddressMode::Repeat);
    for x in [-3.0, -1.0, 0.0, 1.0, 4.0] {
        near(
            repeat
                .sample_premultiplied(&pixels, RasterImageSampling::Linear, [1.0; 2], [x, 0.5])
                .unwrap(),
            [0.5, 0.0, 0.5, 1.0],
        );
    }
    for x in [-0.25, 0.75, 2.75] {
        near(
            repeat
                .sample_premultiplied(&pixels, RasterImageSampling::Nearest, [1.0; 2], [x, 0.5])
                .unwrap(),
            [0.0, 0.0, 1.0, 1.0],
        );
    }
    let clear = mapping(ImageFit::Stretch, [0.5; 2], ImageAddressMode::Transparent);
    assert_eq!(
        clear
            .sample_premultiplied(
                &pixels,
                RasterImageSampling::Bicubic,
                [1.0; 2],
                [f64::MAX, 0.5]
            )
            .unwrap(),
        [0.0; 4]
    );
}

#[test]
fn bicubic_has_an_independent_rational_kernel_and_premultiplied_clamp() {
    let rgba: Vec<u8> = (0..16u8)
        .flat_map(|i| [i * 13, i * 7, 250 - i * 11, i * 17])
        .collect();
    let pixels = RasterImageResource::from_rgba8(4, 4, rgba.clone()).unwrap();
    let paint = mapping(ImageFit::Stretch, [0.5; 2], ImageAddressMode::Clamp);
    // At texel coordinate 1.25 both axes, cubic-convolution weights are these
    // exact rational numbers. This does not call the implementation's kernel.
    let weights = [-9.0 / 64.0, 57.0 / 64.0, 19.0 / 64.0, -3.0 / 64.0];
    let mut expected = [0.0; 4];
    for (index, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
        let weight = weights[index % 4] * weights[index / 4];
        let alpha = f64::from(pixel[3]) / 255.0;
        for channel in 0..3 {
            expected[channel] += f64::from(pixel[channel]) / 255.0 * alpha * weight;
        }
        expected[3] += alpha * weight;
    }
    expected[3] = expected[3].clamp(0.0, 1.0);
    for channel in 0..3 {
        expected[channel] = expected[channel].clamp(0.0, expected[3]);
    }
    near(
        paint
            .sample_premultiplied(
                &pixels,
                RasterImageSampling::Bicubic,
                [1.0; 2],
                [7.0 / 16.0, 9.0 / 16.0],
            )
            .unwrap(),
        expected,
    );
    // Cubic negative lobes cannot produce RGB outside premultiplied bounds.
    let edge = RasterImageResource::from_rgba8(
        4,
        1,
        vec![255, 0, 255, 0, 255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 0],
    )
    .unwrap();
    for index in 0..257 {
        let color = paint
            .sample_premultiplied(
                &edge,
                RasterImageSampling::Bicubic,
                [1.0; 2],
                [f64::from(index) / 256.0, 0.5],
            )
            .unwrap();
        assert!((0.0..=1.0).contains(&color[3]));
        assert!(color[..3].iter().all(|v| (0.0..=color[3]).contains(v)));
        assert_eq!(color[1..3], [0.0, 0.0]);
    }
}

#[test]
fn linear_matches_exhaustive_tent_integral_over_transparent_field() {
    let rgba: Vec<u8> = (0..12u8)
        .flat_map(|i| [i * 17, 251 - i * 13, i * 3, i * 23])
        .collect();
    let pixels = RasterImageResource::from_rgba8(4, 3, rgba.clone()).unwrap();
    let paint = mapping(ImageFit::Stretch, [0.5; 2], ImageAddressMode::Transparent);
    let mut samples = 0;
    for row in -3..20 {
        for col in -3..25 {
            let uv = [f64::from(col) / 19.0, f64::from(row) / 15.0];
            let mut expected = [0.0; 4];
            // Sum all source pixels with the separable tent reconstruction,
            // independent of the implementation's floor/base/four-tap branch.
            for (index, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
                let x = index % 4;
                let y = index / 4;
                let weight = (1.0 - (uv[0] * 4.0 - (x as f64 + 0.5)).abs()).max(0.0)
                    * (1.0 - (uv[1] * 3.0 - (y as f64 + 0.5)).abs()).max(0.0);
                let alpha = f64::from(pixel[3]) / 255.0;
                for channel in 0..3 {
                    expected[channel] += weight * alpha * f64::from(pixel[channel]) / 255.0;
                }
                expected[3] += weight * alpha;
            }
            near(
                paint
                    .sample_premultiplied(
                        &pixels,
                        RasterImageSampling::Linear,
                        [1.0; 2],
                        [uv[0], 1.0 - uv[1]],
                    )
                    .unwrap(),
                expected,
            );
            samples += 1;
        }
    }
    assert_eq!(samples, 644);
}

#[test]
fn interpolation_preserves_exact_endpoints_and_rejects_the_whole_singular_interval() {
    let from = ImageMapping::new(
        ImageFit::Cover,
        [0.0, 0.75],
        ImageAddressMode::Repeat,
        ImageUvTransform::new([0.0, 0.25], [-1.0, 0.5], -0.4).unwrap(),
    )
    .unwrap();
    let to = ImageMapping::new(
        ImageFit::Cover,
        [1.0, 0.25],
        ImageAddressMode::Repeat,
        ImageUvTransform::new([0.5, -0.25], [-3.0, 1.5], 0.8).unwrap(),
    )
    .unwrap();
    assert_eq!(from.interpolate(to, 0.0).unwrap(), from);
    assert_eq!(from.interpolate(to, 1.0).unwrap(), to);
    let middle = from.interpolate(to, 0.5).unwrap();
    near(middle.alignment(), [0.5; 2]);
    near(middle.transform().translation(), [0.25, 0.0]);
    near(middle.transform().scale(), [-2.0, 1.0]);
    near([middle.transform().rotation()], [0.2]);
    let positive = ImageMapping::new(
        ImageFit::Cover,
        to.alignment(),
        to.address(),
        ImageUvTransform::default(),
    )
    .unwrap();
    for time in [0.0, 0.1, 0.5, 1.0] {
        assert_eq!(
            from.interpolate(positive, time),
            Err(ImagePaintError::SingularInterpolation)
        );
    }
    for time in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert_eq!(
            from.interpolate(to, time),
            Err(ImagePaintError::InvalidProgress)
        );
    }
    for different in [
        mapping(ImageFit::Contain, [0.5; 2], ImageAddressMode::Repeat),
        mapping(ImageFit::Cover, [0.5; 2], ImageAddressMode::Transparent),
    ] {
        assert_eq!(
            from.interpolate(different, 0.0),
            Err(ImagePaintError::DifferentMappingTopology)
        );
    }
    // Subnormal equal scales must stay nonzero at every sampled progress. The
    // naive weighted-sum lerp rounds each half of the smallest value to zero.
    let tiny = ImageMapping::new(
        ImageFit::Cover,
        [0.5; 2],
        ImageAddressMode::Clamp,
        ImageUvTransform::new([0.0; 2], [f64::from_bits(1), -f64::from_bits(1)], 0.0).unwrap(),
    )
    .unwrap();
    for alpha in [0.1, 0.25, 0.5, 0.75, 0.9] {
        assert_eq!(tiny.interpolate(tiny, alpha).unwrap(), tiny);
    }
    let twice = ImageMapping::new(
        ImageFit::Cover,
        [0.5; 2],
        ImageAddressMode::Clamp,
        ImageUvTransform::new([0.0; 2], [f64::from_bits(2), -f64::from_bits(2)], 0.0).unwrap(),
    )
    .unwrap();
    assert_eq!(
        tiny.interpolate(twice, 0.5).unwrap().transform().scale(),
        [f64::from_bits(2), -f64::from_bits(2)]
    );
}

#[test]
fn finite_extreme_fit_ratios_and_cover_samples_avoid_intermediate_overflow() {
    let cover = mapping(ImageFit::Cover, [1.0; 2], ImageAddressMode::Clamp);
    let uv = cover
        .image_uv([1e308, 0.5], [1.0, 1e308], [1, 1])
        .unwrap()
        .unwrap();
    near(uv, [2.0, 0.5]);
    // width/height overflows, but the image-to-box aspect ratio and reciprocal
    // remain finite. A centered cover still maps its center to the image center.
    near(
        cover
            .image_uv([0.5; 2], [1e308, 1e-9], [u32::MAX, 1])
            .unwrap()
            .unwrap(),
        [0.5, 0.0],
    );
    let centered = ImageMapping::default();
    near(
        centered
            .image_uv([0.5; 2], [1e308, 1e-9], [u32::MAX, 1])
            .unwrap()
            .unwrap(),
        [0.5; 2],
    );
    near(
        centered
            .image_uv([0.5; 2], [1e-9, 1e308], [1, u32::MAX])
            .unwrap()
            .unwrap(),
        [0.5; 2],
    );
    let stretch = mapping(ImageFit::Stretch, [0.5; 2], ImageAddressMode::Clamp);
    near(
        stretch
            .image_uv([0.25, 0.75], [f64::MAX, f64::from_bits(1)], [1, 1])
            .unwrap()
            .unwrap(),
        [0.25; 2],
    );
}

#[test]
fn all_fits_match_independent_physical_rectangle_placement() {
    let mut samples = 0;
    for fit in [ImageFit::Stretch, ImageFit::Cover, ImageFit::Contain] {
        for local in [[2.0, 3.0], [7.0, 2.0], [1.0, 1.0]] {
            for image in [[3, 7], [11, 2], [4, 4]] {
                for alignment in [[0.0, 1.0], [0.25, 0.75], [0.5; 2], [1.0, 0.0]] {
                    let paint = mapping(fit, alignment, ImageAddressMode::Clamp);
                    // Reference places a physical image rectangle in physical
                    // object bounds; it does not compute normalized aspect ratios.
                    let scales: [f64; 2] = [
                        local[0] / f64::from(image[0]),
                        local[1] / f64::from(image[1]),
                    ];
                    let scale = match fit {
                        ImageFit::Cover => scales[0].max(scales[1]),
                        _ => scales[0].min(scales[1]),
                    };
                    let size = if fit == ImageFit::Stretch {
                        local
                    } else {
                        [f64::from(image[0]) * scale, f64::from(image[1]) * scale]
                    };
                    for point in [
                        [-0.1, 0.3],
                        [0.125, 0.625],
                        [0.5; 2],
                        [0.875, 0.375],
                        [1.1, 0.7],
                    ] {
                        let physical = [point[0] * local[0], point[1] * local[1]];
                        let uv: [f64; 2] = std::array::from_fn(|axis| {
                            (physical[axis] - (local[axis] - size[axis]) * alignment[axis])
                                / size[axis]
                        });
                        let expected = if fit == ImageFit::Contain
                            && uv.into_iter().any(|v| !(0.0..=1.0).contains(&v))
                        {
                            None
                        } else {
                            Some([uv[0], 1.0 - uv[1]])
                        };
                        match (paint.image_uv(point, local, image).unwrap(), expected) {
                            (Some(actual), Some(expected)) => near(actual, expected),
                            (None, None) => {}
                            pair => panic!(
                                "{fit:?} {local:?} {image:?} {alignment:?} {point:?}: {pair:?}"
                            ),
                        }
                        samples += 1;
                    }
                }
            }
        }
    }
    assert_eq!(samples, 540);
}

#[test]
fn invalid_constructor_and_sampling_values_fail_before_use() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
        assert_eq!(
            ImageMapping::new(
                ImageFit::Cover,
                [value, 0.5],
                ImageAddressMode::Clamp,
                ImageUvTransform::default()
            ),
            Err(ImagePaintError::InvalidAlignment)
        );
    }
    for scale in [[0.0, 1.0], [-0.0, 1.0], [f64::INFINITY, 1.0]] {
        assert_eq!(
            ImageUvTransform::new([0.0; 2], scale, 0.0),
            Err(ImagePaintError::InvalidTransform)
        );
    }
    for value in [f64::NAN, f64::INFINITY] {
        assert_eq!(
            ImageUvTransform::new([value, 0.0], [1.0; 2], 0.0),
            Err(ImagePaintError::InvalidTransform)
        );
        assert_eq!(
            ImageUvTransform::new([0.0; 2], [1.0; 2], value),
            Err(ImagePaintError::InvalidTransform)
        );
    }
    let paint = ImageMapping::default();
    for size in [
        [0.0, 1.0],
        [-1.0, 1.0],
        [f64::INFINITY, 1.0],
        [f64::MAX, f64::MIN_POSITIVE],
    ] {
        assert_eq!(
            paint.image_uv([0.5; 2], size, [1, 1]),
            Err(ImagePaintError::InvalidBounds)
        );
    }
    assert_eq!(
        paint.image_uv([f64::NAN, 0.5], [1.0; 2], [1, 1]),
        Err(ImagePaintError::InvalidSamplePoint)
    );
    assert_eq!(
        paint.image_uv([0.5; 2], [1.0; 2], [0, 1]),
        Err(ImagePaintError::InvalidBounds)
    );
}

#[test]
fn image_paint_handles_do_not_pin_pixels_or_alias_foreign_and_later_resources() {
    let mut arena = RasterImageResourceArena::new();
    let pixels: Arc<[u8]> = Arc::from([255, 0, 0, 255]);
    let weak = Arc::downgrade(&pixels);
    let handle = arena.intern_rgba8(1, 1, pixels).unwrap();
    let paint = ImagePaint::new(handle, ImageMapping::default(), RasterImageSampling::Linear);
    let stats = arena.stats();
    for tick in 0..101 {
        let moved = ImagePaint::new(
            handle,
            ImageMapping::new(
                ImageFit::Cover,
                [f64::from(tick) / 100.0, 0.5],
                ImageAddressMode::Clamp,
                ImageUvTransform::default(),
            )
            .unwrap(),
            RasterImageSampling::Linear,
        );
        let sample = paint.interpolate(moved, 0.5).unwrap();
        near(
            sample
                .sample_premultiplied(&arena, [1.0; 2], [0.5; 2])
                .unwrap(),
            [1.0, 0.0, 0.0, 1.0],
        );
        assert_eq!(arena.stats(), stats);
    }
    let foreign = arena.clone();
    assert_eq!(
        paint.sample_premultiplied(&foreign, [1.0; 2], [0.5; 2]),
        Err(ImagePaintError::MissingResource)
    );
    drop(foreign);
    drop(arena.remove(handle));
    assert!(weak.upgrade().is_none());
    assert_eq!(
        paint.sample_premultiplied(&arena, [1.0; 2], [0.5; 2]),
        Err(ImagePaintError::MissingResource)
    );
    let later = arena.intern_rgba8(1, 1, vec![255, 0, 0, 255]).unwrap();
    assert_ne!(later, handle);
    let other = ImagePaint::new(later, paint.mapping(), paint.sampling());
    assert_eq!(
        paint.interpolate(other, 0.5),
        Err(ImagePaintError::DifferentResourceOrSampling)
    );
    let resampled = ImagePaint::new(later, paint.mapping(), RasterImageSampling::Bicubic);
    assert_eq!(
        other.interpolate(resampled, 0.5),
        Err(ImagePaintError::DifferentResourceOrSampling)
    );
    let stale = ImagePaint::new(
        RasterImageResourceHandle {
            version: later.version + 1,
            ..later
        },
        other.mapping(),
        other.sampling(),
    );
    assert_eq!(
        stale.sample_premultiplied(&arena, [1.0; 2], [0.5; 2]),
        Err(ImagePaintError::MissingResource)
    );
}
