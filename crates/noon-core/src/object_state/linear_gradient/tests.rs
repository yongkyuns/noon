use super::*;

#[test]
fn padding_before_a_duplicate_zero_stop_keeps_the_first_color() {
    let red = Color::rgb(1.0, 0.0, 0.0);
    let blue = Color::rgb(0.0, 0.0, 1.0);
    let gradient = LinearGradient::new(
        [0.0, 0.0],
        [1.0, 0.0],
        [stop(0.0, red), stop(0.0, blue), stop(1.0, blue)],
    )
    .unwrap();
    assert_eq!(
        gradient.sample_premultiplied([-0.1, 0.0]).unwrap(),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        gradient.sample_premultiplied([0.0, 0.0]).unwrap(),
        [0.0, 0.0, 1.0, 1.0]
    );
}

fn stop(position: f64, color: Color) -> GradientStop {
    GradientStop::new(position, color).unwrap()
}

#[test]
fn shared_specification_vectors_match_the_independent_rational_reference() {
    fn numbers<const N: usize>(field: &str) -> [f64; N] {
        field
            .split(',')
            .map(|value| value.parse::<f64>().unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }
    let mut count = 0;
    for line in include_str!("../../../../../tests/visual-effects/linear-gradient-vectors.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: [&str; 6] = line.split('|').collect::<Vec<_>>().try_into().unwrap();
        let stops = fields[3].split(';').map(|entry| {
            let [position, r, g, b, a] = numbers(entry);
            stop(
                position,
                Color::rgba(r as f32, g as f32, b as f32, a as f32),
            )
        });
        let gradient = LinearGradient::new(numbers(fields[1]), numbers(fields[2]), stops).unwrap();
        let actual = gradient.sample_premultiplied(numbers(fields[4])).unwrap();
        let expected: [f64; 4] = numbers(fields[5]);
        for (a, e) in actual.into_iter().zip(expected) {
            assert!(
                a.is_finite() && (a - e).abs() <= 1e-12,
                "{}: {actual:?} != {expected:?}",
                fields[0]
            );
        }
        count += 1;
    }
    assert_eq!(count, 25, "every registered reference vector must execute");
}

fn paint(start: [f64; 2], end: [f64; 2]) -> LinearGradient {
    LinearGradient::new(
        start,
        end,
        [
            stop(0.0, Color::rgb(1.0, 0.0, 0.0)),
            stop(1.0, Color::rgb(0.0, 0.0, 1.0)),
        ],
    )
    .unwrap()
}

#[test]
fn axis_projection_has_literal_horizontal_vertical_and_diagonal_vectors() {
    let horizontal = paint([0.0, 0.5], [1.0, 0.5]);
    let vertical = paint([0.5, 0.0], [0.5, 1.0]);
    let diagonal = paint([0.0, 0.0], [1.0, 1.0]);
    for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let expected = [1.0 - progress, 0.0, progress, 1.0];
        assert_eq!(
            horizontal.sample_premultiplied([progress, 0.9]).unwrap(),
            expected
        );
        assert_eq!(
            vertical.sample_premultiplied([0.9, progress]).unwrap(),
            expected
        );
        assert_eq!(
            diagonal.sample_premultiplied([progress, progress]).unwrap(),
            expected
        );
    }
    // Projection onto a diagonal is not its x coordinate.
    assert_eq!(
        diagonal.sample_premultiplied([1.0, 0.0]).unwrap(),
        [0.5, 0.0, 0.5, 1.0]
    );
}

#[test]
fn transparent_color_cannot_bleed_rgb_into_interpolation() {
    let gradient = LinearGradient::new(
        [0.0, 0.0],
        [1.0, 0.0],
        [
            stop(0.0, Color::rgba(1.0, 0.0, 0.0, 0.0)),
            stop(1.0, Color::rgb(0.0, 0.0, 1.0)),
        ],
    )
    .unwrap();
    assert_eq!(
        gradient.sample_premultiplied([0.5, 0.0]).unwrap(),
        [0.0, 0.0, 0.5, 0.5]
    );
    assert_eq!(gradient.sample_premultiplied([0.0, 0.0]).unwrap(), [0.0; 4]);
}

#[test]
fn all_duplicate_stops_and_exact_hard_edges_use_last_stop_at_position() {
    let gradient = LinearGradient::new(
        [0.0, 0.0],
        [1.0, 0.0],
        [
            stop(0.0, Color::rgb(1.0, 0.0, 0.0)),
            stop(0.5, Color::rgb(1.0, 0.0, 0.0)),
            stop(0.5, Color::rgb(0.0, 1.0, 0.0)),
            stop(0.5, Color::rgb(0.0, 0.0, 1.0)),
            stop(1.0, Color::rgb(0.0, 0.0, 1.0)),
        ],
    )
    .unwrap();
    assert_eq!(
        gradient
            .sample_premultiplied([0.5 - f64::EPSILON, 0.0])
            .unwrap(),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        gradient.sample_premultiplied([0.5, 0.0]).unwrap(),
        [0.0, 0.0, 1.0, 1.0]
    );
    let coincident = LinearGradient::new(
        [0.0, 0.0],
        [1.0, 0.0],
        [
            stop(0.5, Color::rgb(1.0, 0.0, 0.0)),
            stop(0.5, Color::rgb(0.0, 0.0, 1.0)),
        ],
    )
    .unwrap();
    assert_eq!(
        coincident.sample_premultiplied([0.0, 0.0]).unwrap(),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        coincident.sample_premultiplied([1.0, 0.0]).unwrap(),
        [0.0, 0.0, 1.0, 1.0]
    );
}

