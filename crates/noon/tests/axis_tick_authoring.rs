use noon::{
    Axes2DState, NumberLineGeometryPlan, NumberLineState, NumberLineTickOptions, NumberRange,
};
use noon_core::{GeometryRef, Vec2, GREEN};

fn endpoints(snapshot: &noon_core::ObjectSnapshot) -> (Vec2, Vec2) {
    let GeometryRef::Line { start, end } = snapshot.geometry else {
        panic!("expected retained line geometry");
    };
    (start, end)
}

#[test]
fn default_ticks_are_independent_retained_lines() {
    let state =
        NumberLineState::centered(NumberRange::new(-2.0, 2.0, 1.0).unwrap(), 4.0, 0.0).unwrap();
    let plan = NumberLineGeometryPlan::new(state, &NumberLineTickOptions::default()).unwrap();
    assert_eq!(plan.ticks().len(), 5);
    let origin = &plan.ticks()[2];
    let (start, end) = endpoints(origin.snapshot());
    assert_eq!(origin.value(), 0.0);
    assert!((start.y + 0.1).abs() <= 1.0e-6);
    assert!((end.y - 0.1).abs() <= 1.0e-6);
}

#[test]
fn canonical_plot_axis_excludes_origin_and_elongates_requested_ticks() {
    let axes = Axes2DState::new(
        NumberRange::new(-10.0, 10.3, 1.0).unwrap(),
        NumberRange::new(-1.5, 1.5, 1.0).unwrap(),
        10.0,
        6.0,
    )
    .unwrap();
    let options = NumberLineTickOptions {
        elongated_values: (-5..=5).map(|index| f64::from(index * 2)).collect(),
        exclude_origin_tick: true,
        color: GREEN,
        ..NumberLineTickOptions::default()
    };
    let plan = NumberLineGeometryPlan::new(axes.x_axis(), &options).unwrap();
    assert_eq!(plan.ticks().len(), 20);
    assert!(plan.ticks().iter().all(|tick| tick.value() != 0.0));
    assert_eq!(
        plan.ticks()
            .iter()
            .find(|tick| tick.value() == 2.0)
            .unwrap()
            .size(),
        0.2
    );
    assert_eq!(
        plan.ticks()
            .iter()
            .find(|tick| tick.value() == 1.0)
            .unwrap()
            .size(),
        0.1
    );
    assert!((plan.line().style.stroke_width - 0.02).abs() <= 1.0e-6);
}

#[test]
fn vertical_axis_ticks_rotate_with_axis_geometry() {
    let axes = Axes2DState::new(
        NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
        NumberRange::new(-2.0, 2.0, 1.0).unwrap(),
        4.0,
        4.0,
    )
    .unwrap();
    let options = NumberLineTickOptions {
        exclude_origin_tick: true,
        ..NumberLineTickOptions::default()
    };
    let plan = NumberLineGeometryPlan::new(axes.y_axis(), &options).unwrap();
    let tick = plan
        .ticks()
        .iter()
        .find(|tick| tick.value() == 1.0)
        .unwrap();
    let (start, end) = endpoints(tick.snapshot());
    assert!((start.y - end.y).abs() <= 1.0e-6);
    assert!(((end.x - start.x).abs() - 0.2).abs() <= 1.0e-6);
}

#[test]
fn disabling_ticks_keeps_axis_line_without_hidden_tick_geometry() {
    let state =
        NumberLineState::centered(NumberRange::new(0.0, 2.0, 1.0).unwrap(), 2.0, 0.0).unwrap();
    let options = NumberLineTickOptions {
        include_ticks: false,
        ..NumberLineTickOptions::default()
    };
    let plan = NumberLineGeometryPlan::new(state, &options).unwrap();
    assert!(plan.ticks().is_empty());
}
