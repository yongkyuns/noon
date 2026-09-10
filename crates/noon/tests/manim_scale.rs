use noon::Scene;

#[test]
fn manim_scale_preserves_an_off_origin_geometry_center() {
    let scene = Scene::new();
    let mut line = scene.line((1.0, 1.0), (3.0, 1.0)).unwrap();
    let before = scene.revision();

    line.manim_scale(2.0, 3.0).unwrap();

    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert_eq!(line.center().unwrap(), (2.0, 1.0));
    assert!((line.width().unwrap() - 4.0).abs() < 1e-6);

    // Noon-native scale remains the origin-space primitive.
    let mut native = scene.line((1.0, 1.0), (3.0, 1.0)).unwrap();
    native.scale(2.0, 3.0).unwrap();
    assert_eq!(native.center().unwrap(), (4.0, 3.0));
}

#[test]
fn live_manim_scale_preserves_center_in_one_atomic_publication() {
    let mut scene = Scene::new();
    let line = scene.line((1.0, -2.0), (3.0, -2.0)).unwrap();
    scene.add(&line).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = scene.revision();

    {
        let mut live = scene.live(&mut session);
        live.manim_scale(&line, 2.0, 0.5).unwrap();
        let layout = live.effective_layout(&line).unwrap();
        assert!((layout.center.0 - 2.0).abs() < 1e-6);
        assert!((layout.center.1 + 2.0).abs() < 1e-6);
        assert!((layout.width - 4.0).abs() < 1e-6);
    }

    assert_eq!(scene.revision(), before.checked_next().unwrap());
}

#[test]
fn rejected_live_manim_scale_does_not_publish_partial_state() {
    let mut scene = Scene::new();
    let line = scene.line((1.0, 0.0), (3.0, 0.0)).unwrap();
    scene.add(&line).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = scene.revision();

    {
        let mut live = scene.live(&mut session);
        let state = live.effective(&line).unwrap();
        assert!(live.manim_scale(&line, f64::NAN, 1.0).is_err());
        assert_eq!(live.effective(&line).unwrap(), state);
    }

    assert_eq!(scene.revision(), before);
}
