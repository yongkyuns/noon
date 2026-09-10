"""ManimCE v0.21 geometry/source-compatibility breadth over Noon primitives.

Geometry wrappers delegate to shared Rust semantic handles. Text is provided by
the shared text-resource wrappers; this module never substitutes vector geometry
for unsupported glyph rendering.
"""

from __future__ import annotations

from _noon_errors import engine_call

import copy
import math
from typing import Any

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
        from _manim_shared_geometry import _dot_init
        _dot_init(self, point, radius, stroke_width, fill_opacity, color, **kwargs)


class Ellipse(_compat.Circle):
    """Manim-compatible ellipse backed by the shared Rust constructor."""

    def __init__(self, width: float = 2.0, height: float = 1.0, **kwargs: Any) -> None:
        from _manim_shared_geometry import _ellipse_init
        _ellipse_init(self, width, height, **kwargs)


class Triangle(_compat.Path):
    """Manim-compatible equilateral ``Triangle`` with RegularPolygon defaults."""

    def __init__(self, **kwargs: Any) -> None:
        from _manim_shared_geometry import _triangle_init
        _triangle_init(self, **kwargs)


def _line_get_start(self: _compat.Line) -> _base.Vec2:
    import _manim_semantic_handles as shared

    observed = shared._manim_line_endpoints_observation(self)
    if observed is not None:
        return _base.Vec2(float(observed.startX), float(observed.startY))
    raise RuntimeError("Mobject observation requires the shared Rust authoring host")


def _line_get_end(self: _compat.Line) -> _base.Vec2:
    import _manim_semantic_handles as shared

    observed = shared._manim_line_endpoints_observation(self)
    if observed is not None:
        return _base.Vec2(float(observed.endX), float(observed.endY))
    raise RuntimeError("Mobject observation requires the shared Rust authoring host")


def _mobject_get_color(self: _base.Mobject) -> _base.Color:
    import _manim_semantic_handles as shared

    observed = shared._manim_color_observation(self)
    if observed is not None:
        return _base.Color(
            float(observed.red),
            float(observed.green),
            float(observed.blue),
            float(observed.alpha),
        )
    raise RuntimeError("Mobject observation requires the shared Rust authoring host")


def _group_get_color(self: _compat.Group) -> _base.Color:
    leaves = _compat._leaf_mobjects(self)
    return _base.WHITE if not leaves else _mobject_get_color(leaves[0])


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
        start_point = _base._as_vec2(start)
        end_point = _base._as_vec2(end)
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

        # Resolve the defining module lazily to avoid an import cycle.
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
    engine_call(source_handle.matchLine, target_handle, operation="Line.match_points")
    return self
