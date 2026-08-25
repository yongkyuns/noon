from pathlib import Path

path = Path("crates/noon/src/legacy.rs")
text = path.read_text()

anchor = '''define_shape!(Circle);\ndefine_shape!(Rectangle);\ndefine_shape!(Square);\ndefine_shape!(Line);\ndefine_shape!(Path);\n\nimpl Circle {\n'''
replacement = '''define_shape!(Circle);\ndefine_shape!(Rectangle);\ndefine_shape!(Square);\ndefine_shape!(Line);\ndefine_shape!(Path);\n\nconst MANIM_CAIRO_DEFAULT_STROKE_WIDTH: f32 = 0.04;\n\nfn manim_vmobject_snapshot(geometry: GeometryRef) -> ObjectSnapshot {\n    let mut snapshot = ObjectSnapshot::new(geometry);\n    let mut transparent_white = WHITE;\n    transparent_white.alpha = 0.0;\n    snapshot.style.fill = Some(transparent_white);\n    snapshot.style.stroke = Some(WHITE);\n    snapshot.style.stroke_width = MANIM_CAIRO_DEFAULT_STROKE_WIDTH;\n    snapshot.style.stroke_join = StrokeJoin::Miter;\n    snapshot.style.stroke_cap = StrokeCap::Butt;\n    snapshot\n}\n\nimpl Circle {\n'''
if text.count(anchor) != 1:
    raise SystemExit("shape helper anchor mismatch")
text = text.replace(anchor, replacement)

replacements = {
'''    pub fn new(radius: f32) -> Self {\n        Self(ObjectSnapshot::new(GeometryRef::circle(radius)))\n    }\n''':
'''    pub fn new(radius: f32) -> Self {\n        Self(manim_vmobject_snapshot(GeometryRef::circle(radius)))\n    }\n''',
'''    pub fn new(width: f32, height: f32) -> Self {\n        Self(ObjectSnapshot::new(GeometryRef::rectangle(width, height)))\n    }\n''':
'''    pub fn new(width: f32, height: f32) -> Self {\n        Self(manim_vmobject_snapshot(GeometryRef::rectangle(width, height)))\n    }\n''',
'''    pub fn new(side_length: f32) -> Self {\n        Self(ObjectSnapshot::new(GeometryRef::square(side_length)))\n    }\n''':
'''    pub fn new(side_length: f32) -> Self {\n        Self(manim_vmobject_snapshot(GeometryRef::square(side_length)))\n    }\n''',
'''    pub fn new(start: Vec2, end: Vec2) -> Self {\n        let snapshot = ObjectSnapshot::new(GeometryRef::line(start, end))\n            .set_fill(None, None)\n            .set_stroke(Some(WHITE), Some(0.04));\n        Self(snapshot)\n    }\n''':
'''    pub fn new(start: Vec2, end: Vec2) -> Self {\n        Self(manim_vmobject_snapshot(GeometryRef::line(start, end)))\n    }\n''',
'''    pub fn new(path: VectorPath) -> Self {\n        Self(ObjectSnapshot::new(GeometryRef::path(path)))\n    }\n''':
'''    pub fn new(path: VectorPath) -> Self {\n        Self(manim_vmobject_snapshot(GeometryRef::path(path)))\n    }\n''',
}
for old, new in replacements.items():
    if text.count(old) != 1:
        raise SystemExit(f"constructor anchor mismatch: {old.splitlines()[0]}")
    text = text.replace(old, new)

path.write_text(text)

# Add a focused Rust regression so the authoring facade cannot drift away from
# the Manim-compatible Python facade while noon-core's neutral Style::default stays unchanged.
test_path = Path("crates/noon/tests/manim_style_defaults.rs")
test_path.write_text('''use noon::prelude::*;\n\n#[test]\nfn rust_authoring_shapes_use_manim_vmobject_defaults() {\n    for snapshot in [\n        Circle::default().snapshot(),\n        Square::default().snapshot(),\n        Line::default().snapshot(),\n    ] {\n        let fill = snapshot.style.fill.expect("Manim VMobject keeps a fill paint layer");\n        assert_eq!(fill.red, 1.0);\n        assert_eq!(fill.green, 1.0);\n        assert_eq!(fill.blue, 1.0);\n        assert_eq!(fill.alpha, 0.0);\n        assert_eq!(snapshot.style.stroke, Some(WHITE));\n        assert!((snapshot.style.stroke_width - 0.04).abs() < f32::EPSILON);\n        assert_eq!(snapshot.style.stroke_join, noon_core::StrokeJoin::Miter);\n        assert_eq!(snapshot.style.stroke_cap, noon_core::StrokeCap::Butt);\n    }\n}\n\n#[test]\nfn core_style_default_remains_renderer_neutral() {\n    let style = noon_core::Style::default();\n    assert_eq!(style.fill, Some(WHITE));\n    assert_eq!(style.stroke, None);\n    assert_eq!(style.stroke_join, noon_core::StrokeJoin::Round);\n    assert_eq!(style.stroke_cap, noon_core::StrokeCap::Round);\n}\n''')
