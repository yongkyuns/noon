//! Cursor placement must not require a representable absolute data midpoint.
use super::*;
use crate::synchronized_plot_presentation::{
    GappedTimeSeriesPlan, SynchronizedTimeSeriesPlan,
};

fn ranges() -> [[f64; 3]; 2] {
    [
        [1.0e16, 1.0e16 + 2.0, 2.0],
        [-1.0e16 - 2.0, -1.0e16, 2.0],
    ]
}

fn data(range: [f64; 3]) -> [TimedPlotSample; 3] {
    [
        TimedPlotSample {
            time: 0.0,
            value: range[0],
        },
        TimedPlotSample {
            time: 1.0,
            value: range[1],
        },
        TimedPlotSample {
            time: 2.0,
            value: range[0],
        },
    ]
}

#[test]
fn large_offset_cursor_centers_stay_in_the_plot_rectangle() {
    for range in ranges() {
        let frame = AxesFrame::centered([0.0, 2.0, 1.0], range, 8.0, 4.0).unwrap();
        let plan = TimeSeriesPlan::new(frame, &data(range), 2.0).unwrap();
        let centers = [[-4.0, 0.0], [0.0, 0.0], [4.0, 0.0]];
        let points = [[-4.0, -2.0], [0.0, 2.0], [4.0, -2.0]];
        assert_eq!(plan.cursor_points(), centers);
        assert_eq!(plan.points(), points);
        assert_eq!(plan.durations(), [1.0, 1.0]);
    }
}

#[test]
fn rotated_cursor_centers_use_the_supplied_world_frame() {
    for range in ranges() {
        // Rotated and translated rectangle: time increases upwards, while the
        // measurement axis spans x=5 to x=1. Its center is x=3 for either range.
        let x = if range[0] > 0.0 { 5.0 } else { 1.0 };
        let frame = AxesFrame::new(
            NumberLineFrame::new([0.0, 2.0, 1.0], [x, -4.0], [x, 4.0]).unwrap(),
            NumberLineFrame::new(range, [5.0, -4.0], [1.0, -4.0]).unwrap(),
        );
        let plan = TimeSeriesPlan::new(frame, &data(range), 2.0).unwrap();
        let centers = [[3.0, -4.0], [3.0, 0.0], [3.0, 4.0]];
        assert_eq!(plan.cursor_points(), centers);
    }
}

#[test]
fn synchronized_and_gapped_plans_share_the_correct_cursor_centers() {
    for range in ranges() {
        let frame = AxesFrame::centered([0.0, 2.0, 1.0], range, 8.0, 4.0).unwrap();
        let first = data(range);
        let mut second = first;
        second[1].time = 0.5;
        let rows: [&[TimedPlotSample]; 2] = [&first, &second];
        let synchronized =
            SynchronizedTimeSeriesPlan::new(frame, &rows, [0.0, 2.0], 2.0).unwrap();
        let breaks: [&[usize]; 2] = [&[0], &[]];
        let gapped =
            GappedTimeSeriesPlan::new(frame, &rows, &breaks, [0.0, 2.0], 2.0).unwrap();
        let centers = [[-4.0, 0.0], [-2.0, 0.0], [0.0, 0.0], [4.0, 0.0]];
        assert_eq!(synchronized.cursor_points(), centers);
        assert_eq!(gapped.cursor_points(), centers);
        assert_eq!(gapped.key_times(), synchronized.key_times());
        assert!(gapped.series()[0].points()[1].is_none());
        assert!(gapped.series()[1].points()[1].is_some());
        assert!(gapped.series()[0].segments()[0].is_none());
    }
}
