//! Geometry invariants behind the pinned Manim arrow-tip raster comparison.
//! Raster coverage is tested separately: correct triangles do not prove smooth pixels.
use std::rc::Rc;

use noon::{ManimArrow, ManimArrowOptions, Mobject, Scene, StrokeCap};
use noon_core::{SemanticObjectContent, StoredGeometry};

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 2e-6, "{actual} != {expected}");
}

fn point_close(actual: (f64, f64), expected: (f64, f64)) {
    close(actual.0, expected.0);
    close(actual.1, expected.1);
}

fn tip_points(tip: &Mobject) -> Vec<(f64, f64)> {
    let query = tip.path_query().unwrap();
    assert!(query.is_closed().unwrap());
    assert_eq!(query.curve_count(), 3);
    let points = query.start_anchors();
    assert_eq!(points.len(), 3);
    assert!(points.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
    let style = tip.state().unwrap().style;
    assert!(style.fill.is_some());
    assert_eq!(style.fill_opacity, 1.0);
    assert_eq!(
        style.stroke_width, 0.0,
        "filled tips must not acquire a jagged outline"
    );
    points
}

fn assert_tip(tip: &Mobject, apex: (f64, f64), direction: (f64, f64), length: f64) {
    let points = tip_points(tip);
    point_close(points[0], apex);
    let base = (
        (points[1].0 + points[2].0) / 2.0,
        (points[1].1 + points[2].1) / 2.0,
    );
    point_close(
        base,
        (apex.0 - direction.0 * length, apex.1 - direction.1 * length),
    );
    close((points[0].0 - base.0).hypot(points[0].1 - base.1), length);
    let width = (points[1].0 - points[2].0).hypot(points[1].1 - points[2].1);
    close(width, length);
    close(
        (points[1].0 - points[2].0) * direction.0 + (points[1].1 - points[2].1) * direction.1,
        0.0,
    );
    let a = (points[1].0 - apex.0, points[1].1 - apex.1);
    let b = (points[2].0 - apex.0, points[2].1 - apex.1);
    // Manim's end and start triangle paths are both counter-clockwise. Preserve
    // that observable order: a reversed path has the same coordinates but can
    // produce different edge coverage in a retained renderer.
    close(a.0 * b.1 - a.1 * b.0, length * length);
}

fn shaft_endpoints(arrow: &ManimArrow) -> ((f64, f64), (f64, f64)) {
    let state = arrow.shaft().state().unwrap();
    let SemanticObjectContent::Geometry(StoredGeometry::Line { start, end }) = state.content else {
        panic!("arrow shaft must remain an analytic line");
    };
    (
        (f64::from(start.x), f64::from(start.y)),
        (f64::from(end.x), f64::from(end.y)),
    )
}

#[test]
fn rotated_and_small_tips_are_closed_symmetric_triangles_attached_to_the_shaft() {
    for degrees in [0.0_f64, 7.0, 33.0, 90.0, 173.0, 287.0] {
        for tip_length in [0.08, 0.22, 0.35] {
            let scene = Scene::new();
            let (sin, cos) = degrees.to_radians().sin_cos();
            let end = (2.0 * cos, 2.0 * sin);
            let mut options = ManimArrowOptions::arrow(0.0, 0.0, end.0, end.1).unwrap();
            options.set_buff(0.0).unwrap();
            options.set_tip_length(tip_length).unwrap();
            let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
            assert_tip(arrow.end_tip(), end, (cos, sin), tip_length);
            let (start, shaft_end) = shaft_endpoints(&arrow);
            point_close(start, (0.0, 0.0));
            point_close(
                shaft_end,
                (end.0 - cos * tip_length, end.1 - sin * tip_length),
            );
        }
    }
}

#[test]
fn post_buff_length_caps_both_tip_dimensions_and_shaft_width() {
    let scene = Scene::new();
    let mut options = ManimArrowOptions::arrow(0.0, 0.0, 0.8, 0.0).unwrap();
    options.set_buff(0.25).unwrap();
    options.set_tip_length(0.35).unwrap();
    let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
    // Visible length = 0.3, so Manim's 1/4 cap gives length AND width 0.075.
    assert_tip(arrow.end_tip(), (0.55, 0.0), (1.0, 0.0), 0.075);
    point_close(shaft_endpoints(&arrow).1, (0.475, 0.0));
    close(arrow.shaft().state().unwrap().style.stroke_width, 0.015);
}

#[test]
fn shaft_thickness_does_not_change_tip_shape_or_add_tip_stroke() {
    let scene = Scene::new();
    let mut previous = None;
    for stroke_width in [0.01, 0.04, 0.09] {
        let mut options = ManimArrowOptions::arrow(-1.0, 0.0, 1.0, 0.0).unwrap();
        options.set_buff(0.0).unwrap();
        options.set_stroke_width(stroke_width).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
        let points = tip_points(arrow.end_tip());
        if let Some(expected) = &previous {
            assert_eq!(&points, expected);
        }
        previous = Some(points);
        close(
            arrow.shaft().state().unwrap().style.stroke_width,
            stroke_width,
        );
    }
}

#[test]
fn short_double_arrow_has_two_outward_tips_and_no_shaft_protrusion() {
    let scene = Scene::new();
    let mut options = ManimArrowOptions::double_arrow(-0.2, 0.0, 0.2, 0.0).unwrap();
    options.set_buff(0.0).unwrap();
    let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
    assert_tip(arrow.end_tip(), (0.2, 0.0), (1.0, 0.0), 0.1);
    assert_tip(arrow.start_tip().unwrap(), (-0.2, 0.0), (-1.0, 0.0), 0.1);
    let (start, end) = shaft_endpoints(&arrow);
    point_close(start, (-0.1, 0.0));
    point_close(end, (0.1, 0.0));
}

#[test]
fn double_arrow_reuses_the_same_outward_triangle_construction_in_both_directions() {
    // Manim constructs the end tip, shortens the Line to its base, then constructs
    // the start tip from that shortened line. Exercise both tangent directions and
    // a translated family so a start-tip coverage issue cannot be mistaken for a
    // different Rust-owned tip length, winding, or shaft-trimming rule.
    for (start, end, translation) in [
        ((-0.2, 0.0), (0.2, 0.0), (1.25, -0.75)),
        ((0.2, 0.0), (-0.2, 0.0), (-1.25, 0.75)),
        ((-0.2, 0.1), (0.2, -0.1), (0.5, -1.0)),
    ] {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::double_arrow(start.0, start.1, end.0, end.1).unwrap();
        options.set_buff(0.0).unwrap();
        options
            .set_translation(translation.0, translation.1)
            .unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();

        let dx = end.0 - start.0;
        let dy = end.1 - start.1;
        let length = dx.hypot(dy);
        let direction = (dx / length, dy / length);
        let tip_length = 0.25 * length;
        let world_start = (start.0 + translation.0, start.1 + translation.1);
        let world_end = (end.0 + translation.0, end.1 + translation.1);

        assert_tip(arrow.end_tip(), world_end, direction, tip_length);
        assert_tip(
            arrow.start_tip().unwrap(),
            world_start,
            (-direction.0, -direction.1),
            tip_length,
        );

        let shaft = arrow.shaft().manim_line_endpoints().unwrap();
        point_close(
            shaft.start,
            (
                world_start.0 + direction.0 * tip_length,
                world_start.1 + direction.1 * tip_length,
            ),
        );
        point_close(
            shaft.end,
            (
                world_end.0 - direction.0 * tip_length,
                world_end.1 - direction.1 * tip_length,
            ),
        );
        assert_eq!(
            arrow.shaft().state().unwrap().style.stroke_cap,
            StrokeCap::Butt
        );
        let endpoints = arrow.manim_endpoints().unwrap();
        point_close(endpoints.start, world_start);
        point_close(endpoints.end, world_end);
    }
}

#[test]
fn zero_length_arrow_has_finite_coincident_tip_anchors() {
    let scene = Scene::new();
    let options = ManimArrowOptions::arrow(1.0, 2.0, 1.0, 2.0).unwrap();
    let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
    // Do not ask a zero-length PathProportionPlan to invent nondegenerate curves.
    let state = arrow.end_tip().state().unwrap();
    assert!(state.transform.translation.x.is_finite());
    assert_eq!(state.style.stroke_width, 0.0);
    let query = arrow.end_tip().local_path_query().unwrap();
    point_close(query.start().unwrap(), (1.0, 2.0));
    point_close(query.end().unwrap(), (1.0, 2.0));
    let (start, end) = shaft_endpoints(&arrow);
    point_close(start, end);
    assert_eq!(arrow.shaft().state().unwrap().style.stroke_width, 0.0);
}
