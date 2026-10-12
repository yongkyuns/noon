use super::*;
use crate::{ManimAxesOptions, ManimRotationPivot, Scene};

fn samples(values: &[[f64; 2]]) -> Vec<TimedPlotSample> {
    values
        .iter()
        .map(|&[time, value]| TimedPlotSample { time, value })
        .collect()
}
fn frame() -> AxesFrame {
    AxesFrame::centered([0.0, 10.0, 1.0], [0.0, 10.0, 1.0], 10.0, 10.0).unwrap()
}
fn near(a: [f64; 2], b: [f64; 2]) {
    for (a, b) in a.into_iter().zip(b) {
        assert!((a - b).abs() < 2e-5, "{a} != {b}");
    }
}

#[test]
fn different_grids_share_data_time_not_sample_index() {
    let a = samples(&[[0.0, 0.0], [2.0, 2.0], [10.0, 10.0]]);
    let b = samples(&[[0.0, 10.0], [5.0, 5.0], [10.0, 0.0]]);
    let plan = SynchronizedTimeSeriesPlan::new(frame(), &[&a, &b], [0.0, 10.0], 6.0).unwrap();
    assert_eq!(plan.key_times(), &[0.0, (2.0 / 10.0) * 6.0, 3.0, 6.0]);
    assert_eq!(plan.series()[0].key_times(), plan.series()[1].key_times());
    assert_eq!(plan.series()[0].samples()[2].value, 5.0);
    assert_eq!(plan.series()[1].samples()[1].value, 8.0);
    near(plan.series()[0].point_at(1.8).unwrap(), [-2.0, -2.0]);
    near(plan.series()[1].point_at(1.8).unwrap(), [-2.0, 2.0]);
    assert_eq!(plan.run_time(), 6.0);
}

#[test]
fn window_clips_without_extrapolation_and_preserves_knots() {
    let a = samples(&[[-2.0, 0.0], [2.0, 4.0], [12.0, 4.0]]);
    let b = samples(&[[-1.0, 2.0], [3.0, 2.0], [11.0, 6.0]]);
    let plan = SynchronizedTimeSeriesPlan::new(frame(), &[&a, &b], [0.0, 10.0], 5.0).unwrap();
    assert_eq!(plan.key_times(), &[0.0, 1.0, 1.5, 5.0]);
    let a = plan.series()[0].samples();
    assert_eq!(
        a[0],
        TimedPlotSample {
            time: 0.0,
            value: 2.0
        }
    );
    assert_eq!(
        a[1],
        TimedPlotSample {
            time: 2.0,
            value: 4.0
        }
    );
    assert_eq!(plan.series()[1].samples().last().unwrap().value, 5.5);
}

#[test]
fn exact_duplicate_knots_are_shared_but_nearby_times_are_not_merged() {
    let next = f64::from_bits(1.0f64.to_bits() + 1);
    let a = samples(&[[0.0, 1.0], [1.0, 2.0], [2.0, 3.0]]);
    let b = samples(&[[0.0, 2.0], [next, 3.0], [2.0, 4.0]]);
    let plan = SynchronizedTimeSeriesPlan::new(frame(), &[&a, &b], [0.0, 2.0], 2.0).unwrap();
    assert_eq!(plan.key_times(), &[0.0, 1.0, next, 2.0]);
}

#[test]
fn singleton_series_agrees_with_existing_plan() {
    let data = samples(&[[0.0, 2.0], [0.5, 1.0], [10.0, 3.0]]);
    let plan = SynchronizedTimeSeriesPlan::new(frame(), &[&data], [0.0, 10.0], 6.0).unwrap();
    assert_eq!(
        plan.series()[0],
        TimeSeriesPlan::new(frame(), &data, 6.0).unwrap()
    );
}

#[test]
fn invalid_inputs_and_uncovered_windows_fail_without_publishing() {
    let scene = Scene::new();
    let revision = scene.revision();
    let valid = samples(&[[0.0, 1.0], [10.0, 2.0]]);
    for bad in [
        vec![],
        samples(&[[0.0, 1.0]]),
        samples(&[[1.0, 1.0], [1.0, 2.0]]),
        samples(&[[2.0, 1.0], [1.0, 2.0]]),
        samples(&[[0.0, 1.0], [10.0, f64::NAN]]),
        samples(&[[1.0, 1.0], [10.0, 2.0]]),
        samples(&[[0.0, 1.0], [9.0, 2.0]]),
    ] {
        assert!(
            SynchronizedTimeSeriesPlan::new(frame(), &[&valid, &bad], [0.0, 10.0], 6.0).is_err()
        );
    }
    for window in [
        [0.0, 0.0],
        [2.0, 1.0],
        [f64::NAN, 1.0],
        [-f64::MAX, f64::MAX],
    ] {
        assert!(SynchronizedTimeSeriesPlan::new(frame(), &[&valid], window, 6.0).is_err());
    }
    for duration in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        assert!(
            SynchronizedTimeSeriesPlan::new(frame(), &[&valid], [0.0, 10.0], duration).is_err()
        );
    }
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        0
    );
}

