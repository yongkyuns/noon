from pathlib import Path

path = Path("crates/noon-geometry/src/morph.rs")
text = path.read_text()
old = "fn canonicalize_ccw(points: &mut Vec<Vec2>, side: MorphSide) -> Result<(), FilledMorphError> {"
new = "fn canonicalize_ccw(points: &mut [Vec2], side: MorphSide) -> Result<(), FilledMorphError> {"
if old not in text:
    raise SystemExit("filled morph canonicalization signature missing")
path.write_text(text.replace(old, new, 1))
