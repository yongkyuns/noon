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
