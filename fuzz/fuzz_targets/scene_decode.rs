#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 256 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(json) = std::str::from_utf8(data) else {
        return;
    };

    let _ = noon_ir::decode_scene(json);
    let _ = noon_ir::decode_semantic_scene(json);
});
