use super::*;

fn paint(seed: u32, phase: [f64; 2]) -> NoisePaint {
    NoisePaint::new(
        seed,
        [1.0; 2],
        phase,
        [Color::rgba(1.0, 0.0, 0.0, 0.0), Color::rgb(0.0, 0.0, 1.0)],
    )
    .unwrap()
}

#[test]
fn literal_seed_hash_vectors_and_independent_rational_field_weights() {
    for (seed, x, y, expected) in [
        (0, 0, 0, 0),
        (7, 0, 0, 0x948b_a1e6),
        (7, 1, 0, 0xa715_b186),
        (7, 0, 1, 0x4e0d_1773),
        (7, 1, 1, 0xb62b_59d8),
        (u32::MAX, 65_535, 65_535, 0xbd32_513f),
    ] {
        assert_eq!(lattice_hash(seed, x, y), expected);
    }
    let p = paint(7, [0.0; 2]);
    for (point, numerator) in [
        ([0.0, 0.0], 38_027.0),
        ([1.0, 0.0], 42_773.0),
        ([0.0, 1.0], 19_981.0),
        ([1.0, 1.0], 46_635.0),
        ([0.5, 0.5], 36_854.0),
        // Quintic weights at 1/4 and 3/4 are exactly 53/512 and 459/512.
        (
            [0.25, 0.75],
            (38_027.0 * 459.0 * 53.0
                + 42_773.0 * 53.0 * 53.0
                + 19_981.0 * 459.0 * 459.0
                + 46_635.0 * 53.0 * 459.0)
                / 262_144.0,
        ),
    ] {
        let sample = p.sample_premultiplied(point).unwrap();
        let expected = numerator / 65_535.0;
        assert_eq!(
            sample[0], 0.0,
            "hidden transparent RGB leaked into the field"
        );
        assert_eq!(sample[1], 0.0);
        assert!(
            (sample[2] - expected).abs() < 1e-14,
            "{point:?}: {sample:?}"
        );
        assert!((sample[3] - expected).abs() < 1e-14);
    }
}

#[test]
fn phase_wrap_point_clamp_seed_and_extreme_finite_values_are_defined() {
    let original = paint(7, [-0.25, 0.75]);
    let repeated = paint(7, [65_535.75, 65_536.75]);
    for point in [[0.0; 2], [0.25, 0.75], [0.5; 2], [1.0; 2]] {
        assert_eq!(
            original.sample_premultiplied(point),
            repeated.sample_premultiplied(point)
        );
        assert_ne!(
            original.sample_premultiplied(point),
            paint(8, [-0.25, 0.75]).sample_premultiplied(point)
        );
    }
    assert_eq!(
        original.sample_premultiplied([-1.0, 2.0]),
        original.sample_premultiplied([0.0, 1.0])
    );
    let extreme = NoisePaint::new(
        u32::MAX,
        [f64::MAX, f64::from_bits(1)],
        [f64::MAX, -f64::MAX],
        [Color::BLACK, Color::WHITE],
    )
    .unwrap();
    assert!(extreme
        .sample_premultiplied([f64::MAX; 2])
        .unwrap()
        .into_iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(&v)));
}

#[test]
fn invalid_noise_values_reject_before_any_shared_value_exists() {
    for frequency in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            NoisePaint::new(0, [frequency, 1.0], [0.0; 2], [Color::BLACK, Color::WHITE]),
            Err(NoisePaintError::InvalidFrequency)
        );
    }
    for phase in [f64::NAN, f64::INFINITY] {
        assert_eq!(
            NoisePaint::new(0, [1.0; 2], [phase, 0.0], [Color::BLACK, Color::WHITE]),
            Err(NoisePaintError::InvalidPhase)
        );
    }
    for channel in [-0.1, 1.1, f32::NAN] {
        assert_eq!(
            NoisePaint::new(
                0,
                [1.0; 2],
                [0.0; 2],
                [Color::rgba(channel, 0.0, 0.0, 1.0), Color::WHITE]
            ),
            Err(NoisePaintError::InvalidColor)
        );
    }
    assert_eq!(
        paint(7, [0.0; 2]).sample_premultiplied([f64::NAN, 0.0]),
        Err(NoisePaintError::InvalidSamplePoint)
    );
}

