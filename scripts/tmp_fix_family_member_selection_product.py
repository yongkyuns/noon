from pathlib import Path

rust_path = Path("crates/noon-web/src/authoring_mobject.rs")
rust = rust_path.read_text()
old_import = (
    "        semantic_family_leaf_ids, semantic_xy_f64, Bounds2D64, FrontendFamilyArrangePlan,\n"
    "        FrontendFamilyTargetEditor, FrontendFamilyTranslation, FrontendMobjectHandle,\n"
)
new_import = (
    "        semantic_family_leaf_ids, semantic_xy_f64, Bounds2D64, FrontendFamilyArrangePlan,\n"
    "        FrontendFamilyMemberSelection, FrontendFamilyTargetEditor, FrontendFamilyTranslation,\n"
    "        FrontendMobjectHandle,\n"
)
if old_import not in rust:
    if "FrontendFamilyMemberSelection, FrontendFamilyTargetEditor" not in rust:
        raise RuntimeError("wasm family selection import anchor not found")
else:
    rust = rust.replace(old_import, new_import, 1)
rust_path.write_text(rust)

test_path = Path("web/python/test_manim_shared_family_member_selection.py")
test = test_path.read_text()
anchor = '''            external = Circle(radius=0.1)\n            source.next_to((5.0, 2.0), RIGHT, submobject_to_align=external)\n'''
combined = '''            external = Circle(radius=0.1)\n            source.next_to(\n                target,\n                RIGHT,\n                submobject_to_align=external,\n                index_of_submobject_to_align=-1,\n            )\n            assert store.bounds_calls[-1] == (\"mobject\", external._semantic_handle.identity)\n            assert store.member_selections[-1] == (\n                target._semantic_family_handle.identity,\n                -1,\n                target_nested._semantic_family_handle.identity,\n            )\n            assert store.selected_calls[-1][0] == \"bounds\"\n            assert store.selected_calls[-1][1] == external._semantic_handle.identity\n            assert store.selected_calls[-1][2] == target_nested._semantic_family_handle.identity\n\n            source.next_to((5.0, 2.0), RIGHT, submobject_to_align=external)\n'''
if anchor not in test:
    if "submobject_to_align=external,\n                index_of_submobject_to_align=-1" not in test:
        raise RuntimeError("combined explicit-source/indexed-target regression anchor not found")
else:
    test = test.replace(anchor, combined, 1)
test_path.write_text(test)
