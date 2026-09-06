"""ManimCE v0.21 geometry/source-compatibility breadth over Noon primitives.

The public wrappers in this module stay on Noon's existing semantic geometry and
flat retained scene model. Besides exact affine Circle specializations, the module
contains small 2D wrappers needed by Manim's own documentation examples.
"""

from __future__ import annotations

import copy
import math
from typing import Any, Iterator

import noon as _base
import _manim_compat as _compat

DEFAULT_DOT_RADIUS = 0.08
PURE_YELLOW = _base.color_from_hex("#FFFF00")


class Dot(_compat.Circle):
    """Manim-compatible small filled circle."""

    def __init__(
        self,
        point: object = _base.ORIGIN,
        radius: float = DEFAULT_DOT_RADIUS,
        stroke_width: float = 0.0,
        fill_opacity: float = 1.0,
        color: _base.Color = _base.WHITE,
        **kwargs: Any,
    ) -> None:
        super().__init__(
            radius=radius,
            stroke_width=stroke_width,
            fill_opacity=fill_opacity,
            color=color,
            **kwargs,
        )
        self.move_to(_compat._as_vec2(point))


class Ellipse(_compat.Circle):
    """Manim-compatible affine circle with independent width and height.

    Noon keeps the renderer geometry analytic. ManimCE's observable VMobject layout,
    however, measures the point/control-point array of its eight cubic circle segments.
    For a rotated non-uniform ellipse that control hull is slightly larger than the
    true analytic extrema, so layout queries intentionally reproduce Manim's hull.
    """

    _CUBIC_HANDLE_FACTOR = 4.0 / 3.0 * math.tan(math.pi / 16.0)

    def __init__(self, width: float = 2.0, height: float = 1.0, **kwargs: Any) -> None:
        super().__init__(**kwargs)
        self.stretch_to_fit_width(float(width))
        self.stretch_to_fit_height(float(height))

    def _manim_layout_bounds(self) -> tuple[_base.Vec2, _base.Vec2]:
        raw = self._current_raw()
        radius = float(raw.geometry["circle"]["radius"])
        transform = raw.transform
        scale_x = float(transform["scale"]["x"])
        scale_y = float(transform["scale"]["y"])
        rotation = float(transform["rotation"])
        translation_x = float(transform["translation"]["x"])
        translation_y = float(transform["translation"]["y"])
        sine = math.sin(rotation)
        cosine = math.cos(rotation)
        factor = self._CUBIC_HANDLE_FACTOR

        points: list[_base.Vec2] = []
        for index in range(8):
            start_angle = index * math.pi / 4.0
            end_angle = (index + 1) * math.pi / 4.0
            start = _base.Vec2(math.cos(start_angle), math.sin(start_angle))
            end = _base.Vec2(math.cos(end_angle), math.sin(end_angle))
            start_tangent = _base.Vec2(-math.sin(start_angle), math.cos(start_angle))
            end_tangent = _base.Vec2(-math.sin(end_angle), math.cos(end_angle))
            control1 = start + factor * start_tangent
            control2 = end - factor * end_tangent

            for point in (start, control1, control2, end):
                x = radius * point.x * scale_x
                y = radius * point.y * scale_y
                points.append(
                    _base.Vec2(
                        x * cosine - y * sine + translation_x,
                        x * sine + y * cosine + translation_y,
                    )
                )

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

    @property
    def width(self) -> float:
        minimum, maximum = self._manim_layout_bounds()
        return maximum.x - minimum.x

    @width.setter
    def width(self, value: float) -> None:
        self.scale_to_fit_width(float(value))

    @property
    def height(self) -> float:
        minimum, maximum = self._manim_layout_bounds()
        return maximum.y - minimum.y

    @height.setter
    def height(self, value: float) -> None:
        self.scale_to_fit_height(float(value))


