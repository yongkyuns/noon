from pathlib import Path

phase = Path("web/python/_manim_phase_b.py")
text = phase.read_text()
old = '''    # Manim VMobjects default to an invisible white fill and visible white
    # stroke. `None` means "use the inherited/default color", not "disable
    # the paint layer". Native Noon constructors keep their own defaults;
    # this function is installed only by the Manim compatibility frontend.
    if "fill" not in kwargs:
        kwargs["fill"] = _with_alpha(_base.WHITE, 0.0)
    if fill_color is not _MISSING and fill_color is not None:
        kwargs["fill"] = _as_color("fill_color", fill_color)

    if "stroke" not in kwargs:
        kwargs["stroke"] = _base.WHITE
    if stroke_color is not _MISSING and stroke_color is not None:
        kwargs["stroke"] = _as_color("stroke_color", stroke_color)

    stroke_width = kwargs.pop("stroke_width", MANIM_DEFAULT_STROKE_WIDTH)
    kwargs["stroke_width"] = _manim_stroke_width(stroke_width)
    kwargs.setdefault("stroke_join", "miter")
    kwargs.setdefault("stroke_cap", "butt")

    raw = _ORIGINAL_MAKE_MOBJECT(geometry, **kwargs)
'''
new = '''    if fill_color is not _MISSING and fill_color is not None:
        kwargs["fill"] = _as_color("fill_color", fill_color)
    if stroke_color is not _MISSING and stroke_color is not None:
        kwargs["stroke"] = _as_color("stroke_color", stroke_color)

    # Only convert an authored Manim width that the facade explicitly supplied.
    # Native Noon IR constructors which omit stroke_width retain native defaults.
    if "stroke_width" in kwargs:
        kwargs["stroke_width"] = _manim_stroke_width(kwargs["stroke_width"])

    raw = _ORIGINAL_MAKE_MOBJECT(geometry, **kwargs)
'''
if text.count(old) != 1:
    raise SystemExit("global-default injection anchor mismatch")
phase.write_text(text.replace(old, new))

compat = Path("web/python/_manim_compat.py")
text = compat.read_text()
anchor = '''def _as_vec2(value: object) -> _base.Vec2:
'''
helper = '''def _manim_vmobject_kwargs(kwargs: dict[str, Any]) -> dict[str, Any]:
    """Apply ManimCE VMobject defaults without changing native Noon IR defaults."""
    result = dict(kwargs)
    result.setdefault("fill", _base.Color(1.0, 1.0, 1.0, 0.0))
    result.setdefault("stroke", _base.WHITE)
    result.setdefault("stroke_width", 4.0)
    result.setdefault("stroke_join", "miter")
    result.setdefault("stroke_cap", "butt")
    return result


def _as_vec2(value: object) -> _base.Vec2:
'''
if text.count(anchor) != 1:
    raise SystemExit("compat helper anchor mismatch")
text = text.replace(anchor, helper)

replacements = {
    'super().__init__(_ir.Circle(radius, **kwargs))': 'super().__init__(_ir.Circle(radius, **_manim_vmobject_kwargs(kwargs)))',
    'super().__init__(_ir.Rectangle(width, height, **kwargs))': 'super().__init__(_ir.Rectangle(width, height, **_manim_vmobject_kwargs(kwargs)))',
    'super().__init__(_ir.Line(start_value, end_value, **kwargs))': 'super().__init__(_ir.Line(start_value, end_value, **_manim_vmobject_kwargs(kwargs)))',
    'super().__init__(_ir.Path(path, **kwargs))': 'super().__init__(_ir.Path(path, **_manim_vmobject_kwargs(kwargs)))',
}
for old, new in replacements.items():
    if text.count(old) != 1:
        raise SystemExit(f"compat constructor anchor mismatch: {old}")
    text = text.replace(old, new)
compat.write_text(text)

Path("scripts/_fix_manim_style_scope.py").unlink()
Path(".github/workflows/_fix-manim-style-scope.yml").unlink()
