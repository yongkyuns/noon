from pathlib import Path

path = Path("scripts/tmp_shared_family_translation_migration.py")
text = path.read_text()
old = '''critical_anchor = '        #[wasm_bindgen(js_name = criticalX)]\\n        pub fn critical_x('
rust = replace_once(
    rust,
    critical_anchor,
    placement_methods + critical_anchor,
    label="insert shared family placement methods",
)
'''
new = '''critical_anchor = '        #[wasm_bindgen(js_name = criticalX)]\\n        pub fn critical_x('
family_layout_start = rust.index("    pub struct WasmAuthoringFamilyLayout")
critical_index = rust.index(critical_anchor, family_layout_start)
rust = rust[:critical_index] + placement_methods + rust[critical_index:]
'''
if text.count(old) != 1:
    raise SystemExit(f"critical anchor patch expected once, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