class Triangle(_compat.Path):
    """Manim-compatible equilateral ``Triangle`` with RegularPolygon defaults."""

    def __init__(self, **kwargs: Any) -> None:
        points = [
            _base.Vec2(
                math.cos(math.pi / 2.0 + index * _base.TAU / 3.0),
                math.sin(math.pi / 2.0 + index * _base.TAU / 3.0),
            )
            for index in range(3)
        ]
        path = _base.VectorPath().move_to(points[0])
        for point in points[1:]:
            path.line_to(point)
        super().__init__(path.close(), **kwargs)


def _world_point(mobject: _base.Mobject, point: _base.Vec2) -> _base.Vec2:
    raw = mobject._current_raw()
    transform = raw.transform
    sx = float(transform["scale"]["x"])
    sy = float(transform["scale"]["y"])
    rotation = float(transform["rotation"])
    tx = float(transform["translation"]["x"])
    ty = float(transform["translation"]["y"])
    x = point.x * sx
    y = point.y * sy
    cosine = math.cos(rotation)
    sine = math.sin(rotation)
    return _base.Vec2(
        x * cosine - y * sine + tx,
        x * sine + y * cosine + ty,
    )


def _line_get_start(self: _compat.Line) -> _base.Vec2:
    raw = self._current_raw()
    point = raw.geometry["line"]["start"]
    return _world_point(self, _base.Vec2(float(point["x"]), float(point["y"])))


def _line_get_end(self: _compat.Line) -> _base.Vec2:
    raw = self._current_raw()
    point = raw.geometry["line"]["end"]
    return _world_point(self, _base.Vec2(float(point["x"]), float(point["y"])))


def _mobject_get_color(self: _base.Mobject) -> _base.Color:
    style = self._current_raw().style
    for channel in ("stroke", "fill"):
        color = style.get(channel)
        if color is not None:
            return _base.Color(
                float(color["red"]),
                float(color["green"]),
                float(color["blue"]),
                float(color.get("alpha", 1.0)),
            )
    return _base.WHITE


def _group_get_color(self: _compat.Group) -> _base.Color:
    leaves = _compat._leaf_mobjects(self)
    return _base.WHITE if not leaves else _mobject_get_color(leaves[0])


def _copy_group_without_constructor(self: _compat.Group) -> _compat.Group:
    """Clone retained families without re-running arbitrary subclass constructors.

    Manim group subclasses commonly keep named references to children (for example,
    ``Arrow._shaft`` and ``Arrow._tip``). Reconstructing ``type(self)(*children)`` is
    incorrect for such classes because their constructor arguments need not be child
    mobjects. Seed ``deepcopy`` with the cloned family tree so subclass attributes are
    remapped to the corresponding detached children while runtime/Scene ownership is
    left behind by each leaf's normal ``copy`` implementation.
    """

    clone = object.__new__(type(self))
    cloned_members = [member.copy() for member in self.submobjects]
    clone.submobjects = cloned_members
    memo: dict[int, object] = {id(self): clone}

    def map_family(original: object, copied: object) -> None:
        memo[id(original)] = copied
        if isinstance(original, _compat.Group) and isinstance(copied, _compat.Group):
            for original_child, copied_child in zip(
                original.submobjects, copied.submobjects, strict=True
            ):
                map_family(original_child, copied_child)

    for original, copied in zip(self.submobjects, cloned_members, strict=True):
        map_family(original, copied)

    for name, value in self.__dict__.items():
        if name != "submobjects":
            setattr(clone, name, copy.deepcopy(value, memo))
    return clone