#[test]
fn box_filter_matches_independent_gaussian_quadrature_and_coarse_mean() {
    // Three-point Gauss integrates each degree-five interpolation basis exactly.
    // This reference evaluates the point polynomial over literal corner values,
    // rather than using the production antiderivative/axis-weight code.
    let nodes = [-(3.0_f64 / 5.0).sqrt(), 0.0, (3.0_f64 / 5.0).sqrt()];
    let weights = [5.0 / 9.0, 8.0 / 9.0, 5.0 / 9.0];
    let curve = |t: f64| 10.0 * t.powi(3) - 15.0 * t.powi(4) + 6.0 * t.powi(5);
    let mut expected = 0.0;
    for (i, x) in nodes.into_iter().enumerate() {
        for (j, y) in nodes.into_iter().enumerate() {
            let u = curve(0.25 + x * 0.25);
            let v = curve(0.75 + y * 0.125);
            expected += ((1.0 - u) * (1.0 - v) * 38_027.0
                + u * (1.0 - v) * 42_773.0
                + (1.0 - u) * v * 19_981.0
                + u * v * 46_635.0)
                * weights[i]
                * weights[j]
                / 4.0
                / 65_535.0;
        }
    }
    let p = paint(7, [0.0; 2]);
    let filtered = p
        .sample_filtered_premultiplied([0.25, 0.75], [0.5, 0.25])
        .unwrap();
    assert!((filtered[2] - expected).abs() < 1e-12);
    assert!((filtered[2] - p.sample_premultiplied([0.25, 0.75]).unwrap()[2]).abs() > 0.001);
    assert_eq!(
        p.sample_filtered_premultiplied([0.25, 0.75], [2.0, 0.1])
            .unwrap(),
        [0.0, 0.0, 0.5, 0.5]
    );
    assert_eq!(
        p.sample_filtered_premultiplied([0.25, 0.75], [-1.0, 0.1]),
        Err(NoisePaintError::InvalidFootprint)
    );
}

#[test]
fn noise_interpolation_keeps_exact_endpoints_and_rejects_seed_changes() {
    let from = paint(7, [f64::MAX, 0.0]);
    let to = paint(7, [-f64::MAX, 1.0]);
    assert_eq!(from.interpolate(&to, 0.0).unwrap(), from);
    assert_eq!(from.interpolate(&to, 1.0).unwrap(), to);
    assert_eq!(from.interpolate(&to, 0.5).unwrap().phase(), [0.0, 0.5]);
    assert_eq!(
        from.interpolate(&paint(8, [0.0; 2]), 0.0),
        Err(NoisePaintError::DifferentSeed)
    );
    assert_eq!(
        from.interpolate(&to, f64::NAN),
        Err(NoisePaintError::InvalidProgress)
    );
}

#[test]
fn rounded_period_seam_stays_within_its_own_sixteen_bit_axis() {
    for phase in [[-1e-20, 0.0], [0.0, -1e-20], [-1e-20; 2]] {
        assert_eq!(paint(7, phase).phase_parts(), ([0; 2], [0.0; 2]));
    }
}

#[test]
fn frequency_animation_preserves_unchanged_subnormal_and_extreme_phase() {
    let phase = [f64::from_bits(1), f64::MAX];
    let from = NoisePaint::new(7, [1.0; 2], phase, [Color::WHITE, Color::BLACK]).unwrap();
    let to = NoisePaint::new(7, [2.0; 2], phase, [Color::WHITE, Color::BLACK]).unwrap();
    for progress in [0.1, 0.25, 0.5, 0.9] {
        assert_eq!(from.interpolate(&to, progress).unwrap().phase(), phase);
    }
}

#[test]
fn fractal_factory_matches_explicit_detail_and_one_octave_is_the_original_field() {
    let basic = NoisePaint::new(0, [8.0; 2], [0.0; 2], [Color::BLACK, Color::WHITE]).unwrap();
    let default = NoisePaint::fractal(Color::BLACK, Color::WHITE).unwrap();
    assert_eq!(default, basic.clone().with_detail(4, 2.0, 0.5).unwrap());
    let single = default.with_detail(1, 2.0, 0.5).unwrap();
    for point in [[0.0, 0.0], [0.173, 0.789], [1.0, 1.0]] {
        for width in [[0.0; 2], [0.17, 0.51], [1.5; 2], [3.0; 2]] {
            assert_eq!(
                single.sample_filtered_premultiplied(point, width),
                basic.sample_filtered_premultiplied(point, width)
            );
        }
    }
}

