"""ManimCE v0.21 ImplicitFunction facade over Rust-owned adaptive contour authoring."""

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
    from js import noonCreateImplicitFunctionPlan as _create_implicit_plan
    from js import noonFinishImplicitFunctionPlan as _finish_implicit_plan
    from js import noonFinishImplicitFunctionPlanWithAxes as _finish_implicit_plan_with_axes
except ImportError:
    _create_mobject_handle = None
    _create_implicit_plan = None
    _finish_implicit_plan = None
    _finish_implicit_plan_with_axes = None

_INSTALLED = False


def _require_shared_implicit() -> None:
    if any(
        value is None
        for value in (
            _create_mobject_handle,
            _create_implicit_plan,
            _finish_implicit_plan,
            _finish_implicit_plan_with_axes,
        )
    ):
        raise RuntimeError("ImplicitFunction requires Noon's browser shared-semantics runtime")


def _implicit_range(
    value: Sequence[float] | None,
    default: tuple[float, float],
    name: str,
) -> list[float]:
    if value is None:
        return [float(default[0]), float(default[1])]
    values = [float(component) for component in value]
    if len(values) < 2:
        raise ValueError(f"{name} must contain at least two values")
    if not math.isfinite(values[0]) or not math.isfinite(values[1]) or values[1] <= values[0]:
        raise ValueError(f"{name} bounds must be finite and strictly increasing")
    return values


def _request_json(
    x_range: Sequence[float],
    y_range: Sequence[float],
    min_depth: int,
    max_quads: int,
    use_smoothing: bool,
) -> str:
    depth = int(min_depth)
    quads = int(max_quads)
    if depth < 0:
        raise ValueError("min_depth must be non-negative")
    if quads < 0:
        raise ValueError("max_quads must be non-negative")
    return json.dumps(
        {
            "x_range": [float(x_range[0]), float(x_range[1])],
            "y_range": [float(y_range[0]), float(y_range[1])],
            "min_depth": depth,
            "max_quads": quads,
            "use_smoothing": bool(use_smoothing),
        },
        separators=(",", ":"),
        allow_nan=False,
    )


def _attach_snapshot(target: _base.Mobject, snapshot_json: str) -> None:
    assert _create_mobject_handle is not None
    _shared._attach_shared_handle(target, _create_mobject_handle(snapshot_json))


def _apply_vmobject_style(target: _base.Mobject, kwargs: dict[str, Any]) -> None:
    color = kwargs.pop("color", None)
    _shared._apply_shared_constructor_kwargs(
        target,
        _compat._manim_vmobject_kwargs(kwargs, default_color=_base.WHITE),
    )
    if color is not None:
        target.set_color(_shared._phase_b._as_color("color", color))


class ImplicitFunction(_compat.VMobject):
    """Retained v0.21 implicit curve with authoring-time adaptive contour extraction."""

    def __init__(
        self,
        func: Callable[[float, float], float],
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
        resolved_x_range = _implicit_range(
            x_range,
            (-float(_base.DEFAULT_FRAME_WIDTH) / 2.0, float(_base.DEFAULT_FRAME_WIDTH) / 2.0),
            "x_range",
        )
        resolved_y_range = _implicit_range(
            y_range,
            (-float(_base.DEFAULT_FRAME_HEIGHT) / 2.0, float(_base.DEFAULT_FRAME_HEIGHT) / 2.0),
            "y_range",
        )
        request_json = _request_json(
            resolved_x_range,
            resolved_y_range,
            min_depth,
            max_quads,
            use_smoothing,
        )
        assert _create_implicit_plan is not None and _finish_implicit_plan is not None
        plan = _create_implicit_plan(request_json)
        snapshot_json = str(_finish_implicit_plan(plan, func))
        _attach_snapshot(self, snapshot_json)
        self.function = func
        self.x_range = resolved_x_range
        self.y_range = resolved_y_range
        self.min_depth = int(min_depth)
        self.max_quads = int(max_quads)
        self.use_smoothing = bool(use_smoothing)
        _apply_vmobject_style(self, kwargs)


def _plot_implicit_curve(
    self: _axes.Axes,
    func: Callable[[float, float], float],
    min_depth: int = 5,
    max_quads: int = 1500,
    **kwargs: Any,
) -> ImplicitFunction:
    _require_shared_implicit()
    if not callable(func):
        raise TypeError("Axes.plot_implicit_curve func must be callable")
    use_smoothing = bool(kwargs.pop("use_smoothing", True))
    x_range = [float(self.x_range[0]), float(self.x_range[1])]
    y_range = [float(self.y_range[0]), float(self.y_range[1])]
    request_json = _request_json(x_range, y_range, min_depth, max_quads, use_smoothing)
    assert _create_implicit_plan is not None and _finish_implicit_plan_with_axes is not None
    plan = _create_implicit_plan(request_json)
    snapshot_json = str(
        _finish_implicit_plan_with_axes(
            plan,
            func,
            json.dumps(self._base_request, separators=(",", ":"), allow_nan=False),
            self.x_axis._axis_snapshot_json(),
            self.y_axis._axis_snapshot_json(),
        )
    )
    graph = object.__new__(ImplicitFunction)
    _attach_snapshot(graph, snapshot_json)
    graph.function = func
    graph.x_range = x_range
    graph.y_range = y_range
    graph.min_depth = int(min_depth)
    graph.max_quads = int(max_quads)
    graph.use_smoothing = use_smoothing
    graph.axes = self
    _apply_vmobject_style(graph, kwargs)
    return graph


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _axes.ImplicitFunction = ImplicitFunction
    _axes.Axes.plot_implicit_curve = _plot_implicit_curve
    for namespace in (_base, _compat):
        setattr(namespace, "ImplicitFunction", ImplicitFunction)
    if "ImplicitFunction" not in _base.__all__:
        _base.__all__.append("ImplicitFunction")
    _INSTALLED = True
