//! Independent finite filter specification. Enumerate each periodic foreground
//! interval and clip it against the projected physical-pixel box. This deliberately
//! does not use the production sampler, whole-period residual, or GPU projector.

#[derive(Clone, Copy, Debug)]
pub struct Pattern {
    pub spacing: f64,
    pub width: f64,
    pub angle: f64,
    pub phase: [f64; 2],
    pub cross: bool,
}

fn interval_coverage(lower: f64, upper: f64, duty: f64) -> f64 {
    if duty == 0.0 || duty == 1.0 {
        return duty;
    }
    let span = upper - lower;
    if span >= 256.0 {
        return duty;
    }
    if span == 0.0 {
        let fraction = lower - lower.floor();
        return f64::from(fraction < duty / 2.0 || fraction >= 1.0 - duty / 2.0);
    }
    let first = lower.floor() as i64 - 1;
    let last = upper.ceil() as i64 + 1;
    let mut covered = 0.0;
    for center in first..=last {
        let band_lower = center as f64 - duty / 2.0;
        let band_upper = center as f64 + duty / 2.0;
        covered += (upper.min(band_upper) - lower.max(band_lower)).max(0.0);
    }
    (covered / span).clamp(0.0, 1.0)
}

/// Physical-pixel corners have already been independently transformed into the
/// object's local scene units. Project all four corners to obtain each filter
/// interval; no center derivative or prepared-renderer values are consulted.
pub fn coverage(corners: [[f64; 2]; 4], pattern: Pattern) -> f64 {
    let sin = pattern.angle.sin();
    let cos = pattern.angle.cos();
    let directions = [[-sin, cos], [cos, sin]];
    let duty = pattern.width / pattern.spacing;
    let mut axes = [0.0; 2];
    for (axis, value) in axes
        .iter_mut()
        .enumerate()
        .take(if pattern.cross { 2 } else { 1 })
    {
        let coordinates = corners.map(|point| {
            (directions[axis][0] * point[0] + directions[axis][1] * point[1]) / pattern.spacing
                + pattern.phase[axis].fract()
        });
        let lower = coordinates.into_iter().fold(f64::INFINITY, f64::min);
        let upper = coordinates.into_iter().fold(f64::NEG_INFINITY, f64::max);
        *value = interval_coverage(lower, upper, duty);
    }
    // Independent area union of two perpendicular band families over their
    // declared axis-aligned reconstruction box.
    1.0 - (1.0 - axes[0]) * (1.0 - axes[1])
}

#[test]
fn explicit_clipping_matches_hand_computed_bands_and_half_open_points() {
    for (lower, upper, duty, expected) in [
        (-0.1, 0.1, 0.25, 1.0),
        (-0.25, 0.25, 0.25, 0.5),
        (0.375, 0.625, 0.25, 0.0),
        (-2.0, 3.0, 0.25, 0.25),
        (0.125, 0.125, 0.25, 0.0),
        (-0.125, -0.125, 0.25, 1.0),
        (0.0, 0.0, 0.0, 0.0),
        (0.5, 0.5, 1.0, 1.0),
        (-128.0, 128.0, 0.35, 0.35),
    ] {
        let actual = interval_coverage(lower, upper, duty);
        assert!(
            (actual - expected).abs() < 1e-12,
            "{lower}..{upper}: {actual} != {expected}"
        );
    }
    let tiny = interval_coverage(0.125 - 1e-8, 0.125 + 1e-8, 0.25);
    assert!((tiny - 0.5).abs() < 1e-8, "tiny edge box: {tiny}");
}

#[test]
fn clipped_cross_union_and_integer_phase_are_independent_of_origin() {
    let corners = [[-0.25, -0.25], [-0.25, 0.25], [0.25, -0.25], [0.25, 0.25]];
    let pattern = Pattern {
        spacing: 1.0,
        width: 0.25,
        angle: 0.0,
        phase: [0.0; 2],
        cross: true,
    };
    assert_eq!(coverage(corners, pattern), 0.75);
    assert_eq!(
        coverage(
            corners,
            Pattern {
                phase: [4096.0, -8192.0],
                ..pattern
            }
        ),
        0.75
    );
    assert_eq!(
        coverage(
            corners,
            Pattern {
                cross: false,
                ..pattern
            }
        ),
        0.5
    );
}
