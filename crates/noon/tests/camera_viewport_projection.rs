//! B6 camera qualification through the ordinary typed authoring/runtime path.
//! No host camera state, frontend interpolation, renderer or transport fixture.

use noon::{AnimationOptions, RateFunction, Scene};
use noon_core::{Camera2DState, DEFAULT_FRAME_HEIGHT};

const SAMPLE_TIMES: [f64; 6] = [0.0, 0.25, 0.5, 0.75, 59.0 / 60.0, 1.0];
const ASPECTS: [f32; 4] = [9.0 / 16.0, 1.0, 16.0 / 9.0, 32.0 / 9.0];

fn assert_projection(camera: Camera2DState, time: f64) {
    let t = time as f32;
    assert!((camera.center.x - 2.0 * t).abs() < 2.0e-6);
    assert!((camera.center.y + t).abs() < 2.0e-6);
    assert!((camera.height - DEFAULT_FRAME_HEIGHT * (1.0 - 0.5 * t)).abs() < 2.0e-6);
    for aspect in ASPECTS {
        let bounds = camera.viewport_bounds(aspect).unwrap();
        assert!((bounds.center().x - camera.center.x).abs() < 2.0e-6);
        assert!((bounds.center().y - camera.center.y).abs() < 2.0e-6);
        assert!((bounds.height() - camera.height).abs() < 2.0e-6);
        assert!((bounds.width() - camera.height * aspect).abs() < 4.0e-6);
    }
}

#[test]
fn pan_zoom_changes_effective_camera_not_authored_or_unrelated_state() {
    let mut scene = Scene::new();
    let frame = scene.camera_frame().unwrap();
    // A camera operation must not rewrite or dirty these unrelated objects.
    for index in 0..1_000 {
        let mut object = scene.square(0.5).unwrap();
        object.set_translation(f64::from(index), 10.0).unwrap();
        scene.add(&object).unwrap();
    }
    let mut target = frame.target_editor().unwrap();
    target.set_translation(2.0, -1.0).unwrap();
    target.set_scale(0.5, 0.5).unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(
            &frame,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let authored = frame.state().unwrap();
    let revision = scene.revision();
    let unrelated = session.frame().objects[1..].to_vec();
    session.take_frame_changes();

    for time in SAMPLE_TIMES {
        scene
            .live(&mut session)
            .advance_segment_to(segment, time)
            .unwrap();
        assert_projection(session.camera().unwrap(), time);
        assert_eq!(scene.revision(), revision);
        assert_eq!(frame.state().unwrap(), authored);
        assert_eq!(&session.frame().objects[1..], unrelated.as_slice());
        let changes = session.take_frame_changes();
        assert!(!changes.is_all());
        assert!(changes.object_indices().iter().all(|&index| index == 0));
        let effective = scene.live(&mut session).effective_layout(&frame).unwrap();
        assert!((effective.center.0 - 2.0 * time).abs() < 2.0e-6);
        assert!((effective.center.1 + time).abs() < 2.0e-6);
        assert!((effective.height - f64::from(session.camera().unwrap().height)).abs() < 2.0e-6);
    }

    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_projection(session.camera().unwrap(), 1.0);
    assert_eq!(
        frame.state().unwrap().transform,
        target.state().unwrap().transform
    );
    assert_eq!(&session.frame().objects[1..], unrelated.as_slice());
    session.take_frame_changes();
    let context = session.publication_context();
    for _ in 0..10 {
        assert_projection(session.camera().unwrap(), 1.0);
    }
    assert_eq!(session.publication_context(), context);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn direct_seek_and_forward_pan_zoom_agree_for_every_viewport_aspect() {
    let mut scene = Scene::new();
    let frame = scene.camera_frame().unwrap();
    let mut target = frame.target_editor().unwrap();
    target.set_translation(2.0, -1.0).unwrap();
    target.set_scale(0.5, 0.5).unwrap();
    let mut forward = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut forward)
        .declare_and_activate_transform_to(
            &frame,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut direct = forward.clone();
    let revision = scene.revision();
    let authored = frame.state().unwrap();
    let mut samples = Vec::new();
    for time in SAMPLE_TIMES {
        scene
            .live(&mut forward)
            .advance_segment_to(segment, time)
            .unwrap();
        let camera = forward.camera().unwrap();
        assert_projection(camera, time);
        samples.push((time, camera));
    }
    for &(time, expected) in samples.iter().rev().chain(samples.iter()) {
        direct.seek(time).unwrap();
        assert_eq!(direct.camera().unwrap(), expected, "camera at {time}");
        for aspect in ASPECTS {
            assert_eq!(
                direct.camera().unwrap().viewport_bounds(aspect),
                expected.viewport_bounds(aspect),
                "viewport at {time}, aspect {aspect}"
            );
        }
    }
    assert_eq!(scene.revision(), revision);
    assert_eq!(frame.state().unwrap(), authored);
}
