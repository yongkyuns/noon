"""ManimCE v0.21 NumberPlane facade over shared retained grid geometry.

Coordinate mapping and grid placement remain Rust-owned. This module only resolves the
public Manim defaults, composes ordinary retained Line leaves into the expected family
metadata, and keeps unsupported nonlinear/text surfaces explicit.
"""

from __future__ import annotations

import math
from typing import Any, Sequence

import noon as _base
import _manim_axes as _axes
import _manim_compat as _compat

try:
    from js import noonCreateNumberPlaneGridPlan as _create_grid_plan
except ImportError:
    _create_grid_plan = None

_INSTALLED = False
_FRAME_X_RADIUS = 64.0 / 9.0
_FRAME_Y_RADIUS = 4.0
_AXIS_GEOMETRY_KEYS = {
    "include_tip",
    "include_ticks",
    "tick_size",
    "numbers_with_elongated_ticks",
    "longer_tick_multiple",
    "exclude_origin_tick",
    "stroke_width",
    "color",
    "stroke_color",
}
_AXIS_METADATA_KEYS = {
    "line_to_number_buff",
    "label_direction",
    "font_size",
    "numbers_to_exclude",
}
_GRID_STYLE_KEYS = {"stroke_color", "stroke_width", "stroke_opacity"}


def _require_shared_plane() -> None:
    if _create_grid_plan is None:
        raise RuntimeError("NumberPlane requires Noon's browser shared-semantics runtime")


def _axis_metadata(value: dict[str, Any] | None, name: str) -> dict[str, Any]:
    if value is None:
        return {}
    if not isinstance(value, dict):
        raise TypeError(f"{name} must be a dict or None")
    result = dict(value)
    unsupported = sorted(set(result) - _AXIS_GEOMETRY_KEYS - _AXIS_METADATA_KEYS)
    if unsupported:
        raise NotImplementedError(
            f"unsupported NumberPlane {name} option(s): {', '.join(unsupported)}"
        )
    return result


def _axis_geometry(value: dict[str, Any]) -> dict[str, Any]:
    geometry: dict[str, Any] = {}
    for key, item in value.items():
        if key not in _AXIS_GEOMETRY_KEYS:
            continue
        actual_key = "color" if key == "stroke_color" else key
        geometry[actual_key] = item
    return geometry


def _grid_style(
    value: dict[str, Any] | None,
    defaults: dict[str, object],
    name: str,
) -> dict[str, object]:
    if value is None:
        supplied: dict[str, Any] = {}
    elif isinstance(value, dict):
        supplied = dict(value)
    else:
        raise TypeError(f"{name} must be a dict or None")
    unsupported = sorted(set(supplied) - _GRID_STYLE_KEYS)
    if unsupported:
        raise NotImplementedError(
            f"unsupported NumberPlane {name} option(s): {', '.join(unsupported)}"
        )
    result = dict(defaults)
    result.update(supplied)
    result["stroke_color"] = _axes._color("stroke_color", result["stroke_color"])
    width = float(result["stroke_width"])
    opacity = float(result["stroke_opacity"])
    if not math.isfinite(width) or width < 0.0:
        raise ValueError("NumberPlane stroke_width must be a finite non-negative value")
    if not math.isfinite(opacity) or not 0.0 <= opacity <= 1.0:
        raise ValueError("NumberPlane stroke_opacity must be finite and in [0, 1]")
    result["stroke_width"] = width
    result["stroke_opacity"] = opacity
    return result


def _faded_style(background: dict[str, object]) -> dict[str, object]:
    result = dict(background)
    for key, value in tuple(result.items()):
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            result[key] = float(value) * 0.5
    return result


def _style_wire(style: dict[str, object]) -> dict[str, object]:
    return {
        "color": _axes._rgba(style["stroke_color"]),
        "stroke_width": float(style["stroke_width"]),
        "stroke_opacity": float(style["stroke_opacity"]),
    }


def _line_group(wire: list[dict[str, Any]]) -> _compat.VGroup:
    return _compat.VGroup(
        *(_axes._line_from_snapshot(item["snapshot"]) for item in wire)
    )


