from pathlib import Path

path = Path("crates/noon-runtime/src/lib.rs")
text = path.read_text()
old = '''fn path_geometry_morphs(from: &ObjectSnapshot, to: &ObjectSnapshot) -> bool {\n    from.geometry != to.geometry\n        && matches!(\n            (&from.geometry, &to.geometry),\n            (GeometryRef::VectorPath(_), GeometryRef::VectorPath(_))\n        )\n}\n\n'''
if old in text:
    path.write_text(text.replace(old, "", 1))
