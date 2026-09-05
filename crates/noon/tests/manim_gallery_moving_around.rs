use noon::legacy::prelude::*;
use noon_core::{Property, RateFunction};

#[test]
fn moving_around_is_four_sequential_target_state_transforms() {
    let mut scene = Scene::new();
    let square = scene.add(
        Square::default()
            .color(BLUE)
            .set_fill(Some(BLUE), Some(1.0)),
    );

    scene
        .play(square.animate().shift(LEFT))
        .run_time(1.0)
        .unwrap();
    scene
        .play(square.animate().set_fill(Some(ORANGE), None))
        .run_time(1.0)
        .unwrap();
    scene
        .play(square.animate().scale(0.3))
        .run_time(1.0)
        .unwrap();
    scene
        .play(square.animate().rotate(0.4))
        .run_time(1.0)
        .unwrap();

    assert_eq!(scene.time(), 4.0);
    assert_eq!(scene.definition().objects().len(), 1);
    let tracks = scene.definition().tracks();
    assert_eq!(tracks.len(), 4);
    for (index, track) in tracks.iter().enumerate() {
        assert_eq!(track.property, Property::Transform);
        assert_eq!(track.timing.start_time, index as f64);
        assert_eq!(track.timing.duration, 1.0);
        assert_eq!(track.timing.easing, RateFunction::Smooth);
    }

    let final_state = scene.snapshot(square).unwrap();
    assert_eq!(final_state.transform.translation, LEFT);
    assert_eq!(final_state.transform.scale, Vec2::new(0.3, 0.3));
    assert!((final_state.transform.rotation - 0.4).abs() < 1e-6);
    assert_eq!(final_state.style.fill, Some(ORANGE));
}
