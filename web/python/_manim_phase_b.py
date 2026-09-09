"""Phase-B glue for Manim source compatibility.

Kept separate from the compatibility surface while Phase B is under active development.
"""

from __future__ import annotations

import noon as _base
import _manim_compat as _compat


class _GenericAnimationBuilder(_compat._CompatAnimationBuilder, _base._AnimationBuilder):
    """Make the generic proxy recognizable by the existing Noon play lowerer."""


# The property installed by _manim_compat resolves this module global at call time,
# so replacing the class preserves the generic proxy while also satisfying the
# low-level Scene.play isinstance check for Noon's animation builder.
_compat._CompatAnimationBuilder = _GenericAnimationBuilder

# Pinned ManimCE v0.21.0 primitive constructor defaults. Rectangle's dimensions
# are width=4, height=2; Square passes its side length explicitly and is unchanged.
MANIM_DEFAULT_RECTANGLE_WIDTH = 4.0
MANIM_DEFAULT_RECTANGLE_HEIGHT = 2.0
_compat.Rectangle.__init__.__defaults__ = (
    MANIM_DEFAULT_RECTANGLE_WIDTH,
    MANIM_DEFAULT_RECTANGLE_HEIGHT,
)


# Independent fill/stroke opacity does not require another serialized style field:
# Noon colors already carry alpha independently for fill and stroke. The overall
# style opacity remains available as a low-level multiplier for fades/compositing.
_ORIGINAL_MAKE_MOBJECT = _base._ir._make_mobject
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


def _as_color(name: str, value: object) -> _base.Color:
    if isinstance(value, _base.Color):
        return value
    if isinstance(value, (str, int)) and not isinstance(value, bool):
        try:
            return _base.color_from_hex(value)
        except (TypeError, ValueError) as error:
            raise ValueError(f"invalid {name}") from error
    raise TypeError(f"{name} must be a Color or #RRGGBB value")


def _compat_make_mobject(
    geometry: dict[str, object],
    **kwargs: object,
):
    fill_color = kwargs.pop("fill_color", _MISSING)
    stroke_color = kwargs.pop("stroke_color", _MISSING)
    fill_opacity = kwargs.pop("fill_opacity", None)
    stroke_opacity = kwargs.pop("stroke_opacity", None)

    if fill_color is not _MISSING and fill_color is not None:
        kwargs["fill"] = _as_color("fill_color", fill_color)
    if stroke_color is not _MISSING and stroke_color is not None:
        kwargs["stroke"] = _as_color("stroke_color", stroke_color)

    # Only convert an authored Manim width that the facade explicitly supplied.
    # Native Noon IR constructors which omit stroke_width retain native defaults.
    if "stroke_width" in kwargs:
        kwargs["stroke_width"] = _manim_stroke_width(kwargs["stroke_width"])
    kwargs.setdefault("stroke_width_mode", "screen_space")

    raw = _ORIGINAL_MAKE_MOBJECT(geometry, **kwargs)
    style = raw.style

    if fill_opacity is not None:
        alpha = _opacity("fill_opacity", fill_opacity)
        fill = style["fill"]
        if fill is None:
            fill = _base.WHITE.to_ir()
            style["fill"] = fill
        fill["alpha"] = alpha

    if stroke_opacity is not None:
        alpha = _opacity("stroke_opacity", stroke_opacity)
        stroke = style["stroke"]
        if stroke is None:
            stroke = _base.WHITE.to_ir()
            style["stroke"] = stroke
        stroke["alpha"] = alpha

    return raw


_base._ir._make_mobject = _compat_make_mobject


def _vmobject_set_color(
    self: _compat.VMobject,
    color: object,
    family: bool = True,
) -> _compat.VMobject:
    raise RuntimeError("Mobject paint requires the shared Rust authoring host")


def _vmobject_set_fill(
    self: _compat.VMobject,
    color: object = None,
    opacity: float | None = None,
    family: bool = True,
) -> _compat.VMobject:
    raise RuntimeError("Mobject paint requires the shared Rust authoring host")


def _vmobject_set_stroke(
    self: _compat.VMobject,
    color: object = None,
    width: float | None = None,
    opacity: float | None = None,
    family: bool = True,
) -> _compat.VMobject:
    raise RuntimeError("Mobject paint requires the shared Rust authoring host")


def _vmobject_set_opacity(
    self: _compat.VMobject,
    opacity: float,
    family: bool = True,
) -> _compat.VMobject:
    raise RuntimeError("Mobject paint requires the shared Rust authoring host")


def _vmobject_get_fill_opacity(self: _compat.VMobject) -> float:
    raise RuntimeError("Mobject paint requires the shared Rust authoring host")


def _vmobject_get_stroke_opacity(self: _compat.VMobject) -> float:
    raise RuntimeError("Mobject paint requires the shared Rust authoring host")


_compat.VMobject.set_color = _vmobject_set_color
_compat.VMobject.set_fill = _vmobject_set_fill
_compat.VMobject.set_stroke = _vmobject_set_stroke
_compat.VMobject.set_opacity = _vmobject_set_opacity
_compat.VMobject.get_fill_opacity = _vmobject_get_fill_opacity
_compat.VMobject.get_stroke_opacity = _vmobject_get_stroke_opacity
