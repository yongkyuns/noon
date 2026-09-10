from pathlib import Path

path = Path("crates/noon/src/dimension_fit.rs")
text = path.read_text()
old = "crate::family_affine::FamilyAffine::Scale(x, y, pivot).transaction("
new = "crate::family_affine::FamilyAffine::ManimScale(x, y, pivot).transaction("
if text.count(old) != 1:
    raise SystemExit(f"expected one dimension-fit scale call, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
