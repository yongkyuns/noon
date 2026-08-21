from pathlib import Path

path = Path("crates/noon-geometry/tests/tessellation_correctness.rs")
text = path.read_text()
old = '''fn midpoint(a: Vec2, b: Vec2) -> Vec2 {\n    scale(add(a, b), 0.5)\n}\n\n'''
if old in text:
    text = text.replace(old, "", 1)
path.write_text(text)
