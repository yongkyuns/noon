from pathlib import Path

path = Path("scripts/tmp_shared_family_arrange_migration.py")
text = path.read_text()
old = '''    check = replace_once(
        check,
        '  "export class WasmAuthoringFamilyLayout",\\n  "export class WasmAuthoringFamilyTranslation",\\n',
        '  "export class WasmAuthoringFamilyLayout",\\n  "export class WasmAuthoringFamilyArrange",\\n  "export class WasmAuthoringFamilyTranslation",\\n',
        label="pin arrange javascript class",
    )
'''
new = '''    class_anchor = '  "export class WasmAuthoringFamilyLayout",\\n  "export class WasmAuthoringFamilyTranslation",\\n'
    class_replacement = '  "export class WasmAuthoringFamilyLayout",\\n  "export class WasmAuthoringFamilyArrange",\\n  "export class WasmAuthoringFamilyTranslation",\\n'
    if check.count(class_anchor) != 2:
        raise RuntimeError(
            f"pin arrange javascript class: expected two JS/TS anchors, found {check.count(class_anchor)}"
        )
    check = check.replace(class_anchor, class_replacement, 1)
'''
if text.count(old) != 1:
    raise RuntimeError(f"expected one ambiguous package block, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
