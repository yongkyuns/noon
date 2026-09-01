"""Phase-B glue for Manim source compatibility.

Kept separate from the compatibility surface while Phase B is under active development.
"""

from __future__ import annotations

import math

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


# Manim layout is based on actual VMobject point/curve extrema, not the control
# hull used for conservative renderer bounds. Detached browser objects already use
# the shared Rust semantic handle for tight local path bounds; this fallback keeps
# CPython and scene-owned compatibility objects on the same contract and handles
# affine transforms without rotating an already-axis-aligned bounding box.
def _transform_point(raw: _base._ir.Mobject, point: _base.Vec2) -> _base.Vec2:
    transform = raw.transform
    scale_x = float(transform["scale"]["x"])
    scale_y = float(transform["scale"]["y"])
    rotation = float(transform["rotation"])
    translation_x = float(transform["translation"]["x"])
    translation_y = float(transform["translation"]["y"])
    sine = math.sin(rotation)
    cosine = math.cos(rotation)
    x = point.x * scale_x
    y = point.y * scale_y
    return _base.Vec2(
        x * cosine - y * sine + translation_x,
        x * sine + y * cosine + translation_y,
    )


def _bounds_from_points(points: list[_base.Vec2]) -> tuple[_base.Vec2, _base.Vec2] | None:
    if not points:
        return None
    return (
        _base.Vec2(
            min(point.x for point in points),
            min(point.y for point in points),
        ),
        _base.Vec2(
            max(point.x for point in points),
            max(point.y for point in points),
        ),
    )


def _quadratic_point(
    p0: _base.Vec2, p1: _base.Vec2, p2: _base.Vec2, t: float
) -> _base.Vec2:
    u = 1.0 - t
    return _base.Vec2(
        u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x,
        u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y,
    )


def _cubic_point(
    p0: _base.Vec2,
    p1: _base.Vec2,
    p2: _base.Vec2,
    p3: _base.Vec2,
    t: float,
) -> _base.Vec2:
    u = 1.0 - t
    return _base.Vec2(
        u * u * u * p0.x
        + 3.0 * u * u * t * p1.x
        + 3.0 * u * t * t * p2.x
        + t * t * t * p3.x,
        u * u * u * p0.y
        + 3.0 * u * u * t * p1.y
        + 3.0 * u * t * t * p2.y
        + t * t * t * p3.y,
    )


def _cubic_derivative_roots(p0: float, p1: float, p2: float, p3: float) -> list[float]:
    a = -p0 + 3.0 * p1 - 3.0 * p2 + p3
    b = 2.0 * (p0 - 2.0 * p1 + p2)
    c = p1 - p0
    epsilon = 1.0e-14
    if abs(a) <= epsilon:
        if abs(b) <= epsilon:
            return []
        return [-c / b]
    discriminant = b * b - 4.0 * a * c
    if discriminant < 0.0:
        return []
    root = math.sqrt(discriminant)
    roots = [(-b + root) / (2.0 * a)]
    if root > epsilon:
        roots.append((-b - root) / (2.0 * a))
    return roots


