from pathlib import Path

path = Path('crates/noon-geometry/tests/tessellation_correctness.rs')
text = path.read_text()
old = 'const REFERENCE_TOLERANCE: f32 = 0.01;'
new = 'const REFERENCE_TOLERANCE: f32 = 0.002;'
if text.count(old) != 1:
    raise SystemExit('expected one reference tolerance constant')
path.write_text(text.replace(old, new, 1))
