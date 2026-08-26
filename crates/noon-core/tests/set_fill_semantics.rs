use noon_core::{Color, GeometryRef, ObjectSnapshot, BLUE, GREEN, PINK, RED};

fn assert_rgb_eq(actual: Color, expected: Color) {
    assert_eq!(actual.red, expected.red);
    assert_eq!(actual.green, expected.green);
    assert_eq!(actual.blue, expected.blue);
}

#[test]
fn set_fill_color_preserves_existing_fill_opacity() {
    let mut snapshot = ObjectSnapshot::new(GeometryRef::square(1.0));
    snapshot.style.fill = Some(Color::rgba(RED.red, RED.green, RED.blue, 0.25));
    snapshot.style.stroke = Some(Color::rgba(GREEN.red, GREEN.green, GREEN.blue, 0.75));
    snapshot.style.opacity = 0.8;

    let next = snapshot.clone().set_fill(Some(BLUE), None);
    let fill = next.style.fill.expect("fill remains enabled");
    assert_rgb_eq(fill, BLUE);
    assert_eq!(fill.alpha, 0.25);
    assert_eq!(next.style.stroke, snapshot.style.stroke);
    assert_eq!(next.style.opacity, 0.8);
}

#[test]
fn set_fill_opacity_preserves_fill_color_stroke_and_object_opacity() {
    let mut snapshot = ObjectSnapshot::new(GeometryRef::square(1.0));
    snapshot.style.fill = Some(Color::rgba(RED.red, RED.green, RED.blue, 0.25));
    snapshot.style.stroke = Some(Color::rgba(GREEN.red, GREEN.green, GREEN.blue, 0.75));
    snapshot.style.opacity = 0.8;

    let next = snapshot.clone().set_fill(None, Some(0.5));
    let fill = next.style.fill.expect("fill remains enabled");
    assert_rgb_eq(fill, RED);
    assert_eq!(fill.alpha, 0.5);
    assert_eq!(next.style.stroke, snapshot.style.stroke);
    assert_eq!(next.style.opacity, 0.8);
}

#[test]
fn set_fill_color_and_opacity_do_not_dim_stroke() {
    let mut snapshot = ObjectSnapshot::new(GeometryRef::square(1.0));
    snapshot.style.fill = Some(RED);
    snapshot.style.stroke = Some(Color::rgba(GREEN.red, GREEN.green, GREEN.blue, 0.65));
    snapshot.style.opacity = 0.7;

    let next = snapshot.clone().set_fill(Some(PINK), Some(0.4));
    let fill = next.style.fill.expect("fill remains enabled");
    assert_rgb_eq(fill, PINK);
    assert_eq!(fill.alpha, 0.4);
    assert_eq!(next.style.stroke, snapshot.style.stroke);
    assert_eq!(next.style.opacity, 0.7);
}

#[test]
fn set_fill_without_arguments_is_a_noop() {
    let mut snapshot = ObjectSnapshot::new(GeometryRef::square(1.0));
    snapshot.style.fill = Some(Color::rgba(RED.red, RED.green, RED.blue, 0.25));
    snapshot.style.stroke = Some(GREEN);
    snapshot.style.opacity = 0.6;

    assert_eq!(snapshot.clone().set_fill(None, None), snapshot);
}
