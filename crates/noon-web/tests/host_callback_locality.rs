use noon_core::{GeometryRef, SceneDefinition};
use noon_ir::encode_scene;
use noon_web::HostScenePlayer;
use serde_json::{json, Value};

const SCENE_OBJECTS: usize = 10_000;
const HOST_OBJECT_INDEX: usize = SCENE_OBJECTS / 2;

#[test]
fn callback_snapshot_stays_local_in_a_large_native_scene() {
    let mut definition = SceneDefinition::new();
    let mut host_object = None;

    for index in 0..SCENE_OBJECTS {
        let object = definition.add(GeometryRef::circle(0.1));
        if index == HOST_OBJECT_INDEX {
            host_object = Some(object);
        }
    }

    let host_object = host_object.expect("host object must be present in the scene");
    let scene_json = encode_scene(&definition).expect("large scene must encode");
    let slots = format!(r#"[{{"id":0,"objects":[{}]}}]"#, host_object.get());
    let mut player = HostScenePlayer::from_json(&scene_json, &slots)
        .expect("large scene with one callback slot must initialize");

    player
        .advance_to(0.25)
        .expect("large scene must advance without host-wide work");
    let frame: Value = serde_json::from_str(
        &player
            .callback_frame_json()
            .expect("callback frame must serialize"),
    )
    .expect("callback frame must be valid JSON");

    let objects = frame["objects"]
        .as_array()
        .expect("callback frame objects must be an array");
    assert_eq!(
        objects.len(),
        1,
        "only callback-owned objects belong in the host snapshot"
    );
    assert_eq!(objects[0]["object"], host_object.get());

    let invocations = frame["invocations"]
        .as_array()
        .expect("callback invocations must be an array");
    assert_eq!(invocations.len(), 1);
    assert_eq!(
        invocations[0]["object_indices"],
        json!([0]),
        "callback routing uses compact indices into the phase-wide callback snapshot table"
    );
}