#[test]
fn pads_to_first_and_last_stop_including_partial_stop_domain() {
    let gradient = LinearGradient::new(
        [0.0, 0.0],
        [1.0, 0.0],
        [
            stop(0.25, Color::rgb(1.0, 0.0, 0.0)),
            stop(0.75, Color::rgb(0.0, 0.0, 1.0)),
        ],
    )
    .unwrap();
    for x in [-f64::MAX, -1.0, 0.0, 0.125, 0.25] {
        assert_eq!(
            gradient.sample_premultiplied([x, 0.0]).unwrap(),
            [1.0, 0.0, 0.0, 1.0]
        );
    }
    for x in [0.75, 0.875, 1.0, 2.0, f64::MAX] {
        assert_eq!(
            gradient.sample_premultiplied([x, 0.0]).unwrap(),
            [0.0, 0.0, 1.0, 1.0]
        );
    }
}

#[test]
fn valid_subnormal_axis_never_divides_by_an_underflowed_squared_length() {
    let tiny = f64::from_bits(1);
    let gradient = paint([0.0, 0.0], [tiny, 0.0]);
    assert_eq!(
        gradient.sample_premultiplied([0.0, 0.0]).unwrap(),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        gradient.sample_premultiplied([tiny, 0.0]).unwrap(),
        [0.0, 0.0, 1.0, 1.0]
    );
    assert_eq!(
        gradient.sample_premultiplied([1.0, 0.0]).unwrap(),
        [0.0, 0.0, 1.0, 1.0]
    );
}

#[test]
fn reject_invalid_values_before_a_definition_exists() {
    for value in [f64::NAN, f64::INFINITY, -f64::INFINITY, -0.1, 1.1] {
        assert_eq!(
            GradientStop::new(value, Color::rgb(1.0, 0.0, 0.0)),
            Err(LinearGradientError::InvalidStopPosition)
        );
        assert_eq!(
            LinearGradient::new(
                [value, 0.0],
                [1.0, 0.0],
                [
                    stop(0.0, Color::rgb(1.0, 0.0, 0.0)),
                    stop(1.0, Color::rgb(0.0, 0.0, 1.0))
                ]
            ),
            Err(LinearGradientError::InvalidEndpoints)
        );
    }
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        for channel in 0..4 {
            let mut rgba = [0.5; 4];
            rgba[channel] = value;
            assert_eq!(
                GradientStop::new(0.0, Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3])),
                Err(LinearGradientError::InvalidColor)
            );
        }
    }
    assert_eq!(
        LinearGradient::new(
            [0.0; 2],
            [0.0; 2],
            [
                stop(0.0, Color::rgb(1.0, 0.0, 0.0)),
                stop(1.0, Color::rgb(0.0, 0.0, 1.0))
            ]
        ),
        Err(LinearGradientError::DegenerateAxis)
    );
    for stops in [vec![], vec![stop(0.0, Color::rgb(1.0, 0.0, 0.0))]] {
        assert_eq!(
            LinearGradient::new([0.0; 2], [1.0; 2], stops),
            Err(LinearGradientError::TooFewStops)
        );
    }
    assert_eq!(
        LinearGradient::new(
            [0.0; 2],
            [1.0; 2],
            [
                stop(0.8, Color::rgb(1.0, 0.0, 0.0)),
                stop(0.2, Color::rgb(0.0, 0.0, 1.0))
            ]
        ),
        Err(LinearGradientError::UnorderedStops)
    );
    for invalid in [f64::NAN, f64::INFINITY, -f64::INFINITY] {
        assert_eq!(
            paint([0.0; 2], [1.0; 2]).sample_premultiplied([invalid, 0.0]),
            Err(LinearGradientError::InvalidSamplePoint)
        );
    }
}

#[test]
fn sampling_is_history_free_and_cloning_shares_immutable_stops() {
    let gradient = paint([0.0; 2], [1.0; 2]);
    let copy = gradient.clone();
    assert!(Arc::ptr_eq(&gradient.stops, &copy.stops));
    for x in [0.7, 0.0, 1.0, 0.2, 0.7, 0.4] {
        assert_eq!(
            gradient.sample_premultiplied([x, x]),
            copy.sample_premultiplied([x, x])
        );
    }
    assert_eq!(gradient.start(), [0.0; 2]);
    assert_eq!(gradient.end(), [1.0; 2]);
    assert_eq!(gradient.stops().len(), 2);
}
