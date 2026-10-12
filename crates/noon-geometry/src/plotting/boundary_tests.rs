//! Empty and one-sample spans need no representable next parameter.
use super::*;

#[test]
fn nonempty_underflowed_intervals_keep_start_and_exact_end() {
    for range in [
        [0.0, 1.0e-300, 1.0e100],
        [-1.0e-300, 0.0, 1.0e100],
        [0.0, f64::from_bits(1), 2.0],
    ] {
        assert_eq!((range[1] - range[0]) / range[2], 0.0);
        let plan = PlotSamplingOptions::parametric(&range)
            .unwrap()
            .plan()
            .unwrap();
        assert_eq!(plan.parameters(), &[range[0], range[1]]);
        assert_eq!(plan.subpaths().len(), 1);
        assert_eq!(plan.subpaths()[0], 0..2);
    }
}

#[test]
fn underflowed_intervals_charge_both_samples_before_evaluation() {
    let mut options = PlotSamplingOptions::parametric(&[0.0, 1.0e-300, 1.0e100]).unwrap();
    options.max_samples = 1;
    assert_eq!(
        options.plan(),
        Err(PlotPreparationError::SampleLimitExceeded)
    );
    options.max_samples = 2;
    assert_eq!(options.plan().unwrap().parameters(), &[0.0, 1.0e-300]);
}

#[test]
fn single_regular_sample_does_not_require_a_finite_next_increment() {
    let start: f64 = 1.0e308;
    let end = 1.25e308;
    let step = 1.0e308;
    assert!(!(start + step).is_finite());
    let plan = PlotSamplingOptions::parametric(&[start, end, step])
        .unwrap()
        .plan()
        .unwrap();
    assert_eq!(plan.parameters(), &[start, end]);
    let path = plan
        .evaluate(|t| [(t - start) / (end - start), 0.0], false)
        .unwrap();
    assert_eq!(path.commands().len(), 2);
    assert!(path.is_finite());
}

#[test]
fn empty_discontinuity_span_emits_only_its_exact_endpoint() {
    let start: f64 = 1.0e308;
    let end = 1.25e308;
    let mut options = PlotSamplingOptions::parametric(&[start, end, 1.0e308]).unwrap();
    options.discontinuities = vec![start];
    options.dt = 0.0;
    options.max_samples = 3;
    let plan = options.plan().unwrap();
    assert_eq!(plan.parameters(), &[start, start, end]);
    assert_eq!(plan.subpaths().len(), 2);
    assert_eq!(plan.subpaths()[0], 0..1);
    assert_eq!(plan.subpaths()[1], 1..3);
    options.max_samples = 2;
    assert_eq!(
        options.plan(),
        Err(PlotPreparationError::SampleLimitExceeded)
    );
}

#[test]
fn underflowed_discontinuous_spans_keep_all_four_endpoints() {
    let mut options = PlotSamplingOptions::parametric(&[0.0, 1.0e-300, 1.0e100]).unwrap();
    let middle = 0.5e-300;
    options.discontinuities = vec![middle];
    options.dt = 0.125e-300;
    options.max_samples = 4;
    let plan = options.plan().unwrap();
    assert_eq!(
        plan.parameters(),
        &[0.0, middle - options.dt, middle + options.dt, 1.0e-300]
    );
    assert_eq!(plan.subpaths().len(), 2);
    assert_eq!(plan.subpaths()[0], 0..2);
    assert_eq!(plan.subpaths()[1], 2..4);
    options.max_samples = 3;
    assert_eq!(
        options.plan(),
        Err(PlotPreparationError::SampleLimitExceeded)
    );
}

#[test]
fn multi_sample_spans_keep_the_representable_increment_rule() {
    let start = 0.1;
    let step = 0.2;
    let increment = (start + step) - start;
    let plan = PlotSamplingOptions::parametric(&[start, 1.0, step])
        .unwrap()
        .plan()
        .unwrap();
    let expected: Vec<_> = (0..5)
        .map(|index| start + f64::from(index) * increment)
        .chain(std::iter::once(1.0))
        .collect();
    assert_eq!(plan.parameters(), expected);
}