class NumberPlane(_axes.Axes):
    """Static linear retained NumberPlane with ManimCE v0.21 grid semantics."""

    def __init__(
        self,
        x_range: Sequence[float] | None = None,
        y_range: Sequence[float] | None = None,
        x_length: float | None = None,
        y_length: float | None = None,
        background_line_style: dict[str, Any] | None = None,
        faded_line_style: dict[str, Any] | None = None,
        faded_line_ratio: int = 1,
        make_smooth_after_applying_functions: bool = True,
        **kwargs: Any,
    ) -> None:
        _require_shared_plane()

        axis_user = _axis_metadata(kwargs.pop("axis_config", None), "axis_config")
        x_axis_user = _axis_metadata(kwargs.pop("x_axis_config", None), "x_axis_config")
        y_axis_user = _axis_metadata(kwargs.pop("y_axis_config", None), "y_axis_config")
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported NumberPlane option(s): {unsupported}")

        axis_config: dict[str, Any] = {
            "stroke_width": 2.0,
            "include_ticks": False,
            "include_tip": False,
            "line_to_number_buff": float(getattr(_base, "SMALL_BUFF", 0.1)),
            "label_direction": _base.DR,
            "font_size": 24.0,
        }
        axis_config.update(axis_user)
        y_axis_config: dict[str, Any] = {"label_direction": _base.DR}
        y_axis_config.update(y_axis_user)

        resolved_x_range = _axes._range(
            x_range, (-_FRAME_X_RADIUS, _FRAME_X_RADIUS, 1.0)
        )
        resolved_y_range = _axes._range(
            y_range, (-_FRAME_Y_RADIUS, _FRAME_Y_RADIUS, 1.0)
        )
        resolved_x_length = (
            float(resolved_x_range[1] - resolved_x_range[0])
            if x_length is None
            else float(x_length)
        )
        resolved_y_length = (
            float(resolved_y_range[1] - resolved_y_range[0])
            if y_length is None
            else float(y_length)
        )
        if not math.isfinite(resolved_x_length) or resolved_x_length <= 0.0:
            raise ValueError("NumberPlane x_length must be finite and positive")
        if not math.isfinite(resolved_y_length) or resolved_y_length <= 0.0:
            raise ValueError("NumberPlane y_length must be finite and positive")

        background = _grid_style(
            background_line_style,
            {
                "stroke_color": _base.BLUE_D,
                "stroke_width": 2.0,
                "stroke_opacity": 1.0,
            },
            "background_line_style",
        )
        faded = (
            _faded_style(background)
            if faded_line_style is None
            else _grid_style(faded_line_style, background, "faded_line_style")
        )
        ratio = int(faded_line_ratio)
        if ratio < 0:
            raise ValueError("NumberPlane faded_line_ratio must be non-negative")

        super().__init__(
            x_range=resolved_x_range,
            y_range=resolved_y_range,
            x_length=resolved_x_length,
            y_length=resolved_y_length,
            axis_config=_axis_geometry(axis_config),
            x_axis_config=_axis_geometry(x_axis_user),
            y_axis_config=_axis_geometry(y_axis_config),
            tips=False,
        )

        request = {
            **self._base_request,
            "faded_line_ratio": ratio,
            "background_style": _style_wire(background),
            "faded_style": _style_wire(faded),
        }
        assert _create_grid_plan is not None
        plan = _create_grid_plan(
            __import__("json").dumps(request, separators=(",", ":"), allow_nan=False)
        )
        wire = __import__("json").loads(str(plan.geometryJson()))

        self.x_lines = _line_group(wire["x_lines"])
        self.y_lines = _line_group(wire["y_lines"])
        self.faded_x_lines = _line_group(wire["faded_x_lines"])
        self.faded_y_lines = _line_group(wire["faded_y_lines"])
        self.background_lines = _compat.VGroup(*self.x_lines, *self.y_lines)
        self.faded_lines = _compat.VGroup(*self.faded_x_lines, *self.faded_y_lines)

        # Match Manim's add_to_back(faded_lines, background_lines) ordering without
        # serializing an artificial plane node. `x_lines`/`y_lines` are metadata views
        # over the same leaves and are intentionally not added independently.
        self.submobjects = [self.faded_lines, self.background_lines, *self.axes]

        self.axis_config = axis_config
        self.x_axis_config = x_axis_user
        self.y_axis_config = y_axis_config
        self.background_line_style = background
        self.faded_line_style = faded
        self.faded_line_ratio = ratio
        self.make_smooth_after_applying_functions = bool(
            make_smooth_after_applying_functions
        )
        self._number_plane_grid_plan = plan

    def copy(self) -> NumberPlane:
        return type(self)(
            x_range=list(self.x_range),
            y_range=list(self.y_range),
            x_length=self.x_length,
            y_length=self.y_length,
            background_line_style=dict(self.background_line_style),
            faded_line_style=dict(self.faded_line_style),
            faded_line_ratio=self.faded_line_ratio,
            make_smooth_after_applying_functions=self.make_smooth_after_applying_functions,
            axis_config=dict(self.axis_config),
            x_axis_config=dict(self.x_axis_config),
            y_axis_config=dict(self.y_axis_config),
        )

    def prepare_for_nonlinear_transform(self, num_inserted_curves: int = 50) -> NumberPlane:
        del num_inserted_curves
        raise NotImplementedError(
            "NumberPlane.prepare_for_nonlinear_transform requires retained pointwise nonlinear deformation"
        )

    def get_vector(self, coords: Sequence[float], **kwargs: Any):
        del coords, kwargs
        raise NotImplementedError(
            "NumberPlane.get_vector requires the retained Arrow compatibility surface"
        )


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    setattr(_base, "NumberPlane", NumberPlane)
    setattr(_compat, "NumberPlane", NumberPlane)
    if "NumberPlane" not in _base.__all__:
        _base.__all__.append("NumberPlane")
    _INSTALLED = True
