use super::*;

fn paint(options: HatchPaintOptions) -> HatchPaint {
    HatchPaint::new(
        Color::rgba(0.0, 0.0, 1.0, 0.5),
        Color::rgba(1.0, 0.0, 0.0, 0.0),
        options,
    )
    .unwrap()
}

fn unit_options() -> HatchPaintOptions {
    HatchPaintOptions {
        spacing: 1.0,
        width: 0.25,
        angle: 0.0,
        phase: [0.0; 2],
        cross: false,
    }
}

#[test]
fn literal_local_line_phase_width_and_alpha_vectors() {
    let hatch = paint(unit_options());
    for (point, expected) in [
        ([0.0, 0.0], [0.0, 0.0, 0.5, 0.5]),
        ([7.0, 0.1], [0.0, 0.0, 0.5, 0.5]),
        ([0.0, -0.125], [0.0, 0.0, 0.5, 0.5]),
        ([0.0, 0.125], [0.0; 4]),
        ([0.0, 0.5], [0.0; 4]),
        ([0.0, -1.0], [0.0, 0.0, 0.5, 0.5]),
    ] {
        assert_eq!(hatch.sample_premultiplied(point).unwrap(), expected);
    }
    let shifted = paint(HatchPaintOptions {
        phase: [0.25, 0.0],
        ..unit_options()
    });
    assert_eq!(shifted.sample_premultiplied([0.0, 0.0]).unwrap(), [0.0; 4]);
    assert_eq!(
        shifted.sample_premultiplied([0.0, -0.25]).unwrap(),
        [0.0, 0.0, 0.5, 0.5]
    );
    let perpendicular = paint(HatchPaintOptions {
        angle: std::f64::consts::FRAC_PI_2,
        ..unit_options()
    });
    assert_eq!(
        perpendicular.sample_premultiplied([0.5, 0.0]).unwrap(),
        [0.0; 4]
    );
}

#[test]
fn periodic_band_filter_matches_independent_interval_clipping() {
    // Independent reference enumerates the actual bands intersecting the box.
    // Production instead removes complete periods and uses three local bands.
    let reference = |center: f64, width: f64, duty: f64| {
        if width >= 256.0 {
            return duty;
        }
        let lower = center - width * 0.5;
        let upper = center + width * 0.5;
        let mut sum = 0.0;
        for n in (lower.floor() as i32 - 1)..=(upper.ceil() as i32 + 1) {
            let a = f64::from(n) - duty * 0.5;
            let b = f64::from(n) + duty * 0.5;
            sum += (upper.min(b) - lower.max(a)).max(0.0);
        }
        sum / width
    };
    for duty in [0.0, 0.125, 0.25, 0.7, 1.0] {
        for center in [-1.375, -0.125, 0.0, 0.123, 0.5, 0.875, 2.0] {
            for width in [0.007, 0.125, 0.75, 1.0, 1.4, 2.75, 17.125, 255.75, 256.0] {
                let actual = band_coverage(center, width, duty);
                let expected = reference(center, width, duty);
                assert!(
                    (actual - expected).abs() < 1e-12,
                    "center={center} width={width} duty={duty}: {actual} != {expected}"
                );
            }
        }
    }
    for width in [f64::MIN_POSITIVE, 1e-100, 1e-18] {
        assert_eq!(band_coverage(0.125, width, 0.25), 0.5);
        assert_eq!(band_coverage(0.0, width, 0.25), 1.0);
        assert_eq!(band_coverage(0.5, width, 0.25), 0.0);
    }
}

#[test]
fn crosshatch_is_union_and_duty_endpoints_are_exact() {
    let hatch = paint(HatchPaintOptions {
        cross: true,
        ..unit_options()
    });
    // Both independent box-axis line coverages are 1/2: their union is 3/4.
    assert_eq!(
        hatch
            .sample_filtered_premultiplied([0.0, 0.0], [0.5; 2])
            .unwrap(),
        [0.0, 0.0, 0.375, 0.375]
    );
    for width in [0.0, 1.0] {
        let endpoint = paint(HatchPaintOptions {
            width,
            cross: true,
            ..unit_options()
        });
        for footprint in [[0.0; 2], [0.5, 1.4], [1e100; 2]] {
            assert_eq!(
                endpoint
                    .sample_filtered_premultiplied([0.123, 0.456], footprint)
                    .unwrap(),
                [0.0, 0.0, 0.5 * width, 0.5 * width]
            );
        }
    }
}

