use super::*;
use crate::Color;

fn stops() -> [GradientStop; 2] {
    [
        GradientStop::new(0.0, Color::rgb(1.0, 0.0, 0.0)).unwrap(),
        GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
    ]
}

#[test]
fn literal_elliptical_field_vectors_include_diagonals_and_pad() {
    let paint = RadialGradient::new([0.25, 0.75], [0.5, 0.25], stops()).unwrap();
    for (point, distance) in [
        ([0.25, 0.75], 0.0),
        ([0.5, 0.75], 0.5),
        ([0.25, 0.625], 0.5),
        // A 3-4-5 triangle in ellipse coordinates, scaled by 0.5.
        ([0.4, 0.85], 0.5),
        ([0.75, 0.75], 1.0),
        ([1.25, 0.75], 1.0),
    ] {
        let expected = [1.0 - distance, 0.0, distance, 1.0];
        let actual = paint.sample_premultiplied(point).unwrap();
        for (a, e) in actual.into_iter().zip(expected) {
            assert!((a - e).abs() < 1e-14, "{point:?}: {actual:?}");
        }
    }
    assert_eq!(paint.center(), [0.25, 0.75]);
    assert_eq!(paint.radii(), [0.5, 0.25]);
}

#[test]
fn hidden_rgb_and_coincident_stops_use_the_shared_premultiplied_contract() {
    let paint = RadialGradient::new(
        [0.0; 2],
        [1.0; 2],
        [
            GradientStop::new(0.0, Color::rgba(0.0, 1.0, 0.0, 0.0)).unwrap(),
            GradientStop::new(0.5, Color::rgb(1.0, 0.0, 0.0)).unwrap(),
            GradientStop::new(0.5, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
            GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        paint.sample_premultiplied([0.25, 0.0]).unwrap(),
        [0.5, 0.0, 0.0, 0.5]
    );
    assert_eq!(
        paint.sample_premultiplied([0.5, 0.0]).unwrap(),
        [0.0, 0.0, 1.0, 1.0]
    );
}

#[test]
fn positive_subnormal_and_huge_radii_have_defined_center_and_pad_samples() {
    let paint = RadialGradient::new([0.5; 2], [f64::from_bits(1); 2], stops()).unwrap();
    assert_eq!(
        paint.sample_premultiplied([0.5; 2]).unwrap(),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        paint.sample_premultiplied([f64::MAX; 2]).unwrap(),
        [0.0, 0.0, 1.0, 1.0]
    );
    let huge = RadialGradient::new([0.0; 2], [f64::MAX; 2], stops()).unwrap();
    assert_eq!(
        huge.sample_premultiplied([f64::MAX, 0.0]).unwrap(),
        [0.0, 0.0, 1.0, 1.0]
    );
}

#[test]
fn invalid_fields_and_stops_reject_before_a_value_exists() {
    for value in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert_eq!(
            RadialGradient::new([value, 0.5], [0.5; 2], stops()),
            Err(GradientError::InvalidCenter)
        );
    }
    for value in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        assert_eq!(
            RadialGradient::new([0.5; 2], [value, 0.5], stops()),
            Err(GradientError::InvalidRadii)
        );
    }
    assert_eq!(
        RadialGradient::new([0.5; 2], [0.5; 2], []),
        Err(GradientError::TooFewStops)
    );
    assert_eq!(
        RadialGradient::new([0.5; 2], [0.5; 2], stops().into_iter().rev()),
        Err(GradientError::UnorderedStops)
    );
    let paint = RadialGradient::new([0.5; 2], [0.5; 2], stops()).unwrap();
    assert_eq!(
        paint.sample_premultiplied([f64::NAN, 0.5]),
        Err(GradientError::InvalidSamplePoint)
    );
}
