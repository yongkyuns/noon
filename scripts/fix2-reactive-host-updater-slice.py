from pathlib import Path

path = Path("crates/noon-runtime/src/execution_slots.rs")
text = path.read_text()
old = """            ScenePatch::SetTransform { object, .. } | ScenePatch::SetStyle { object, .. } => {\n                let slot = self\n"""
new = """            ScenePatch::SetGeometry { object, .. }\n            | ScenePatch::SetTransform { object, .. }\n            | ScenePatch::SetStyle { object, .. } => {\n                let slot = self\n"""
if text.count(old) != 1:
    raise SystemExit(f"expected one execution property classifier, found {text.count(old)}")
text = text.replace(old, new, 1)
old = """            ScenePatch::RemoveObject(object)\n            | ScenePatch::SetTransform { object, .. }\n            | ScenePatch::SetStyle { object, .. } => {\n"""
new = """            ScenePatch::RemoveObject(object)\n            | ScenePatch::SetGeometry { object, .. }\n            | ScenePatch::SetTransform { object, .. }\n            | ScenePatch::SetStyle { object, .. } => {\n"""
if text.count(old) != 1:
    raise SystemExit(f"expected one execution context classifier, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