#[test]
fn periodic_projection_preserves_authored_large_and_tiny_phase() {
    let ordinary = paint(HatchPaintOptions {
        phase: [-0.25, 0.125],
        cross: true,
        ..unit_options()
    });
    let repeated = paint(HatchPaintOptions {
        phase: [4_294_967_295.75, -4_294_967_295.875],
        cross: true,
        ..unit_options()
    });
    assert_eq!(ordinary.phase_wrapped(), repeated.phase_wrapped());
    for local in [[0.0; 2], [1.25, -0.5], [-0.123, 4.567]] {
        assert_eq!(
            ordinary.sample_filtered_premultiplied(local, [0.15; 2]),
            repeated.sample_filtered_premultiplied(local, [0.15; 2])
        );
    }
    let tiny = paint(HatchPaintOptions {
        phase: [-1e-100, 0.0],
        ..unit_options()
    });
    assert_eq!(tiny.phase(), [-1e-100, 0.0]);
    assert_eq!(tiny.phase_wrapped(), [0.0; 2]);
}

#[test]
fn invalid_values_samples_and_topology_fail_explicitly() {
    let create = |options| HatchPaint::new(Color::WHITE, Color::BLACK, options);
    for spacing in [0.0, -0.25, f64::NAN, f64::INFINITY] {
        assert_eq!(
            create(HatchPaintOptions {
                spacing,
                ..unit_options()
            }),
            Err(HatchPaintError::InvalidSpacing)
        );
    }
    for width in [-0.01, 1.001, f64::NAN, f64::INFINITY] {
        assert_eq!(
            create(HatchPaintOptions {
                width,
                ..unit_options()
            }),
            Err(HatchPaintError::InvalidWidth)
        );
    }
    assert_eq!(
        create(HatchPaintOptions {
            angle: f64::NAN,
            ..unit_options()
        }),
        Err(HatchPaintError::InvalidAngle)
    );
    assert_eq!(
        create(HatchPaintOptions {
            phase: [0.0, f64::INFINITY],
            ..unit_options()
        }),
        Err(HatchPaintError::InvalidPhase)
    );
    assert_eq!(
        HatchPaint::new(
            Color::rgba(1.01, 0.0, 0.0, 1.0),
            Color::BLACK,
            unit_options()
        ),
        Err(HatchPaintError::InvalidColor)
    );
    let hatch = paint(unit_options());
    assert_eq!(
        hatch.sample_premultiplied([f64::NAN, 0.0]),
        Err(HatchPaintError::InvalidSamplePoint)
    );
    assert_eq!(
        hatch.sample_filtered_premultiplied([0.0; 2], [f64::INFINITY, 0.0]),
        Err(HatchPaintError::InvalidFootprint)
    );
    let cross = paint(HatchPaintOptions {
        cross: true,
        ..unit_options()
    });
    assert_eq!(
        hatch.interpolate(&cross, 0.0),
        Err(HatchPaintError::DifferentTopology)
    );
    assert_eq!(
        hatch.interpolate(&hatch, f64::NAN),
        Err(HatchPaintError::InvalidProgress)
    );
}

#[test]
fn interpolation_retains_exact_endpoints_unwrapped_phase_and_premultiplied_colors() {
    let from = paint(HatchPaintOptions {
        spacing: 0.5,
        width: 0.125,
        angle: f64::MAX,
        phase: [f64::MAX, f64::from_bits(1)],
        ..unit_options()
    });
    let to = HatchPaint::new(
        Color::rgb(1.0, 0.0, 0.0),
        Color::TRANSPARENT,
        HatchPaintOptions {
            spacing: 1.5,
            width: 0.375,
            angle: -f64::MAX,
            phase: [-f64::MAX, f64::from_bits(1)],
            ..unit_options()
        },
    )
    .unwrap();
    assert_eq!(from.interpolate(&to, 0.0).unwrap(), from);
    assert_eq!(from.interpolate(&to, 1.0).unwrap(), to);
    let middle = from.interpolate(&to, 0.5).unwrap();
    assert_eq!(middle.spacing(), 1.0);
    assert_eq!(middle.width(), 0.25);
    assert_eq!(middle.angle(), 0.0);
    assert_eq!(middle.phase(), [0.0, f64::from_bits(1)]);
    let sampled = middle.sample_premultiplied([0.0; 2]).unwrap();
    for (a, b) in sampled.into_iter().zip([0.5, 0.0, 0.25, 0.75]) {
        assert!((a - b).abs() < 1e-7);
    }
}

#[test]
fn ordinary_fill_transition_lifts_solids_and_absence_only_inside_interval() {
    use crate::{PaintTransition, SemanticPaint};
    use std::sync::Arc;
    let hatch = Arc::new(paint(unit_options()));
    for other in [None, Some(SemanticPaint::Solid(Color::RED))] {
        let transition = PaintTransition::new(
            Some(SemanticPaint::Hatch(hatch.clone())),
            0.25,
            other.clone(),
            0.75,
        )
        .unwrap();
        assert_eq!(
            transition.sample(0.0).unwrap(),
            (Some(SemanticPaint::Hatch(hatch.clone())), 0.25)
        );
        assert_eq!(transition.sample(1.0).unwrap(), (other, 0.75));
        let (middle, opacity) = transition.sample(0.5).unwrap();
        assert!(middle.as_ref().unwrap().as_hatch().is_some());
        assert_eq!(opacity, 0.5);
    }
}
