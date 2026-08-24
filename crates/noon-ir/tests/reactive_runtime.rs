use noon_core::{ReactiveValue, SignalId, Vec2};
use noon_ir::decode_semantic_scene;
use noon_runtime::SceneInstance;

fn python_style_scene_json() -> &'static str {
    r#"{
        "version":1,
        "objects":[
            {
                "id":0,
                "geometry":{"rectangle":{"size":{"x":1.0,"y":1.0}}},
                "transform":{"translation":{"x":0.0,"y":0.0},"rotation":0.0,"scale":{"x":1.0,"y":1.0}},
                "style":{"fill":null,"stroke":null,"stroke_width":1.0,"stroke_join":"round","stroke_cap":"round","opacity":1.0}
            },
            {
                "id":1,
                "geometry":{"circle":{"radius":0.5}},
                "transform":{"translation":{"x":0.0,"y":0.0},"rotation":0.0,"scale":{"x":1.0,"y":1.0}},
                "style":{"fill":null,"stroke":null,"stroke_width":1.0,"stroke_join":"round","stroke_cap":"round","opacity":1.0}
            }
        ],
        "tracks":[],
        "reactive":{
            "signals":[
                {"id":0,"source":{"input":{"scalar":1.5}}},
                {"id":1,"source":{"input":{"scalar":2.0}}},
                {"id":2,"source":{"derived":{"add":[
                    {"constant":{"vec2":{"x":0.0,"y":1.0}}},
                    {"mul":[
                        {"signal":1},
                        {"constant":{"vec2":{"x":1.0,"y":0.0}}}
                    ]}
                ]}}}
            ],
            "bindings":[
                {"signal":0,"object":0,"property":"rotation"},
                {"signal":2,"object":1,"property":"position"}
            ]
        }
    }"#
}

#[test]
fn python_document_is_lowered_by_native_runtime() {
    let semantic =
        decode_semantic_scene(python_style_scene_json()).expect("wire graph must decode");
    let mut instance =
        SceneInstance::from_semantic(&semantic).expect("semantic scene must compile");

    assert_eq!(instance.frame().objects[0].transform.rotation, 1.5);
    assert_eq!(
        instance.frame().objects[1].transform.translation,
        Vec2::new(2.0, 1.0)
    );
    assert_eq!(
        instance.reactive_value(SignalId::new(2)),
        Some(&ReactiveValue::Vec2(Vec2::new(2.0, 1.0)))
    );

    instance.take_frame_changes();
    instance
        .set_reactive_input(SignalId::new(1), 4.0_f32)
        .expect("native input update must succeed");

    assert_eq!(
        instance.frame().objects[1].transform.translation,
        Vec2::new(4.0, 1.0)
    );
    assert_eq!(instance.take_frame_changes().object_indices(), &[1]);
    assert_eq!(instance.last_reactive_stats().derived_signals_evaluated, 1);
    assert_eq!(instance.last_reactive_stats().dense_targets_applied, 1);
}
