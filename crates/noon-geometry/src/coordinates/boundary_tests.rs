//! Range magnitudes and exact admission limits must not change plotted geometry.
use super::*;

fn near(actual: [f64; 2], expected: [f64; 2]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 1.0e-12,
            "{actual} != {expected}"
        );
    }
}

#[test]
fn adjacent_large_positive_ranges_keep_their_lower_left_corner() {
    let low = 1.0e16;
    let high = low + 2.0;
    // Placement must not require a representable absolute data midpoint.
    assert_eq!(low + (high - low) * 0.5, low);
    let frame = AxesFrame::centered([low, high, 2.0], [low, high, 2.0], 8.0, 4.0).unwrap();
    near(frame.x().start(), [-4.0, -2.0]);
    near(frame.y().start(), [-4.0, -2.0]);
    near(frame.coords_to_point(low, low).unwrap(), [-4.0, -2.0]);
    near(frame.coords_to_point(high, high).unwrap(), [4.0, 2.0]);
    assert_eq!(frame.point_to_coords([-4.0, -2.0]).unwrap(), [low, low]);
    assert_eq!(frame.point_to_coords([4.0, 2.0]).unwrap(), [high, high]);
}

#[test]
fn adjacent_large_negative_ranges_keep_their_upper_right_corner() {
    let high = -1.0e16;
    let low = high - 2.0;
    let frame = AxesFrame::centered([low, high, 2.0], [low, high, 2.0], 8.0, 4.0).unwrap();
    near(frame.x().end(), [4.0, 2.0]);
    near(frame.y().end(), [4.0, 2.0]);
    near(frame.coords_to_point(low, low).unwrap(), [-4.0, -2.0]);
    near(frame.coords_to_point(high, high).unwrap(), [4.0, 2.0]);
}

#[test]
fn independently_offset_ranges_map_all_four_corners() {
    let ranges = [
        [1.0e16, 1.0e16 + 2.0, 2.0],
        [-1.0e16 - 2.0, -1.0e16, 2.0],
        [-1.0, 3.0, 1.0],
    ];
    for x in ranges {
        for y in ranges {
            let frame = AxesFrame::centered(x, y, 8.0, 4.0).unwrap();
            near(frame.coords_to_point(x[0], y[0]).unwrap(), [-4.0, -2.0]);
            near(frame.coords_to_point(x[0], y[1]).unwrap(), [-4.0, 2.0]);
            near(frame.coords_to_point(x[1], y[0]).unwrap(), [4.0, -2.0]);
            near(frame.coords_to_point(x[1], y[1]).unwrap(), [4.0, 2.0]);
        }
    }
}

#[test]
fn exact_tick_capacity_counts_the_shared_origin_once() {
    let ticks = number_line_tick_values([-2.0, 2.0, 1.0], false, false, 5).unwrap();
    assert_eq!(ticks, vec![-2.0, -1.0, 0.0, 1.0, 2.0]);
    assert_eq!(
        number_line_tick_values([-2.0, 2.0, 1.0], false, false, 4),
        Err(CoordinateError::TickLimitExceeded)
    );
}

#[test]
fn one_origin_tick_fits_one_slot_but_not_zero() {
    assert_eq!(
        number_line_tick_values([0.0, 0.5, 1.0], false, false, 1).unwrap(),
        vec![0.0]
    );
    assert_eq!(
        number_line_tick_values([0.0, 0.5, 1.0], false, false, 0),
        Err(CoordinateError::TickLimitExceeded)
    );
    assert!(number_line_tick_values([0.0, 0.5, 1.0], false, true, 0)
        .unwrap()
        .is_empty());
}

#[test]
fn exact_capacity_preserves_endpoint_and_origin_policies() {
    for range in [
        [-2.0, 2.0, 1.0],
        [-2.0, 0.0, 1.0],
        [0.0, 2.0, 1.0],
        [2.0, 4.0, 0.75],
        [-4.0, -2.0, 0.75],
        [-1.0, 1.0, 0.3],
    ] {
        for include_tip in [false, true] {
            for exclude_origin in [false, true] {
                let expected =
                    number_line_tick_values(range, include_tip, exclude_origin, 100).unwrap();
                let count = expected.len();
                assert_eq!(
                    number_line_tick_values(range, include_tip, exclude_origin, count).unwrap(),
                    expected
                );
                if count > 0 {
                    assert_eq!(
                        number_line_tick_values(range, include_tip, exclude_origin, count - 1),
                        Err(CoordinateError::TickLimitExceeded)
                    );
                }
            }
        }
    }
}
