use super::*;

fn frame() -> AxesFrame {
    AxesFrame::centered([0.0, 10.0, 2.0], [0.0, 3.0, 1.0], 10.0, 3.0).unwrap()
}
fn samples(values: &[(f64, f64)]) -> Vec<TimedPlotSample> {
    values.iter().map(|&(time, value)| TimedPlotSample { time, value }).collect()
}
fn recordings() -> [Vec<TimedPlotSample>; 2] {
    [samples(&[(0.0, 0.4), (1.0, 0.8), (2.0, 1.2), (7.0, 2.2), (10.0, 1.8)]),
     samples(&[(0.0, 2.4), (4.0, 2.6), (6.0, 2.0), (9.0, 2.2), (10.0, 2.4)])]
}

#[test]
fn no_breaks_preserve_existing_dense_planning() {
    let rows = recordings();
    let series = [&rows[0][..], &rows[1][..]];
    let dense = SynchronizedTimeSeriesPlan::new(frame(), &series, [0.0, 10.0], 6.0).unwrap();
    let plan = GappedTimeSeriesPlan::new(frame(), &series, &[&[], &[]], [0.0, 10.0], 6.0).unwrap();
    assert_eq!(plan.key_times(), dense.key_times());
    assert_eq!(plan.durations(), dense.durations());
    assert_eq!(plan.cursor_points(), dense.cursor_points());
    for (row, original) in plan.series().iter().zip(dense.series()) {
        assert_eq!(row.points(), original.points().iter().copied().map(Some).collect::<Vec<_>>());
        for (segment, points) in row.segments().iter().zip(original.points().windows(2)) {
            assert_eq!(*segment, Some([points[0], points[1]]));
        }
    }
}

#[test]
fn breaks_mask_foreign_knots_but_preserve_measured_endpoints() {
    let rows = recordings();
    let plan = GappedTimeSeriesPlan::new(frame(), &[&rows[0], &rows[1]], &[&[2], &[]], [0.0, 10.0], 6.0).unwrap();
    assert_eq!(plan.data_times(), &[0.0, 1.0, 2.0, 4.0, 6.0, 7.0, 9.0, 10.0]);
    let row = &plan.series()[0];
    assert_eq!(row.points()[3..5], [None, None]);
    assert_eq!(row.points()[2], Some(frame().coords_to_point(2.0, 1.2).unwrap()));
    assert_eq!(row.points()[5], Some(frame().coords_to_point(7.0, 2.2).unwrap()));
    assert!(row.segments()[2..5].iter().all(Option::is_none));
    assert!(row.segments()[..2].iter().all(Option::is_some));
    assert!(row.segments()[5..].iter().all(Option::is_some));
    assert!(plan.series()[1].segments().iter().all(Option::is_some));
    assert_eq!(plan.run_time(), 6.0);
}

#[test]
fn two_known_endpoints_do_not_imply_a_connection() {
    let row = samples(&[(0.0, 1.0), (10.0, 2.0)]);
    let plan = GappedTimeSeriesPlan::new(frame(), &[&row], &[&[0]], [0.0, 10.0], 6.0).unwrap();
    assert!(plan.series()[0].points().iter().all(Option::is_some));
    assert_eq!(plan.series()[0].segments(), &[None]);
}

#[test]
fn a_window_entirely_inside_a_gap_has_no_measurements() {
    let row = samples(&[(0.0, 1.0), (10.0, 2.0)]);
    let plan = GappedTimeSeriesPlan::new(frame(), &[&row], &[&[0]], [2.0, 8.0], 3.0).unwrap();
    assert_eq!(plan.series()[0].points(), &[None, None]);
    assert_eq!(plan.series()[0].segments(), &[None]);
    assert_eq!(plan.data_times(), &[2.0, 8.0]);
    assert_eq!(plan.durations(), &[3.0]);
}

#[test]
fn consecutive_breaks_keep_an_isolated_measurement_without_lines() {
    let row = samples(&[(0.0, 0.0), (5.0, 1.0), (10.0, 2.0)]);
    let plan = GappedTimeSeriesPlan::new(frame(), &[&row], &[&[0, 1]], [0.0, 10.0], 6.0).unwrap();
    assert_eq!(plan.series()[0].points()[1], Some(frame().coords_to_point(5.0, 1.0).unwrap()));
    assert_eq!(plan.series()[0].segments(), &[None, None]);
}

#[test]
fn malformed_break_lists_fail_without_changing_the_source() {
    let rows = recordings();
    let original = rows.clone();
    for breaks in [vec![2, 2], vec![3, 1], vec![4], vec![usize::MAX]] {
        assert!(GappedTimeSeriesPlan::new(frame(), &[&rows[0]], &[&breaks], [0.0, 10.0], 6.0).is_err());
    }
    assert!(GappedTimeSeriesPlan::new(frame(), &[&rows[0]], &[], [0.0, 10.0], 6.0).is_err());
    assert_eq!(rows, original);
}

#[test]
fn gaps_do_not_allow_extrapolation_or_nonfinite_input() {
    for row in [samples(&[(1.0, 0.0), (10.0, 1.0)]), samples(&[(0.0, f64::NAN), (10.0, 1.0)])] {
        assert!(GappedTimeSeriesPlan::new(frame(), &[&row], &[&[0]], [0.0, 10.0], 6.0).is_err());
    }
}

#[test]
fn unrelated_breaks_outside_selected_window_do_not_disconnect_it() {
    let row = samples(&[(-2.0, 0.0), (-1.0, 1.0), (0.0, 0.0), (10.0, 2.0), (12.0, 1.0)]);
    let plan = GappedTimeSeriesPlan::new(frame(), &[&row], &[&[0, 3]], [0.0, 10.0], 6.0).unwrap();
    assert!(plan.series()[0].segments().iter().all(Option::is_some));
}

#[test]
fn gaps_cannot_bypass_the_expanded_grid_budget() {
    let rows: Vec<_> = (0..16).map(|offset| {
        let mut row = samples(&[(0.0, 0.0)]);
        row.extend((1..80).map(|i| TimedPlotSample { time: f64::from(i * 16 + offset), value: 1.0 }));
        row.push(TimedPlotSample { time: 2000.0, value: 0.0 });
        row
    }).collect();
    let refs: Vec<_> = rows.iter().map(Vec::as_slice).collect();
    let breaks: Vec<_> = rows.iter().map(|r| (0..r.len() - 1).collect::<Vec<_>>()).collect();
    let break_refs: Vec<_> = breaks.iter().map(Vec::as_slice).collect();
    assert!(GappedTimeSeriesPlan::new(frame(), &refs, &break_refs, [0.0, 2000.0], 6.0).is_err());
}
