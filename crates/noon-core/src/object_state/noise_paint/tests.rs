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
