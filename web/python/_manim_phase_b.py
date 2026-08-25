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


def _bind_raw(
    scene: _compat.Scene,
    member: _base.Mobject,
    *,
    key: str | None = None,
) -> None:
    """Bind one public wrapper using the canonical low-level Scene emitter."""

    raw_object = _base._ir.Scene.add(scene, member._current_raw(), key=key)
    member._bind(scene, raw_object)


def _scene_add(
    self: _compat.Scene,
    *mobjects: object,
    key: str | None = None,
) -> _base.Mobject | _compat.Scene:
    if not mobjects:
        return self

    leaves = [
        member
        for value in mobjects
        for member in _compat._leaf_mobjects(value)
    ]
    if key is not None and len(leaves) != 1:
        raise ValueError("an explicit key can only be used when adding one Mobject")

    for index, member in enumerate(leaves):
        newly_bound = member._scene is None
        if newly_bound:
            _bind_raw(self, member, key=key if index == 0 else None)
        elif member._scene is not self:
            raise ValueError("Mobject already belongs to another Scene")

        assert member._object is not None
        tracks = self._ensure_lifecycle_timeline_available(
            member._object, self._cursor, "Scene.add target"
        )
        if newly_bound and self._cursor > 0.0:
            self._add_presence_track(
                member._object,
                False,
                True,
                self._cursor,
                key=f"@scene-add:{member._object.id}:{self._cursor:g}",
            )
        elif tracks and not self._presence_at(member._object, self._cursor):
            self._add_presence_track(
                member._object,
                False,
                True,
                self._cursor,
                key=f"@scene-add:{member._object.id}:{self._cursor:g}",
            )

    for value in mobjects:
        self._register_top_level(value)

    # Preserve Noon's established one-object return as a backwards-compatible
    # extension. Typical Manim source ignores Scene.add's return value.
    return leaves[0] if len(leaves) == 1 else self


def _bind_introducer_target(self: _compat.Scene, target: object) -> None:
    if isinstance(target, _compat.Group):
        for member in _compat._leaf_mobjects(target):
            if member._scene is None:
                _bind_raw(self, member)
            elif member._scene is not self:
                raise ValueError("Mobject already belongs to another Scene")
        self._register_top_level(target)
        return

    if isinstance(target, _base.Mobject):
        if target._scene is None:
            _bind_raw(self, target)
        elif target._scene is not self:
            raise ValueError("Mobject already belongs to another Scene")
        self._register_top_level(target)


_compat.Scene.add = _scene_add
_compat.Scene._bind_introducer_target = _bind_introducer_target


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


def _with_alpha(color: _base.Color, alpha: float) -> _base.Color:
    return _base.Color(color.red, color.green, color.blue, alpha)


def _compat_make_mobject(
    geometry: dict[str, object],
    **kwargs: object,
):
    fill_color = kwargs.pop("fill_color", _MISSING)
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


def _vmobject_set_fill(
    self: _compat.VMobject,
    color: object = None,
    opacity: float | None = None,
    family: bool = True,
) -> _compat.VMobject:
    del family  # Leaf VMobjects have no Python submobjects in the current facade.
    raw = _base._raw_mobject(self._current_raw())

    # Hybrid compatibility rule: Manim's set_fill(opacity=...) preserves color,
    # while historical Noon set_fill(None) disables fill. Both remain useful.
    if color is not None:
        parsed = _as_color("fill color", color)
        previous_alpha = (
            raw.style["fill"]["alpha"] if raw.style["fill"] is not None else parsed.alpha
        )
        raw.style["fill"] = parsed.to_ir()
        raw.style["fill"]["alpha"] = previous_alpha
    elif opacity is None:
        raw.style["fill"] = None

    if opacity is not None:
        alpha = _opacity("fill opacity", opacity)
        if raw.style["fill"] is None:
            raw.style["fill"] = _with_alpha(_base.WHITE, alpha).to_ir()
        else:
            raw.style["fill"]["alpha"] = alpha
    return self._apply(raw)


def _vmobject_set_stroke(
    self: _compat.VMobject,
    color: object = None,
    width: float | None = None,
    opacity: float | None = None,
    family: bool = True,
) -> _compat.VMobject:
    del family
    raw = _base._raw_mobject(self._current_raw())

    # As with fill, an entirely empty call preserves Noon's explicit-disable escape
    # hatch; Manim-style width=/opacity= calls preserve the current stroke color.
    if color is not None:
        parsed = _as_color("stroke color", color)
        previous_alpha = (
            raw.style["stroke"]["alpha"]
            if raw.style["stroke"] is not None
            else parsed.alpha
        )
        raw.style["stroke"] = parsed.to_ir()
        raw.style["stroke"]["alpha"] = previous_alpha
    elif width is None and opacity is None:
        raw.style["stroke"] = None

    if width is not None:
        raw.style["stroke_width"] = _manim_stroke_width(width)
        if raw.style["stroke"] is None:
            raw.style["stroke"] = _base.WHITE.to_ir()

    if opacity is not None:
        alpha = _opacity("stroke opacity", opacity)
        if raw.style["stroke"] is None:
            raw.style["stroke"] = _with_alpha(_base.WHITE, alpha).to_ir()
        else:
            raw.style["stroke"]["alpha"] = alpha
    return self._apply(raw)


def _vmobject_set_opacity(
    self: _compat.VMobject,
    opacity: float,
    family: bool = True,
) -> _compat.VMobject:
    del family
    alpha = _opacity("opacity", opacity)
    raw = _base._raw_mobject(self._current_raw())
    for channel in ("fill", "stroke"):
        if raw.style[channel] is not None:
            raw.style[channel]["alpha"] = alpha
    return self._apply(raw)


def _vmobject_get_fill_opacity(self: _compat.VMobject) -> float:
    fill = self._current_raw().style["fill"]
    return 0.0 if fill is None else float(fill["alpha"])


def _vmobject_get_stroke_opacity(self: _compat.VMobject) -> float:
    stroke = self._current_raw().style["stroke"]
    return 0.0 if stroke is None else float(stroke["alpha"])


_compat.VMobject.set_fill = _vmobject_set_fill
_compat.VMobject.set_stroke = _vmobject_set_stroke
_compat.VMobject.set_opacity = _vmobject_set_opacity
_compat.VMobject.get_fill_opacity = _vmobject_get_fill_opacity
_compat.VMobject.get_stroke_opacity = _vmobject_get_stroke_opacity
