from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if old not in text:
        raise SystemExit(f'pattern not found in {path}: {old[:80]!r}')
    p.write_text(text.replace(old, new, 1))


replace_once(
    'crates/noon/src/legacy.rs',
    '''fn manim_vmobject_snapshot(geometry: GeometryRef) -> ObjectSnapshot {\n    let mut snapshot = ObjectSnapshot::new(geometry);\n    let mut transparent_white = WHITE;\n    transparent_white.alpha = 0.0;\n    snapshot.style.fill = Some(transparent_white);\n    snapshot.style.stroke = Some(WHITE);\n''',
    '''fn manim_vmobject_snapshot(geometry: GeometryRef, default_color: Color) -> ObjectSnapshot {\n    let mut snapshot = ObjectSnapshot::new(geometry);\n    let mut transparent_fill = default_color;\n    transparent_fill.alpha = 0.0;\n    snapshot.style.fill = Some(transparent_fill);\n    snapshot.style.stroke = Some(default_color);\n''',
)
replace_once(
    'crates/noon/src/legacy.rs',
    'Self(manim_vmobject_snapshot(GeometryRef::circle(radius)))',
    'Self(manim_vmobject_snapshot(GeometryRef::circle(radius), RED))',
)
replace_once(
    'crates/noon/src/legacy.rs',
    '''Self(manim_vmobject_snapshot(GeometryRef::rectangle(\n            width, height,\n        )))''',
    '''Self(manim_vmobject_snapshot(\n            GeometryRef::rectangle(width, height),\n            WHITE,\n        ))''',
)
replace_once(
    'crates/noon/src/legacy.rs',
    'Self(manim_vmobject_snapshot(GeometryRef::square(side_length)))',
    'Self(manim_vmobject_snapshot(GeometryRef::square(side_length), WHITE))',
)
replace_once(
    'crates/noon/src/legacy.rs',
    'Self(manim_vmobject_snapshot(GeometryRef::line(start, end)))',
    'Self(manim_vmobject_snapshot(GeometryRef::line(start, end), WHITE))',
)
replace_once(
    'crates/noon/src/legacy.rs',
    'Self(manim_vmobject_snapshot(GeometryRef::path(path)))',
    'Self(manim_vmobject_snapshot(GeometryRef::path(path), WHITE))',
)

replace_once(
    'crates/noon/tests/manim_style_defaults.rs',
    '''    for snapshot in [\n        Circle::default().snapshot(),\n        Square::default().snapshot(),\n        Line::default().snapshot(),\n    ] {''',
    '''    for snapshot in [Square::default().snapshot(), Line::default().snapshot()] {''',
)
insert = '''\n#[test]\nfn rust_circle_uses_manim_specific_red_default() {\n    let snapshot = Circle::default();\n    let fill = snapshot\n        .snapshot()\n        .style\n        .fill\n        .expect("Manim Circle keeps a transparent fill paint layer");\n    assert_eq!(fill.red, RED.red);\n    assert_eq!(fill.green, RED.green);\n    assert_eq!(fill.blue, RED.blue);\n    assert_eq!(fill.alpha, 0.0);\n    assert_eq!(snapshot.snapshot().style.stroke, Some(RED));\n    assert!((snapshot.snapshot().style.stroke_width - 0.04).abs() < f32::EPSILON);\n}\n'''
p = Path('crates/noon/tests/manim_style_defaults.rs')
text = p.read_text()
marker = '\n#[test]\nfn core_style_default_remains_renderer_neutral()'
if marker not in text:
    raise SystemExit('Rust test insertion marker not found')
p.write_text(text.replace(marker, insert + marker, 1))

replace_once(
    'web/python/_manim_compat.py',
    '''def _manim_vmobject_kwargs(kwargs: dict[str, Any]) -> dict[str, Any]:\n    """Apply ManimCE VMobject defaults without changing native Noon IR defaults."""\n    result = dict(kwargs)\n    result.setdefault("fill", _base.Color(1.0, 1.0, 1.0, 0.0))\n    result.setdefault("stroke", _base.WHITE)\n''',
    '''def _manim_vmobject_kwargs(\n    kwargs: dict[str, Any], *, default_color: _base.Color = _base.WHITE\n) -> dict[str, Any]:\n    """Apply ManimCE VMobject defaults without changing native Noon IR defaults."""\n    result = dict(kwargs)\n    result.setdefault(\n        "fill",\n        _base.Color(default_color.red, default_color.green, default_color.blue, 0.0),\n    )\n    result.setdefault("stroke", default_color)\n''',
)
replace_once(
    'web/python/_manim_compat.py',
    'super().__init__(_ir.Circle(radius, **_manim_vmobject_kwargs(kwargs)))',
    '''super().__init__(\n            _ir.Circle(\n                radius,\n                **_manim_vmobject_kwargs(kwargs, default_color=_base.RED),\n            )\n        )''',
)
