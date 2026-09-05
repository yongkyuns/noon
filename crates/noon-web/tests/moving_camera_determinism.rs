use noon::legacy::prelude::*;
use noon_core::{Camera2DState, GeometryRef, Transform2D};
use noon_ir::{decode_scene, encode_scene};
use noon_web::{scene_snapshot_json, EngineScenePlayer, ExecutionDeltaEnvelope};

fn moving_camera_scene_json() -> String {
    let mut scene = MovingCameraScene::new();
    let frame = scene.camera_frame();
    scene.wait(0.3).unwrap();
    scene
        .play(frame.animate().move_to(LEFT * 2.0))
        .run_time(1.0)
        .unwrap();
    scene.wait(0.3).unwrap();
    scene
        .play(frame.animate().move_to(RIGHT * 2.0))
        .run_time(1.0)
        .unwrap();
    encode_scene(&scene.into_definition()).unwrap()
}

fn direct_camera(scene_json: &str, time: f64) -> Camera2DState {
    let definition = decode_scene(scene_json).unwrap();
    let camera_object = definition.camera_object().unwrap();
    let snapshot: serde_json::Value =
        serde_json::from_str(&scene_snapshot_json(scene_json, time).unwrap()).unwrap();
    let object = snapshot["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|object| object["id"].as_u64() == Some(camera_object.get()))
        .unwrap();
    let content = object["content"].as_object().unwrap();
    assert_eq!(content["kind"], "geometry");
    let geometry: GeometryRef = serde_json::from_value(content["geometry"].clone()).unwrap();
    let transform: Transform2D = serde_json::from_value(object["transform"].clone()).unwrap();
    Camera2DState::from_frame_object(&geometry, transform).unwrap()
}

fn delta_camera(delta_json: &str) -> Camera2DState {
    serde_json::from_str::<ExecutionDeltaEnvelope>(delta_json)
        .unwrap()
        .camera
}

fn assert_camera_close(actual: Camera2DState, expected: Camera2DState) {
    assert!((actual.center.x - expected.center.x).abs() < 1.0e-5);
    assert!((actual.center.y - expected.center.y).abs() < 1.0e-5);
    assert!((actual.height - expected.height).abs() < 1.0e-5);
}

#[test]
fn direct_seek_incremental_playback_and_loop_rewind_agree_for_camera() {
    let scene_json = moving_camera_scene_json();
    let mut engine = EngineScenePlayer::new(&scene_json, 2.6, 41).unwrap();

    let initial = engine.initial_delta_json().unwrap();
    assert_camera_close(delta_camera(&initial), direct_camera(&scene_json, 0.0));
    assert!(engine.tick_delta_json(0.0).unwrap().is_none());

    let at_one = engine.tick_delta_json(1_000.0).unwrap().unwrap();
    assert_camera_close(delta_camera(&at_one), direct_camera(&scene_json, 1.0));

    let at_two_point_two = engine.tick_delta_json(2_200.0).unwrap().unwrap();
    assert_camera_close(
        delta_camera(&at_two_point_two),
        direct_camera(&scene_json, 2.2),
    );

    // 3.0 seconds on a 2.6-second loop rewinds deterministically to scene time 0.4.
    let rewound = engine.tick_delta_json(3_000.0).unwrap().unwrap();
    assert_camera_close(delta_camera(&rewound), direct_camera(&scene_json, 0.4));
}
