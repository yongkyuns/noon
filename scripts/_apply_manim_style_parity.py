from pathlib import Path

phase = Path("web/python/_manim_phase_b.py")
text = phase.read_text()

old = '''_ORIGINAL_MAKE_MOBJECT = _base._ir._make_mobject
_MISSING = object()


def _opacity(name: str, value: object) -> float:
    return _base._ir._unit_interval(name, value)
'''
new = '''_ORIGINAL_MAKE_MOBJECT = _base._ir._make_mobject
_MISSING = object()

# Pinned ManimCE v0.21.0 Cairo presentation contract. Cairo converts
# VMobject stroke widths to scene units with this multiplier and AUTO
# leaves its native miter-join / butt-cap defaults in effect.
MANIM_CAIRO_LINE_WIDTH_MULTIPLE = 0.01
MANIM_DEFAULT_STROKE_WIDTH = 4.0


def _manim_stroke_width(value: object) -> float:
    width = _base._ir._finite_number("stroke width", value)
    if width < 0.0:
        raise ValueError("stroke width must be non-negative")
    return width * MANIM_CAIRO_LINE_WIDTH_MULTIPLE


def _opacity(name: str, value: object) -> float:
    return _base._ir._unit_interval(name, value)
'''
if text.count(old) != 1:
    raise SystemExit("phase-b constants anchor mismatch")
text = text.replace(old, new)

old = '''    fill_color = kwargs.pop("fill_color", _MISSING)
    stroke_color = kwargs.pop("stroke_color", _MISSING)
    fill_opacity = kwargs.pop("fill_opacity", None)
    stroke_opacity = kwargs.pop("stroke_opacity", None)

    if fill_color is not _MISSING:
        kwargs["fill"] = None if fill_color is None else _as_color("fill_color", fill_color)
    if stroke_color is not _MISSING:
        kwargs["stroke"] = (
            None if stroke_color is None else _as_color("stroke_color", stroke_color)
        )

    raw = _ORIGINAL_MAKE_MOBJECT(geometry, **kwargs)
'''
new = '''    fill_color = kwargs.pop("fill_color", _MISSING)
    stroke_color = kwargs.pop("stroke_color", _MISSING)
    fill_opacity = kwargs.pop("fill_opacity", None)
    stroke_opacity = kwargs.pop("stroke_opacity", None)

    # Manim VMobjects default to an invisible white fill and visible white
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
if text.count(old) != 1:
    raise SystemExit("compat constructor anchor mismatch")
text = text.replace(old, new)

old = '''    if width is not None:
        value = _base._ir._finite_number("stroke width", width)
        if value < 0.0:
            raise ValueError("stroke width must be non-negative")
        raw.style["stroke_width"] = value
        if raw.style["stroke"] is None:
            raw.style["stroke"] = _base.WHITE.to_ir()
'''
new = '''    if width is not None:
        raw.style["stroke_width"] = _manim_stroke_width(width)
        if raw.style["stroke"] is None:
            raw.style["stroke"] = _base.WHITE.to_ir()
'''
if text.count(old) != 1:
    raise SystemExit("set_stroke anchor mismatch")
text = text.replace(old, new)
phase.write_text(text)

handles = Path("web/python/_manim_semantic_handles.py")
text = handles.read_text()
old = '''    if width is not None:
        value = _base._ir._finite_number("stroke width", width)
        if value < 0.0:
            raise ValueError("stroke width must be non-negative")
        handle.setStrokeWidth(value)
'''
new = '''    if width is not None:
        handle.setStrokeWidth(_phase_b._manim_stroke_width(width))
'''
if text.count(old) != 1:
    raise SystemExit("semantic-handle stroke anchor mismatch")
handles.write_text(text.replace(old, new))

smoke = Path("scripts/manim-compat-smoke.mjs")
text = smoke.read_text()
anchor = '''const styleSource = `
from noon import *
'''
addition = '''const defaultVmobjectStyleSource = `
from noon import *

class DefaultVmobjectStyle(Scene):
    def construct(self):
        circle = Circle()
        assert abs(circle.get_fill_opacity() - 0.0) < 1e-12
        assert abs(circle.get_stroke_opacity() - 1.0) < 1e-12
        assert abs(circle.style["stroke_width"] - 0.04) < 1e-9
        assert circle.style["stroke_join"] == "miter"
        assert circle.style["stroke_cap"] == "butt"
        assert circle.style["fill"]["red"] == 1.0
        assert circle.style["stroke"]["red"] == 1.0

        explicit = Square(stroke_width=10)
        assert abs(explicit.style["stroke_width"] - 0.10) < 1e-9
        explicit.set_stroke(width=20)
        assert abs(explicit.style["stroke_width"] - 0.20) < 1e-9

        filled = Circle(fill_color=PINK, fill_opacity=0.5)
        assert abs(filled.get_fill_opacity() - 0.5) < 1e-12
        assert abs(filled.style["stroke_width"] - 0.04) < 1e-9
        self.add(circle, explicit, filled)
`;

const styleSource = `
from noon import *
'''
if text.count(anchor) != 1:
    raise SystemExit("style source anchor mismatch")
text = text.replace(anchor, addition)

anchor = '''  const style = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    styleSource,
  );
'''
addition = '''  const defaultVmobjectStyle = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    defaultVmobjectStyleSource,
  );
  assert.equal(defaultVmobjectStyle.kind, "scene_document");
  assert.equal(defaultVmobjectStyle.document.objects.length, 3);
  const defaultStyle = defaultVmobjectStyle.document.objects[0].style;
  assert.equal(defaultStyle.fill.alpha, 0);
  assert.equal(defaultStyle.stroke.alpha, 1);
  assert.ok(Math.abs(defaultStyle.stroke_width - 0.04) < 1e-7);
  assert.equal(defaultStyle.stroke_join, "miter");
  assert.equal(defaultStyle.stroke_cap, "butt");

  const style = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    styleSource,
  );
'''
if text.count(anchor) != 1:
    raise SystemExit("style evaluation anchor mismatch")
smoke.write_text(text.replace(anchor, addition))

Path("scripts/_apply_manim_style_parity.py").unlink()
Path(".github/workflows/_apply-manim-style-parity.yml").unlink()
