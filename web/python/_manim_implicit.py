"""ManimCE v0.21 implicit-function authoring over deterministic retained contours."""

from __future__ import annotations

import json
import math
from typing import Any, Callable, Sequence

import noon as _base
import _manim_axes as _axes
import _manim_compat as _compat
import _manim_semantic_handles as _shared

try:
    from js import noonCreateAuthoringMobjectHandle as _create_mobject_handle
    from js import noonImplicitFunctionSnapshotJson as _implicit_snapshot_json
except ImportError:
    _create_mobject_handle = None
    _implicit_snapshot_json = None

_INSTALLED = False


def _require_shared_implicit() -> None:
    if _create_mobject_handle is None or _implicit_snapshot_json is None:
        raise RuntimeError(
            "ImplicitFunction requires Noon's browser shared-semantics runtime"
        )


def _implicit_range(
    name: str,
    value: Sequence[float] | None,
    default_extent: float,
) -> list[float]:
    values = (
        [-float(default_extent) / 2.0, float(default_extent) / 2.0]
        if value is None
        else [float(component) for component in value]
    )
    if len(values) < 2:
        raise ValueError(f"{name} must contain at least two values")
    result = values[:2]
    if not all(math.isfinite(component) for component in result):
        raise ValueError(f"{name} bounds must be finite")
    if result[0] >= result[1]:
        raise ValueError(f"{name} bounds must be strictly increasing")
    return result


class ImplicitFunction(_compat.VMobject):
    """Retained ManimCE v0.21 2D implicit curve ``f(x, y) = 0``."""

    def __init__(
        self,
        func: Callable[[float, float], object],
        x_range: Sequence[float] | None = None,
        y_range: Sequence[float] | None = None,
        min_depth: int = 5,
        max_quads: int = 1500,
        use_smoothing: bool = True,
        **kwargs: Any,
    ) -> None:
        _require_shared_implicit()
        if not callable(func):
            raise TypeError("ImplicitFunction func must be callable")
        resolved_x = _implicit_range(
            "x_range", x_range, float(_base.DEFAULT_FRAME_WIDTH)
        )
        resolved_y = _implicit_range(
            "y_range", y_range, float(_base.DEFAULT_FRAME_HEIGHT)
        )
        resolved_min_depth = int(min_depth)
        resolved_max_quads = int(max_quads)
        if resolved_min_depth < 0:
            raise ValueError("min_depth must be non-negative")
        if resolved_max_quads < 0:
            raise ValueError("max_quads must be non-negative")

        color = kwargs.pop("color", None)
        request = {
            "x_range": resolved_x,
            "y_range": resolved_y,
            "min_depth": resolved_min_depth,
            "max_quads": resolved_max_quads,
            "use_smoothing": bool(use_smoothing),
        }
        assert _implicit_snapshot_json is not None
        snapshot_json = str(
            _implicit_snapshot_json(
                json.dumps(request, separators=(",", ":"), allow_nan=False),
                func,
            )
        )
        assert _create_mobject_handle is not None
        _shared._attach_shared_handle(self, _create_mobject_handle(snapshot_json))
        self.function = func
        self.min_depth = resolved_min_depth
        self.max_quads = resolved_max_quads
        self.use_smoothing = bool(use_smoothing)
        self.x_range = list(resolved_x)
        self.y_range = list(resolved_y)
        _shared._apply_shared_constructor_kwargs(
            self,
            _compat._manim_vmobject_kwargs(kwargs, default_color=_base.WHITE),
        )
        if color is not None:
            self.set_color(_axes._color("color", color))


def _axis_unit_size(axes: _axes.Axes, axis: int) -> float:
    origin = axes.c2p(0.0, 0.0)
    point = axes.c2p(1.0, 0.0) if axis == 0 else axes.c2p(0.0, 1.0)
    size = (point - origin).length()
    if not math.isfinite(size) or size <= 0.0:
        raise ValueError("Axes unit size must be finite and positive")
    return float(size)


def _plot_implicit_curve(
    self: _axes.Axes,
    func: Callable[[float, float], object],
    min_depth: int = 5,
    max_quads: int = 1500,
    **kwargs: Any,
) -> ImplicitFunction:
    """Pinned v0.21 linear-Axes implicit plotting over retained contour geometry."""

    graph = ImplicitFunction(
        func,
        x_range=self.x_range[:2],
        y_range=self.y_range[:2],
        min_depth=min_depth,
        max_quads=max_quads,
        **kwargs,
    )
    # Upstream explicitly stretches the generated coordinate-space graph about
    # ORIGIN, then shifts it to the Axes origin. Noon's retained tuple scale is
    # origin-space as well, which is the exact desired pivot for this helper.
    graph.scale((_axis_unit_size(self, 0), _axis_unit_size(self, 1)))
    graph.shift(self.get_origin())
    return graph


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    setattr(_base, "ImplicitFunction", ImplicitFunction)
    setattr(_compat, "ImplicitFunction", ImplicitFunction)
    _axes.Axes.plot_implicit_curve = _plot_implicit_curve
    if "ImplicitFunction" not in _base.__all__:
        _base.__all__.append("ImplicitFunction")
    _INSTALLED = True