class Arrow(_compat.Group):
    """2D Manim-style arrow composed from a Line and a triangular tip.

    This gives documentation examples a retained family rather than a special renderer
    primitive. Exact arrow-tip raster parity remains tracked separately from source
    compatibility and family transforms.
    """

    def __init__(
        self,
        start: object = _base.LEFT,
        end: object = _base.RIGHT,
        buff: float = 0.25,
        color: _base.Color = _base.WHITE,
        **kwargs: Any,
    ) -> None:
        start_point = _compat._as_vec2(start)
        end_point = _compat._as_vec2(end)
        delta = end_point - start_point
        length = delta.length()
        if length <= 0.0:
            raise ValueError("Arrow start and end must differ")
        direction = delta / length
        trim = min(max(float(buff), 0.0), length * 0.49)
        shaft_start = start_point + direction * trim
        shaft_end = end_point - direction * trim
        shaft = _compat.Line(shaft_start, shaft_end, color=color, **kwargs)
        tip = Triangle(color=color, fill_opacity=1.0, stroke_opacity=0.0)
        tip.scale(min(0.18, length * 0.12))
        tip.rotate(math.atan2(direction.y, direction.x) - math.pi / 2.0)
        tip.move_to(shaft_end)
        self._shaft = shaft
        self._tip = tip
        super().__init__(shaft, tip)

    def get_start(self) -> _base.Vec2:
        return _line_get_start(self._shaft)

    def get_end(self) -> _base.Vec2:
        return self._tip.get_center()


class Text(_compat.Rectangle):
    """Single retained text handle used until the glyph backend lands.

    The important compatibility property for the upstream examples is that a text
    object behaves as one Mobject for animations such as Indicate while remaining
    iterable for LaggedStartMap. Rendering is intentionally not marked parity-qualified
    until the dedicated text/glyph implementation replaces this temporary geometry.
    """

    def __init__(
        self,
        text: object,
        font_size: float = 48.0,
        color: _base.Color = _base.WHITE,
        **kwargs: Any,
    ) -> None:
        value = str(text)
        scale = max(float(font_size), 1.0) / 48.0
        visible = max(len(value), 1)
        width = max(0.46 * visible + 0.08 * max(visible - 1, 0), 0.3) * scale
        height = 0.72 * scale
        super().__init__(
            width=width,
            height=height,
            color=color,
            fill_opacity=1.0,
            stroke_opacity=0.0,
        )
        self.text = value
        self.font_size = float(font_size)
        self._text_kwargs = dict(kwargs)

    def __iter__(self) -> Iterator[_base.Mobject]:
        yield self

    def __len__(self) -> int:
        return 1

    def __getitem__(self, index: int) -> _base.Mobject:
        if index in (0, -1):
            return self
        raise IndexError(index)


class Tex(Text):
    """Source-compatible Tex Mobject pending exact LaTeX glyph rendering."""


class MathTex(Tex):
    """Source-compatible MathTex Mobject pending exact LaTeX glyph rendering."""


def _public_bound_method_name(source: object, method: object) -> str:
    """Recover the public attribute name for a bound compatibility method.

    Compatibility helpers are often installed onto public methods after definition,
    so ``method.__name__`` can expose an internal helper name such as
    ``_vmobject_set_color``. ApplyMethod needs the public name to invoke the same
    operation on its detached target copy.
    """

    implementation = getattr(method, "__func__", None)
    if implementation is not None:
        for owner in type(source).__mro__:
            for candidate, attribute in owner.__dict__.items():
                if not candidate.startswith("_") and attribute is implementation:
                    return candidate

    name = getattr(method, "__name__", None)
    if not isinstance(name, str):
        raise TypeError("ApplyMethod requires a bound Mobject/Group method")
    return name


class ApplyMethod:
    """Manim ApplyMethod adapter over Noon's existing target-state animation builder."""

    def __new__(cls, method: object, *args: Any, **kwargs: Any):
        source = getattr(method, "__self__", None)
        if not isinstance(source, (_base.Mobject, _compat.Group)):
            raise TypeError("ApplyMethod requires a bound Mobject/Group method")
        name = _public_bound_method_name(source, method)

        # Resolve lazily because _manim_geometry is installed before _manim_animate.
        import _manim_animate as _animate

        builder = (
            _animate._AlignedGroupAnimationBuilder(source)
            if isinstance(source, _compat.Group)
            else _animate._AlignedAnimationBuilder(source)
        )
        target_method = getattr(builder.target, name)
        result = target_method(*args)
        if result is not None and result is not builder.target:
            raise TypeError("ApplyMethod target method must mutate and return self or None")
        builder.anim_args = dict(kwargs)
        builder.cannot_pass_args = True
        return builder