#[test]
fn fractal_normalizes_weights_filters_each_octave_and_uses_distinct_seeds() {
    let f = NoisePaint::fractal(Color::BLACK, Color::WHITE)
        .unwrap()
        .with_seed(u32::MAX)
        .with_frequency([3.0, 5.0])
        .unwrap()
        .with_phase([-0.173, 0.231])
        .unwrap();
    for point in [[0.13, 0.27], [0.55, 0.81]] {
        for footprint in [[0.0; 2], [0.2, 0.4], [1.2, 0.1], [3.0; 2]] {
            let mut expected = 0.0;
            for i in 0..4 {
                let scale = 2.0_f64.powi(i);
                let single = NoisePaint::new(
                    u32::MAX.wrapping_add((i as u32).wrapping_mul(0x9e37_79b9)),
                    [3.0 * scale, 5.0 * scale],
                    [-0.173 * scale, 0.231 * scale],
                    [Color::BLACK, Color::WHITE],
                )
                .unwrap();
                expected += 0.5_f64.powi(i)
                    * single
                        .sample_filtered_premultiplied(point, footprint.map(|v| v * scale))
                        .unwrap()[0];
            }
            expected /= 1.875;
            let actual = f.sample_filtered_premultiplied(point, footprint).unwrap();
            assert!(
                (actual[0] - expected).abs() < 1e-10,
                "{point:?} {footprint:?}: {actual:?} != {expected}"
            );
            assert_eq!(actual[3], 1.0);
        }
    }
}

#[test]
fn fractal_ramp_and_transitions_retain_alpha_topology_and_shared_unchanged_stops() {
    let f = NoisePaint::fractal(Color::BLACK, Color::WHITE)
        .unwrap()
        .with_stops([
            GradientStop::new(0.0, Color::rgba(1.0, 0.0, 0.0, 0.0)).unwrap(),
            GradientStop::new(0.5, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
            GradientStop::new(1.0, Color::WHITE).unwrap(),
        ])
        .unwrap();
    assert_eq!(f.color_at(0.0), [0.0; 4]);
    assert_eq!(f.color_at(0.25), [0.0, 0.0, 0.5, 0.5]);
    assert_eq!(f.color_at(0.5), [0.0, 0.0, 1.0, 1.0]);
    let target = f.clone().with_phase([1.5, -0.5]).unwrap();
    let middle = f.interpolate(&target, 0.5).unwrap();
    assert_eq!(middle.phase(), [0.75, -0.25]);
    match (&middle.stops, &f.stops) {
        (NoiseStops::Shared(a), NoiseStops::Shared(b)) => {
            assert!(
                Arc::ptr_eq(a, b),
                "phase-only animation shares the larger ramp"
            );
        }
        _ => panic!("larger ramps retain shared storage"),
    }
    assert_eq!(f.interpolate(&target, 0.0).unwrap(), f);
    assert_eq!(f.interpolate(&target, 1.0).unwrap(), target);
    assert_eq!(
        f.interpolate(&f.clone().with_detail(3, 2.0, 0.5).unwrap(), 0.5),
        Err(NoisePaintError::DifferentDetail)
    );
    assert_eq!(
        f.interpolate(&f.clone().with_detail(4, 1.5, 0.5).unwrap(), 0.5),
        Err(NoisePaintError::DifferentDetail)
    );
    assert_eq!(
        f.interpolate(
            &NoisePaint::fractal(Color::BLACK, Color::WHITE).unwrap(),
            0.5
        ),
        Err(NoisePaintError::DifferentStopCounts)
    );
}

#[test]
fn fractal_invalid_detail_and_ramp_are_rejected_and_extreme_phase_stays_finite() {
    let base = NoisePaint::fractal(Color::BLACK, Color::WHITE).unwrap();
    for octaves in [0, 9, u8::MAX] {
        assert_eq!(
            base.clone().with_detail(octaves, 2.0, 0.5),
            Err(NoisePaintError::InvalidOctaves)
        );
    }
    for scale in [0.0, 0.9, 4.1, f64::INFINITY, f64::NAN] {
        assert_eq!(
            base.clone().with_detail(4, scale, 0.5),
            Err(NoisePaintError::InvalidLacunarity)
        );
    }
    for gain in [-0.1, 1.1, f64::INFINITY, f64::NAN] {
        assert_eq!(
            base.clone().with_detail(4, 2.0, gain),
            Err(NoisePaintError::InvalidGain)
        );
    }
    assert!(base.clone().with_stops([]).is_err());
    let value = base.with_phase([f64::MAX, -f64::MAX]).unwrap();
    for octave in 0..4 {
        let (cell, fraction) = value.octave_phase_parts(octave).unwrap();
        assert!(cell.into_iter().all(|v| v < 65536));
        assert!(fraction
            .into_iter()
            .all(|v| v.is_finite() && (0.0..1.0).contains(&v)));
    }
    assert!(value.octave_phase_parts(4).is_none());
    assert!(value
        .sample_premultiplied([0.17, 0.39])
        .unwrap()
        .into_iter()
        .all(f64::is_finite));
}
