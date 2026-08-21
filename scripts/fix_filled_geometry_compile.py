from pathlib import Path

path = Path("crates/noon-geometry/src/morph.rs")
text = path.read_text()
marker = "const FILL_AREA_EPSILON: f32 = 1.0e-5;"
if marker not in text:
    raise SystemExit("filled morph marker missing")
helper = '''fn cross(a: Vec2, b: Vec2) -> f32 {\n    a.x * b.y - a.y * b.x\n}\n\n'''
if helper not in text:
    text = text.replace(marker, helper + marker, 1)
path.write_text(text)
