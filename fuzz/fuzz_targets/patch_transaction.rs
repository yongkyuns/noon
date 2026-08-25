#![no_main]

use libfuzzer_sys::fuzz_target;
use noon_core::{GeometryRef, MutationTransaction, SceneDefinition};

const MAX_INPUT: usize = 256 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(json) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(batch) = noon_ir::decode_patch_batch(json) else {
        return;
    };

    let mut scene = SceneDefinition::new();
    scene.add(GeometryRef::circle(1.0));
    scene.add(GeometryRef::rectangle(2.0, 1.0));
    let before = scene.clone();
    let transaction = MutationTransaction::from_mutations(batch.patches);
    if scene.apply_transaction(&transaction).is_err() {
        assert_eq!(scene, before, "rejected fuzz transaction partially mutated the scene");
    }
});