#[test]
fn product_budget_rejects_small_inputs_that_expand_too_far() {
    // 16 disjoint interior grids have only 832 input samples but 12832 outputs.
    let series: Vec<Vec<TimedPlotSample>> = (0..16)
        .map(|row| {
            let mut points = vec![TimedPlotSample {
                time: 0.0,
                value: 1.0,
            }];
            for i in 0..50 {
                points.push(TimedPlotSample {
                    time: f64::from(i * 16 + row + 1),
                    value: 1.0,
                });
            }
            points.push(TimedPlotSample {
                time: 1000.0,
                value: 1.0,
            });
            points
        })
        .collect();
    let refs: Vec<_> = series.iter().map(Vec::as_slice).collect();
    assert!(matches!(
        SynchronizedTimeSeriesPlan::new(frame(), &refs, [0.0, 1000.0], 1.0),
        Err(Error::InvalidInput(
            "synchronized expanded point limit exceeded"
        ))
    ));
}

#[test]
fn input_and_series_budgets_are_separate() {
    let data = samples(&[[0.0, 0.0], [1.0, 1.0]]);
    assert!(SynchronizedTimeSeriesPlan::new(frame(), &[], [0.0, 1.0], 1.0).is_err());
    assert!(
        SynchronizedTimeSeriesPlan::new(frame(), &[data.as_slice(); 17], [0.0, 1.0], 1.0).is_err()
    );
    let large = vec![data[0]; MAX_TIMED_PLOT_SAMPLES + 1];
    assert!(matches!(
        SynchronizedTimeSeriesPlan::new(frame(), &[&large], [0.0, 1.0], 1.0),
        Err(Error::InvalidInput(
            "synchronized input sample limit exceeded"
        ))
    ));
}

#[test]
fn large_epoch_offset_does_not_normalize_each_series_independently() {
    let epoch = 1_700_000_000.0;
    let a = samples(&[[epoch, 1.0], [epoch + 10.0, 3.0]]);
    let b = samples(&[[epoch - 2.0, 0.0], [epoch + 5.0, 7.0], [epoch + 12.0, 0.0]]);
    let axes =
        AxesFrame::centered([epoch, epoch + 10.0, 2.0], [0.0, 10.0, 1.0], 10.0, 4.0).unwrap();
    let plan =
        SynchronizedTimeSeriesPlan::new(axes, &[&a, &b], [epoch, epoch + 10.0], 6.0).unwrap();
    assert_eq!(plan.key_times(), &[0.0, 3.0, 6.0]);
    near(
        plan.series()[0].points()[1],
        axes.coords_to_point(epoch + 5.0, 2.0).unwrap(),
    );
    near(
        plan.series()[1].points()[1],
        axes.coords_to_point(epoch + 5.0, 7.0).unwrap(),
    );
}

#[test]
fn shared_snapshot_tracks_affine_axes_without_mutation_or_live_cache() {
    let mut scene = Scene::new();
    let axes = scene
        .axes(&ManimAxesOptions::new(
            [0.0, 10.0, 2.0],
            [0.0, 10.0, 2.0],
            10.0,
            4.0,
        ))
        .unwrap();
    axes.family().scale(1.5, 0.8).unwrap();
    axes.family()
        .rotate(0.7, ManimRotationPivot::Center)
        .unwrap();
    let snapshot = axes.authored_frame().unwrap();
    let data = samples(&[[0.0, 1.0], [5.0, 2.0], [10.0, 3.0]]);
    let revision = scene.revision();
    let plan = SynchronizedTimeSeriesPlan::new(snapshot, &[&data], [0.0, 10.0], 2.0).unwrap();
    near(
        plan.series()[0].points()[1],
        snapshot.coords_to_point(5.0, 2.0).unwrap(),
    );
    assert_eq!(scene.revision(), revision);
    axes.family().shift(2.0, 0.0).unwrap();
    assert_ne!(
        plan.series()[0].points()[1],
        axes.authored_frame()
            .unwrap()
            .coords_to_point(5.0, 2.0)
            .unwrap()
    );
}
