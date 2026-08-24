use std::collections::BTreeSet;

use noon_ir::{
    decode_patch_batch, decode_scene, decode_semantic_scene, encode_patch_batch, encode_scene,
    IrError, SemanticIrError,
};
use serde_json::Value;

const MANIFEST: &str = include_str!("../../../compat/wire-contracts-v1.json");
const EMPTY_SCENE: &str = include_str!("../../../compat/wire/v1/scene-empty.json");
const UNKNOWN_FIELD_SCENE: &str = include_str!("../../../compat/wire/v1/scene-unknown-field.json");
const REACTIVE_SCENE: &str = include_str!("../../../compat/wire/v1/semantic-reactive.json");
const EMPTY_PATCH: &str = include_str!("../../../compat/wire/v1/patch-empty.json");
const FUTURE_SCENE: &str = include_str!("../../../compat/wire/invalid/future-scene.json");
const FUTURE_PATCH: &str = include_str!("../../../compat/wire/invalid/future-patch.json");
const MISSING_VERSION: &str = include_str!("../../../compat/wire/invalid/missing-version.json");
const DUPLICATE_OBJECT: &str = include_str!("../../../compat/wire/invalid/duplicate-object.json");

#[test]
fn contract_manifest_inventories_current_cross_language_boundaries() {
    let manifest: Value = serde_json::from_str(MANIFEST).expect("contract manifest is JSON");
    assert_eq!(manifest["manifest_version"], 1);
    assert_eq!(manifest["noon_ir_version"], 1);
    assert_eq!(manifest["authoring_protocol"]["channel"], "noon.authoring");
    assert_eq!(manifest["authoring_protocol"]["version"], 5);
    let names = manifest["contracts"].as_array().expect("contracts array").iter().map(|contract| contract["name"].as_str().expect("contract name")).collect::<BTreeSet<_>>();
    for required in ["scene_document","semantic_scene_document","patch_batch","authoring_envelope","authoring_result","host_callback_slots","host_callback_frame"] {
        assert!(names.contains(required), "wire contract missing {required}");
    }
}

#[test]
fn canonical_v1_fixtures_round_trip_and_preserve_stable_text_where_promised() {
    let scene = decode_scene(EMPTY_SCENE).expect("v1 empty scene decodes");
    assert_eq!(encode_scene(&scene).unwrap(), EMPTY_SCENE.trim());
    let batch = decode_patch_batch(EMPTY_PATCH).expect("v1 empty patch decodes");
    assert_eq!(encode_patch_batch(&batch).unwrap(), EMPTY_PATCH.trim());
    let semantic = decode_semantic_scene(REACTIVE_SCENE).expect("v1 reactive scene decodes");
    assert_eq!(semantic.definition().objects().len(), 1);
    assert_eq!(semantic.reactive().signals().len(), 1);
    assert_eq!(semantic.reactive().bindings().len(), 1);
}

#[test]
fn additive_unknown_top_level_fields_are_ignored_by_v1_readers() {
    let decoded = decode_scene(UNKNOWN_FIELD_SCENE).expect("additive metadata remains compatible");
    assert!(decoded.objects().is_empty());
    assert!(decoded.tracks().is_empty());
}

#[test]
fn future_versions_win_over_unknown_future_payload_variants() {
    assert!(matches!(decode_scene(FUTURE_SCENE), Err(IrError::UnsupportedVersion(2))));
    assert!(matches!(decode_patch_batch(FUTURE_PATCH), Err(IrError::UnsupportedVersion(2))));
    assert!(matches!(decode_semantic_scene(FUTURE_SCENE), Err(SemanticIrError::Scene(IrError::UnsupportedVersion(2)))));
}

#[test]
fn missing_required_version_and_duplicate_ids_are_rejected() {
    assert!(matches!(decode_scene(MISSING_VERSION), Err(IrError::Json(_))));
    assert!(decode_scene(DUPLICATE_OBJECT).is_err());
}
