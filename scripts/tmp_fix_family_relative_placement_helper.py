from pathlib import Path

path = Path("scripts/tmp_shared_family_relative_placement_migration.py")
text = path.read_text()
old = '''rust = replace_once(
    rust,
    '        authoring_style_from_legacy, finite_f32, legacy_solid_color, render_f64,\\n',
    '        authoring_style_from_legacy, finite_f32, legacy_solid_color, manim_family_align_to_delta,\\n        manim_family_next_to_delta, render_f64,\\n',
    label="import relative-placement helpers into wasm module",
)
'''
new = '''rust = replace_once(
    rust,
    '    use super::{\\n        semantic_family_leaf_ids, semantic_xy_f64, Bounds2D64, FrontendFamilyTargetEditor,\\n        FrontendFamilyTranslation, FrontendMobjectHandle, ManimNextToArgs, SemanticNodeId,\\n        SemanticStore,\\n    };\\n',
    '    use super::{\\n        manim_family_align_to_delta, manim_family_next_to_delta, semantic_family_leaf_ids,\\n        semantic_xy_f64, Bounds2D64, FrontendFamilyTargetEditor, FrontendFamilyTranslation,\\n        FrontendMobjectHandle, ManimNextToArgs, SemanticNodeId, SemanticStore,\\n    };\\n',
    label="import relative-placement helpers into wasm module",
)
'''
if text.count(old) != 1:
    raise RuntimeError(f"expected one helper import block, found {text.count(old)}")
text = text.replace(old, new, 1)
for name in ("manim_family_next_to_delta", "manim_family_align_to_delta"):
    old_fn = f"fn {name}("
    new_fn = f'#[cfg(any(target_arch = "wasm32", test))]\nfn {name}('
    if text.count(old_fn) != 1:
        raise RuntimeError(f"expected one {name} declaration, found {text.count(old_fn)}")
    text = text.replace(old_fn, new_fn, 1)
path.write_text(text)
