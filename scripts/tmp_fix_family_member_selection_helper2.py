from pathlib import Path

path = Path("scripts/tmp_shared_family_member_selection_migration.py")
text = path.read_text()
old = '''    rust = replace_once(
        rust,
        "        #[wasm_bindgen(js_name = alignToPoint)]\\n",
        selected_next_to + "        #[wasm_bindgen(js_name = alignToPoint)]\\n",
        label="add selected next_to methods",
    )
'''
new = '''    layout_impl = rust.index("    #[wasm_bindgen]\\n    impl WasmAuthoringFamilyLayout {")
    align_anchor = "        #[wasm_bindgen(js_name = alignToPoint)]\\n"
    align_pos = rust.index(align_anchor, layout_impl)
    rust = rust[:align_pos] + selected_next_to + rust[align_pos:]
'''
if old not in text:
    raise RuntimeError("selected next_to migration anchor block not found")
path.write_text(text.replace(old, new, 1))