def _vector_path_world_bounds(
    raw: _base._ir.Mobject, commands: list[object]
) -> tuple[_base.Vec2, _base.Vec2] | None:
    points: list[_base.Vec2] = []
    current: _base.Vec2 | None = None
    subpath_start: _base.Vec2 | None = None

    def include(point: _base.Vec2) -> None:
        points.append(point)

    for command in commands:
        if command == "close":
            if current is not None:
                include(current)
            if subpath_start is not None:
                include(subpath_start)
                current = subpath_start
            continue

        kind, payload = next(iter(command.items()))
        to_payload = payload.get("to")
        end = (
            None
            if to_payload is None
            else _transform_point(
                raw,
                _base.Vec2(float(to_payload["x"]), float(to_payload["y"])),
            )
        )

        if kind == "move_to":
            if end is not None:
                include(end)
                current = end
                subpath_start = end
            continue

        if kind == "line_to":
            if current is not None:
                include(current)
            if end is not None:
                include(end)
                current = end
            continue

        if kind == "quadratic_to" and end is not None:
            if current is None:
                include(end)
                current = end
                continue
            control_payload = payload["control"]
            control = _transform_point(
                raw,
                _base.Vec2(
                    float(control_payload["x"]),
                    float(control_payload["y"]),
                ),
            )
            start = current
            include(start)
            include(end)
            for axis in (0, 1):
                p0 = start[axis]
                p1 = control[axis]
                p2 = end[axis]
                denominator = p0 - 2.0 * p1 + p2
                if abs(denominator) <= 1.0e-14:
                    continue
                t = (p0 - p1) / denominator
                if 0.0 < t < 1.0:
                    include(_quadratic_point(start, control, end, t))
            current = end
            continue

        if kind == "cubic_to" and end is not None:
            if current is None:
                include(end)
                current = end
                continue
            control1_payload = payload["control1"]
            control2_payload = payload["control2"]
            control1 = _transform_point(
                raw,
                _base.Vec2(
                    float(control1_payload["x"]),
                    float(control1_payload["y"]),
                ),
            )
            control2 = _transform_point(
                raw,
                _base.Vec2(
                    float(control2_payload["x"]),
                    float(control2_payload["y"]),
                ),
            )
            start = current
            include(start)
            include(end)
            roots = _cubic_derivative_roots(
                start.x, control1.x, control2.x, end.x
            ) + _cubic_derivative_roots(start.y, control1.y, control2.y, end.y)
            for t in roots:
                if 0.0 < t < 1.0:
                    include(_cubic_point(start, control1, control2, end, t))
            current = end

    return _bounds_from_points(points)


def _manim_layout_bounds(raw: _base._ir.Mobject) -> tuple[_base.Vec2, _base.Vec2] | None:
    geometry = raw.geometry
    transform = raw.transform
    translation = _base.Vec2(
        float(transform["translation"]["x"]),
        float(transform["translation"]["y"]),
    )

    if "circle" in geometry:
        radius = float(geometry["circle"]["radius"])
        scale_x = float(transform["scale"]["x"])
        scale_y = float(transform["scale"]["y"])
        rotation = float(transform["rotation"])
        sine = math.sin(rotation)
        cosine = math.cos(rotation)
        half_width = radius * math.hypot(scale_x * cosine, scale_y * sine)
        half_height = radius * math.hypot(scale_x * sine, scale_y * cosine)
        return (
            _base.Vec2(translation.x - half_width, translation.y - half_height),
            _base.Vec2(translation.x + half_width, translation.y + half_height),
        )

    if "rectangle" in geometry:
        size = geometry["rectangle"]["size"]
        half = _base.Vec2(float(size["x"]) * 0.5, float(size["y"]) * 0.5)
        return _bounds_from_points(
            [
                _transform_point(raw, _base.Vec2(x, y))
                for x, y in (
                    (-half.x, -half.y),
                    (-half.x, half.y),
                    (half.x, -half.y),
                    (half.x, half.y),
                )
            ]
        )

    if "line" in geometry:
        line = geometry["line"]
        return _bounds_from_points(
            [
                _transform_point(
                    raw,
                    _base.Vec2(float(point["x"]), float(point["y"])),
                )
                for point in (line["start"], line["end"])
            ]
        )

    if "vector_path" in geometry:
        return _vector_path_world_bounds(raw, geometry["vector_path"]["commands"])

    return None


_base._bounds = _manim_layout_bounds


def _bind_raw(scene: _compat.Scene, member: _base.Mobject, *, key: str | None = None) -> None:
    member._bind_to_scene(scene, key=key)


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
    del family
    parsed = _as_color("color", color)
    raw = _base._raw_mobject(self._current_raw())
    for channel in ("fill", "stroke"):
        current = raw.style[channel]
        if current is None:
            # Preserve Noon's explicit disabled-layer extension. Ordinary Manim
            # VMobjects retain both paint layers, including zero-alpha fill.
            continue
        alpha = float(current["alpha"])
        raw.style[channel] = parsed.to_ir()
        raw.style[channel]["alpha"] = alpha
    return self._apply(raw)


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


_compat.VMobject.set_color = _vmobject_set_color
_compat.VMobject.set_fill = _vmobject_set_fill
_compat.VMobject.set_stroke = _vmobject_set_stroke
_compat.VMobject.set_opacity = _vmobject_set_opacity
_compat.VMobject.get_fill_opacity = _vmobject_get_fill_opacity
_compat.VMobject.get_stroke_opacity = _vmobject_get_stroke_opacity
