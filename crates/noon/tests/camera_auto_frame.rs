use noon::{AuthoringError, Scene};
use noon_core::DEFAULT_FRAME_HEIGHT;

#[test]
fn auto_frame_fits_transformed_content_with_margin_and_aspect() {
    let mut scene = Scene::new();
    let frame = scene.camera_frame().unwrap();
    let mut left = scene.rectangle(2.0, 4.0).unwrap();
    left.set_translation(-4.0, 1.0).unwrap();
    let mut right = scene.square(2.0).unwrap();
    right.set_translation(3.0, -2.0).unwrap();

    let fit = scene
        .camera_auto_frame(&[&left, &right], 16.0 / 9.0, 0.5)
        .unwrap();
    assert_eq!(fit.center, (-0.5, 0.0));
    assert!((fit.height - 7.0).abs() < 1.0e-12);

    let target = scene
        .camera_auto_frame_target(&frame, &[&left, &right], 16.0 / 9.0, 0.5)
        .unwrap();
    let state = target.state().unwrap();
    assert_eq!(state.transform.translation.x, -0.5);
    assert_eq!(state.transform.translation.y, 0.0);
    assert!(
        (state.transform.scale.x - fit.height / f64::from(DEFAULT_FRAME_HEIGHT)).abs() < 1.0e-6
    );
    assert_eq!(state.transform.scale.x, state.transform.scale.y);
}

#[test]
fn wide_content_expands_height_to_preserve_requested_aspect() {
    let mut scene = Scene::new();
    let wide = scene.rectangle(10.0, 1.0).unwrap();
    let fit = scene.camera_auto_frame(&[&wide], 2.0, 1.0).unwrap();
    assert_eq!(fit.center, (0.0, 0.0));
    assert_eq!(fit.height, 6.0);
}

#[test]
fn auto_frame_observation_does_not_mutate_scene_or_sources() {
    let mut scene = Scene::new();
    let frame = scene.camera_frame().unwrap();
    let object = scene.square(2.0).unwrap();
    let frame_before = frame.state().unwrap();
    let object_before = object.state().unwrap();
    let revision = scene.revision();

    let fit = scene.camera_auto_frame(&[&object], 1.0, 0.25).unwrap();
    assert_eq!(fit.center, (0.0, 0.0));
    assert_eq!(fit.height, 2.5);
    assert_eq!(scene.revision(), revision);
    assert_eq!(frame.state().unwrap(), frame_before);
    assert_eq!(object.state().unwrap(), object_before);
}

#[test]
fn auto_frame_rejects_invalid_inputs_before_target_allocation() {
    let mut scene = Scene::new();
    let frame = scene.camera_frame().unwrap();
    let object = scene.square(2.0).unwrap();
    let revision = scene.revision();

    for result in [
        scene.camera_auto_frame(&[], 1.0, 0.0),
        scene.camera_auto_frame(&[&object], 0.0, 0.0),
        scene.camera_auto_frame(&[&object], f64::NAN, 0.0),
        scene.camera_auto_frame(&[&object], 1.0, -1.0),
        scene.camera_auto_frame(&[&object], 1.0, f64::MAX),
    ] {
        assert!(matches!(
            result,
            Err(AuthoringError::InvalidCameraAutoFrame(_))
        ));
    }
    assert_eq!(scene.revision(), revision);

    let mut other_scene = Scene::new();
    let other = other_scene.square(1.0).unwrap();
    assert_eq!(
        scene.camera_auto_frame(&[&other], 1.0, 0.0).unwrap_err(),
        AuthoringError::ForeignStore
    );
    assert_eq!(scene.revision(), revision);
    assert!(scene
        .camera_auto_frame_target(&frame, &[&object], 1.0, 0.0)
        .is_ok());
}
