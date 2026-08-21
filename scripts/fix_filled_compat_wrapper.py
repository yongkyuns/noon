from pathlib import Path

path = Path("crates/noon-geometry/src/tessellation.rs")
text = path.read_text()
old = '''pub fn tessellate_styled(
    path: &VectorPath,
    stroke_width: f32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
) -> Result<TessellatedPath, GeometryError> {
    tessellate_styled_with_fill(path, stroke_width, stroke_join, stroke_cap, true)
}
'''
new = '''pub fn tessellate_styled(
    path: &VectorPath,
    stroke_width: f32,
    stroke_join: StrokeJoin,
    stroke_cap: StrokeCap,
) -> Result<TessellatedPath, GeometryError> {
    // Preserve the historical helper contract: static paths include their fill
    // surface, while morph paths were stroke-only before fill became an explicit
    // renderer/style decision. Production rendering uses the explicit variant.
    let fill_enabled = path.morph_target().is_none();
    tessellate_styled_with_fill(
        path,
        stroke_width,
        stroke_join,
        stroke_cap,
        fill_enabled,
    )
}
'''
if old not in text:
    raise SystemExit("filled tessellation compatibility wrapper missing")
path.write_text(text.replace(old, new, 1))
