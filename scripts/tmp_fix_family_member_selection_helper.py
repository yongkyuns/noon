from pathlib import Path

path = Path("scripts/tmp_shared_family_member_selection_migration.py")
text = path.read_text()
old = '''    rust = replace_once(
        rust,
        "        #[wasm_bindgen(getter, js_name = centerX)]\\n",
        family_bounds_method + "        #[wasm_bindgen(getter, js_name = centerX)]\\n",
        label="add family bounds handle",
    )
'''
new = '''    family_layout_tail = (
        "            self.next_leaf += 1;\\n"
        "            Ok(())\\n"
        "        }\\n\\n"
        "        #[wasm_bindgen(getter, js_name = centerX)]\\n"
    )
    rust = replace_once(
        rust,
        family_layout_tail,
        (
            "            self.next_leaf += 1;\\n"
            "            Ok(())\\n"
            "        }\\n\\n"
            + family_bounds_method
            + "        #[wasm_bindgen(getter, js_name = centerX)]\\n"
        ),
        label="add family bounds handle",
    )
'''
if text.count(old) != 1:
    raise RuntimeError(f"expected one ambiguous family bounds block, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
