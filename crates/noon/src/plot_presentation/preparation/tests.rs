use super::*;

fn frame() -> AxesFrame {
    AxesFrame::centered([0.0, 10.0, 2.0], [0.0, 2.0, 0.5], 10.0, 4.0).unwrap()
}

fn samples() -> [TimedPlotSample; 3] {
    [
        TimedPlotSample {
            time: 0.0,
            value: 0.0,
        },
        TimedPlotSample {
            time: 1.0,
            value: 2.0,
        },
        TimedPlotSample {
            time: 10.0,
            value: 1.0,
        },
    ]
}

fn near(a: [f64; 2], b: [f64; 2]) {
    assert!((a[0] - b[0]).abs() < 1e-10, "{a:?} != {b:?}");
    assert!((a[1] - b[1]).abs() < 1e-10, "{a:?} != {b:?}");
}

#[test]
fn tick_labels_use_the_shared_range_and_real_numeric_values() {
    let labels = number_labels(frame().x(), None, 0, true).unwrap();
    assert_eq!(
        labels.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(),
        ["2", "4", "6", "8", "10"]
    );
    near(labels[0].point, [-3.0, -2.0]);
    near(labels[4].point, [5.0, -2.0]);
}

#[test]
fn rotated_labels_follow_the_captured_shaft_and_keep_input_order() {
    let line = NumberLineFrame::new([-1.0, 1.0, 0.5], [2.0, -1.0], [2.0, 3.0]).unwrap();
    let labels = number_labels(line, Some(&[1.0, -0.5, 1.0]), 1, false).unwrap();
    assert_eq!(labels[0].text, "1.0");
    assert_eq!(labels[1].text, "-0.5");
    assert_eq!(labels[0], labels[2]);
    near(labels[1].point, [2.0, 0.0]);
}

#[test]
fn formatting_removes_negative_zero_after_rounding_not_before() {
    let labels = number_labels(frame().x(), Some(&[-0.0, -0.004, -0.006]), 2, false).unwrap();
    assert_eq!(
        labels.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(),
        ["0.00", "0.00", "-0.01"]
    );
}

#[test]
fn label_preparation_rejects_nonfinite_values_precision_and_budget() {
    assert!(number_labels(frame().x(), Some(&[1.0, f64::NAN]), 1, false).is_err());
    assert!(number_labels(frame().x(), None, 13, false).is_err());
    assert!(number_labels(frame().x(), Some(&[1.0; MAX_PLOT_LABELS + 1]), 0, false).is_err());
    assert!(number_labels(frame().x(), Some(&[]), 0, false)
        .unwrap()
        .is_empty());
}

#[test]
fn uneven_timestamps_drive_durations_instead_of_sample_count_or_distance() {
    let plan = TimeSeriesPlan::new(frame(), &samples(), 5.0).unwrap();
    assert_eq!(plan.key_times(), [0.0, 0.5, 5.0]);
    assert_eq!(plan.durations(), [0.5, 4.5]);
    near(
        plan.point_at(0.5).unwrap(),
        frame().coords_to_point(1.0, 2.0).unwrap(),
    );
    near(
        plan.point_at(2.75).unwrap(),
        frame().coords_to_point(5.5, 1.5).unwrap(),
    );
    assert_eq!(plan.points().len(), samples().len());
}

#[test]
fn cursor_centers_follow_timestamp_without_value_dependent_speed() {
    let plan = TimeSeriesPlan::new(frame(), &samples(), 5.0).unwrap();
    near(plan.cursor_points()[0], [-5.0, 0.0]);
    near(plan.cursor_points()[1], [-4.0, 0.0]);
    near(plan.cursor_points()[2], [5.0, 0.0]);
}

#[test]
fn exact_endpoints_and_history_independent_observations() {
    let plan = TimeSeriesPlan::new(frame(), &samples(), 3.7).unwrap();
    assert_eq!(plan.key_times()[0], 0.0);
    assert_eq!(plan.run_time(), 3.7);
    assert_eq!(plan.point_at(0.0).unwrap(), plan.points()[0]);
    assert_eq!(plan.point_at(3.7).unwrap(), plan.points()[2]);
    let direct = plan.point_at(2.4).unwrap();
    for i in 0..24 {
        plan.point_at(i as f64 / 10.0).unwrap();
    }
    assert_eq!(direct, plan.point_at(2.4).unwrap());
    for invalid in [-1.0, 3.8, f64::NAN, f64::INFINITY] {
        assert!(plan.point_at(invalid).is_err());
    }
}

#[test]
fn nonzero_data_epoch_does_not_change_relative_playback_intervals() {
    let mut data = samples();
    for sample in &mut data {
        sample.time += 100.0;
    }
    let shifted_frame =
        AxesFrame::centered([100.0, 110.0, 2.0], [0.0, 2.0, 0.5], 10.0, 4.0).unwrap();
    let shifted = TimeSeriesPlan::new(shifted_frame, &data, 5.0).unwrap();
    let original = TimeSeriesPlan::new(frame(), &samples(), 5.0).unwrap();
    assert_eq!(shifted.key_times(), original.key_times());
    assert_eq!(shifted.points(), original.points());
}

#[test]
fn invalid_data_fails_before_returning_any_presentation() {
    for data in [
        vec![],
        vec![samples()[0]],
        vec![samples()[0], samples()[0]],
        vec![samples()[1], samples()[0]],
        vec![
            samples()[0],
            TimedPlotSample {
                time: f64::NAN,
                value: 1.0,
            },
        ],
        vec![
            samples()[0],
            TimedPlotSample {
                time: 1.0,
                value: f64::INFINITY,
            },
        ],
        vec![
            TimedPlotSample {
                time: -f64::MAX,
                value: 0.0,
            },
            TimedPlotSample {
                time: f64::MAX,
                value: 1.0,
            },
        ],
    ] {
        assert!(TimeSeriesPlan::new(frame(), &data, 5.0).is_err());
    }
    for duration in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        assert!(TimeSeriesPlan::new(frame(), &samples(), duration).is_err());
    }
}

#[test]
fn unrepresentable_timing_is_rejected_rather_than_dropping_samples() {
    let data = [
        TimedPlotSample {
            time: 0.0,
            value: 0.0,
        },
        TimedPlotSample {
            time: f64::MIN_POSITIVE,
            value: 1.0,
        },
        TimedPlotSample {
            time: 10.0,
            value: 2.0,
        },
    ];
    assert!(TimeSeriesPlan::new(frame(), &data, f64::MIN_POSITIVE).is_err());
}
