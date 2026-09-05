use noon::legacy::{Circle, IntoSnapshot, Scene, GREEN, PINK, RED};

#[test]
fn shape_set_fill_keeps_stroke_and_object_opacity_independent() {
    let snapshot = Circle::default()
        .set_stroke(Some(GREEN), Some(4.0))
        .set_fill(Some(RED), Some(0.25))
        .into_snapshot();

    let fill = snapshot.style.fill.expect("circle fill remains enabled");
    assert_eq!(fill.red, RED.red);
    assert_eq!(fill.green, RED.green);
    assert_eq!(fill.blue, RED.blue);
    assert_eq!(fill.alpha, 0.25);
    assert_eq!(snapshot.style.stroke, Some(GREEN));
    assert_eq!(snapshot.style.opacity, 1.0);
}

#[test]
fn animate_set_fill_keeps_stroke_and_object_opacity_independent() {
    let mut scene = Scene::new();
    let circle = scene.add(
        Circle::default()
            .set_stroke(Some(GREEN), Some(4.0))
            .set_fill(Some(RED), Some(0.25)),
    );

    scene
        .play(circle.animate().set_fill(Some(PINK), Some(0.5)))
        .run_time(1.0)
        .unwrap();

    let target = scene.snapshot(circle).unwrap();
    let fill = target.style.fill.expect("animated target keeps a fill");
    assert_eq!(fill.red, PINK.red);
    assert_eq!(fill.green, PINK.green);
    assert_eq!(fill.blue, PINK.blue);
    assert_eq!(fill.alpha, 0.5);
    assert_eq!(target.style.stroke, Some(GREEN));
    assert_eq!(target.style.opacity, 1.0);
}
