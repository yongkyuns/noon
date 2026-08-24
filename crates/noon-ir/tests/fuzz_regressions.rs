use noon_core::{GeometryRef, MutationTransaction, SceneDefinition};
use noon_ir::{decode_patch_batch, decode_scene, decode_semantic_scene, IrError};

#[test]
fn malformed_and_future_scene_inputs_fail_without_panicking() {
    for input in [
        "",
        "{}",
        "[]",
        r#"{"version":4294967295,"objects":[],"tracks":[]}"#,
        r#"{"version":1,"objects":"not-an-array","tracks":[]}"#,
        r#"{"version":1,"objects":[],"tracks":[{"object":999}]}"#,
    ] {
        let _ = decode_scene(input);
        let _ = decode_semantic_scene(input);
    }

    assert!(matches!(
        decode_scene(r#"{"version":4294967295,"objects":[],"tracks":[]}"#),
        Err(IrError::UnsupportedVersion(4294967295))
    ));
}

#[test]
fn rejected_patch_corpus_never_partially_mutates_scene() {
    let cases = [
        r#"{"version":1,"sequence":1,"patches":[{"remove_object":999}]}"#,
        r#"{"version":1,"sequence":2,"patches":[{"set_style":{"object":999,"style":{"fill":null,"stroke":null,"stroke_width":1.0,"stroke_join":"round","stroke_cap":"round","opacity":1.0}}}]}"#,
    ];

    for input in cases {
        let batch = decode_patch_batch(input).expect("fixed corpus must decode as a batch");
        let mut scene = SceneDefinition::new();
        scene.add(GeometryRef::circle(1.0));
        let before = scene.clone();
        let transaction = MutationTransaction::from_mutations(batch.patches);
        assert!(scene.apply_transaction(&transaction).is_err());
        assert_eq!(scene, before);
    }
}
