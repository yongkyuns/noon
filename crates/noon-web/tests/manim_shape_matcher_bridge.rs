use noon::{IntoSnapshot, Rectangle, BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY};
use noon_core::{ObjectSnapshot, Vec2, BLACK};
use noon_web::{
    manim_background_rectangle_snapshots_json, manim_surrounding_rectangle_snapshot_json,
    manim_surrounding_rectangle_snapshots_json,
};

fn rectangle_json(width: f32, height: f32, center: Vec2) -> String {
    serde_json::to_string(&Rectangle::new(width, height).shift(center).into_snapshot())
        .expect("valid target snapshot")
}

fn decode(snapshot_json: &str) -> ObjectSnapshot {
    serde_json::from_str(snapshot_json).expect("valid matcher snapshot")
}

#[test]
fn public_variadic_surrounding_rectangle_bridge_preserves_union_semantics() {
    let targets = vec![
        rectangle_json(2.0, 2.0, Vec2::new(-2.0, 0.0)),
        rectangle_json(4.0, 1.0, Vec2::new(3.0, 2.0)),
    ];

    let snapshot = decode(
        &manim_surrounding_rectangle_snapshots_json(&targets, 0.25, 0.5, 0.0)
            .expect("valid public variadic matcher"),
    );

    assert_eq!(snapshot.center(), Vec2::new(1.0, 0.75));
    assert!((snapshot.width() - 8.5).abs() <= 1e-5);
    assert!((snapshot.height() - 4.5).abs() <= 1e-5);
}

#[test]
fn public_variadic_background_rectangle_bridge_preserves_union_style() {
    let targets = vec![
        rectangle_json(1.0, 2.0, Vec2::new(-1.0, -1.0)),
        rectangle_json(3.0, 1.0, Vec2::new(2.0, 2.0)),
    ];

    let snapshot = decode(
        &manim_background_rectangle_snapshots_json(
            &targets,
            0.0,
            0.0,
            0.0,
            f64::from(BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY),
        )
        .expect("valid public variadic background"),
    );

    assert_eq!(snapshot.center(), Vec2::new(1.0, 0.25));
    assert!((snapshot.width() - 5.0).abs() <= 1e-5);
    assert!((snapshot.height() - 4.5).abs() <= 1e-5);
    let fill = snapshot.style.fill.expect("background fill");
    assert_eq!(
        (fill.red, fill.green, fill.blue),
        (BLACK.red, BLACK.green, BLACK.blue)
    );
    assert!((fill.alpha - BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY).abs() <= 1e-5);
    assert_eq!(snapshot.style.stroke_width, 0.0);
}

#[test]
fn public_single_target_wrapper_matches_variadic_bridge() {
    let target = rectangle_json(4.0, 2.0, Vec2::new(1.0, -2.0));
    let single = decode(
        &manim_surrounding_rectangle_snapshot_json(&target, 0.25, 0.5, 0.1)
            .expect("valid single-target matcher"),
    );
    let variadic = decode(
        &manim_surrounding_rectangle_snapshots_json(&[target], 0.25, 0.5, 0.1)
            .expect("valid one-member variadic matcher"),
    );

    assert_eq!(single, variadic);
}

#[test]
fn public_variadic_bridge_rejects_incomplete_target_sets() {
    assert!(manim_surrounding_rectangle_snapshots_json(&[], 0.1, 0.1, 0.0).is_err());

    let targets = vec![
        rectangle_json(2.0, 1.0, Vec2::ZERO),
        "not a snapshot".to_owned(),
    ];
    assert!(manim_background_rectangle_snapshots_json(
        &targets,
        0.0,
        0.0,
        0.0,
        f64::from(BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY),
    )
    .is_err());
}
