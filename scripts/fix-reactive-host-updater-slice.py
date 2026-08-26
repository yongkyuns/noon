from pathlib import Path

path = Path("crates/noon-core/src/patch/transaction_preflight.rs")
text = path.read_text()
old = """            ScenePatch::SetTransform { object, .. } | ScenePatch::SetStyle { object, .. } => {\n                if !objects.contains(object) {\n"""
new = """            ScenePatch::SetGeometry { object, .. }\n            | ScenePatch::SetTransform { object, .. }\n            | ScenePatch::SetStyle { object, .. } => {\n                if !objects.contains(object) {\n"""
if text.count(old) != 1:
    raise SystemExit(f"expected one transaction-preflight property match, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