def _bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:
    """Use wrapper-specific Manim layout bounds while preserving flat runtime data."""

    leaves = _compat._leaf_mobjects(value)
    bounds: list[tuple[_base.Vec2, _base.Vec2]] = []
    for member in leaves:
        custom = getattr(member, "_manim_layout_bounds", None)
        bound = custom() if callable(custom) else _base._bounds(member._current_raw())
        if bound is not None:
            bounds.append(bound)
    if not bounds:
        return None
    return (
        _base.Vec2(
            min(bound[0].x for bound in bounds),
            min(bound[0].y for bound in bounds),
        ),
        _base.Vec2(
            max(bound[1].x for bound in bounds),
            max(bound[1].y for bound in bounds),
        ),
    )


def match_points(self: _base.Mobject, mobject: object) -> _base.Mobject:
    if not isinstance(self, _compat.Line) or not isinstance(mobject, _compat.Line):
        raise NotImplementedError(
            "match_points currently supports analytic Line-to-Line matching"
        )
    # The canonical callback path stages the Rust-derived effective transform in
    # its ordered overlay. A callback-local target is an opaque endpoint operand
    # and therefore never allocates semantic identity in the authoring store.
    try:
        from _manim_updaters import canonical_line_match

        if canonical_line_match(self, mobject):
            return self
    except ImportError:
        pass

    source_handle = getattr(self, "_semantic_handle", None)
    target_handle = getattr(mobject, "_semantic_handle", None)
    if (
        source_handle is None
        or target_handle is None
        or not bool(getattr(self, "_semantic_handle_fresh", False))
        or not bool(getattr(mobject, "_semantic_handle_fresh", False))
        or not hasattr(source_handle, "matchLine")
    ):
        raise NotImplementedError(
            "Line.match_points requires opaque shared semantic Line handles"
        )
    try:
        source_handle.matchLine(target_handle)
    except Exception as error:
        raise ValueError(str(error)) from None
    # Legacy bound snapshots remain an explicit migration projection. Canonical
    # scenes already observe the same handle and need no Python-side state copy.
    try:
        from _manim_semantic_handles import _sync_bound_transform

        _sync_bound_transform(self, source_handle)
    except ImportError:
        pass
    return self


def install() -> None:
    public = {
        "DEFAULT_DOT_RADIUS": DEFAULT_DOT_RADIUS,
        "PURE_YELLOW": PURE_YELLOW,
        "SMALL_BUFF": _base.SMALL_BUFF,
        "MED_SMALL_BUFF": _base.MED_SMALL_BUFF,
        "MED_LARGE_BUFF": _base.MED_LARGE_BUFF,
        "LARGE_BUFF": _base.LARGE_BUFF,
        "Dot": Dot,
        "Ellipse": Ellipse,
        "Triangle": Triangle,
        "Arrow": Arrow,
        "Text": Text,
        "Tex": Tex,
        "MathTex": MathTex,
        "ApplyMethod": ApplyMethod,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        if name not in {"DEFAULT_DOT_RADIUS", "PURE_YELLOW"}:
            setattr(_compat, name, value)

    _compat.Line.get_start = _line_get_start
    _compat.Line.get_end = _line_get_end
    _base.Mobject.get_color = _mobject_get_color
    _compat.Group.get_color = _group_get_color
    _compat.Group.copy = _copy_group_without_constructor

    # Existing compatibility layout methods resolve this module global at call time,
    # so the hook affects only Manim-facing authoring/layout and never renderer bounds.
    _compat._bounds_for = _bounds_for
    _base.Mobject.match_points = match_points

    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports


install()
