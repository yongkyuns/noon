"""ManimCE v0.21 PolarPlane facade over shared retained radial geometry.

Radius/azimuth subdivision and retained Circle/Line construction remain Rust-owned.
This module resolves public defaults, composes the ordinary retained family, and keeps
text/nonlinear surfaces explicit until their shared substrates are qualified.
"""

from __future__ import annotations

import json
import math
from typing import Any

import noon as _base
import _manim_axes as _axes
import _manim_compat as _compat
import _manim_number_plane as _number_plane

try:
    from js import noonCreatePolarPlaneGridPlan as _create_polar_grid_plan
except ImportError:
    _create_polar_grid_plan = None

_INSTALLED = False
_FRAME_Y_RADIUS = float(getattr(_base, "DEFAULT_FRAME_HEIGHT", 8.0)) / 2.0
_DEFAULT_AZIMUTH_STEPS = {
    "PI radians": 20.0,
    "TAU radians": 20.0,
    "degrees": 36.0,
    "gradians": 40.0,
    None: 1.0,
}


def _require_shared_polar_plane() -> None:
    if _create_polar_grid_plan is None:
        raise RuntimeError("PolarPlane requires Noon's browser shared-semantics runtime")


def _finite(name: str, value: object) -> float:
    try:
        result = float(value)
    except (TypeError, ValueError) as error:
        raise TypeError(f"{name} must be a finite number") from error
    if not math.isfinite(result):
        raise ValueError(f"{name} must be finite")
    return result


def _positive(name: str, value: object) -> float:
    result = _finite(name, value)
    if result <= 0.0:
        raise ValueError(f"{name} must be positive")
    return result


def _circle_from_snapshot(snapshot: dict[str, Any]) -> _compat.Circle:
    value = _axes._mobject_from_snapshot(_compat.Circle, snapshot)
    geometry = snapshot.get("geometry", {}).get("circle")
    if not isinstance(geometry, dict):
        raise TypeError("shared PolarPlane circle snapshot is malformed")
    value.radius = float(geometry["radius"])
    return value


def _line_group(wire: list[dict[str, Any]]) -> _compat.VGroup:
    return _compat.VGroup(*(_axes._line_from_snapshot(item["snapshot"]) for item in wire))


def _circle_group(wire: list[dict[str, Any]]) -> _compat.VGroup:
    return _compat.VGroup(*(_circle_from_snapshot(item["snapshot"]) for item in wire))


class PolarPlane(_axes.Axes):
    """Static retained PolarPlane with ManimCE v0.21 radial grid semantics."""

    def __init__(
        self,
        radius_max: float = _FRAME_Y_RADIUS,
        size: float | None = None,
        radius_step: float = 1.0,
        azimuth_step: float | None = None,
        azimuth_units: str | None = "PI radians",
        azimuth_compact_fraction: bool = True,
        azimuth_offset: float = 0.0,
        azimuth_direction: str = "CCW",
        azimuth_label_buff: float = float(getattr(_base, "SMALL_BUFF", 0.1)),
        azimuth_label_font_size: float = 24.0,
        radius_config: dict[str, Any] | None = None,
        background_line_style: dict[str, Any] | None = None,
        faded_line_style: dict[str, Any] | None = None,
        faded_line_ratio: int = 1,
        make_smooth_after_applying_functions: bool = True,
        **kwargs: Any,
    ) -> None:
        _require_shared_polar_plane()
        if azimuth_units not in _DEFAULT_AZIMUTH_STEPS:
            raise ValueError(
                "Invalid azimuth units. Expected one of: PI radians, TAU radians, degrees, gradians or None."
            )
        if azimuth_direction not in ("CW", "CCW"):
            raise ValueError("Invalid azimuth units. Expected one of: CW, CCW.")
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported PolarPlane option(s): {unsupported}")

        resolved_radius_max = _positive("PolarPlane radius_max", radius_max)
        resolved_radius_step = _positive("PolarPlane radius_step", radius_step)
        resolved_size = (
            2.0 * resolved_radius_max
            if size is None
            else _positive("PolarPlane size", size)
        )
        resolved_azimuth_step = (
            _DEFAULT_AZIMUTH_STEPS[azimuth_units]
            if azimuth_step is None
            else _positive("PolarPlane azimuth_step", azimuth_step)
        )
        resolved_azimuth_offset = _finite("PolarPlane azimuth_offset", azimuth_offset)
        resolved_label_buff = _finite("PolarPlane azimuth_label_buff", azimuth_label_buff)
        resolved_label_font_size = _positive(
            "PolarPlane azimuth_label_font_size", azimuth_label_font_size
        )
        ratio = _number_plane._faded_line_ratio(faded_line_ratio)

        radius_axis_config: dict[str, Any] = {
            "stroke_width": 2.0,
            "include_ticks": False,
            "include_tip": False,
            "line_to_number_buff": float(getattr(_base, "SMALL_BUFF", 0.1)),
            "label_direction": _base.DL,
            "font_size": 24.0,
        }
        radius_axis_config.update(
            _number_plane._axis_metadata(radius_config, "radius_config")
        )
        background = _number_plane._grid_style(
            background_line_style,
            {
                "stroke_color": _base.BLUE_D,
                "stroke_width": 2.0,
                "stroke_opacity": 1.0,
            },
            "background_line_style",
        )
        faded = (
            _number_plane._faded_style(background)
            if faded_line_style is None
            else _number_plane._grid_style(
                faded_line_style,
                _number_plane._DEFAULT_GRID_LINE_STYLE,
                "faded_line_style",
            )
        )

        radius_range = [
            -resolved_radius_max,
            resolved_radius_max,
            resolved_radius_step,
        ]
        super().__init__(
            x_range=radius_range,
            y_range=radius_range,
            x_length=resolved_size,
            y_length=resolved_size,
            axis_config=_number_plane._axis_geometry(radius_axis_config),
            tips=False,
        )

        request = {
            "radius_max": resolved_radius_max,
            "radius_step": resolved_radius_step,
            "size": resolved_size,
            "azimuth_step": resolved_azimuth_step,
            "azimuth_offset": resolved_azimuth_offset,
            "faded_line_ratio": ratio,
            "background_style": _number_plane._style_wire(background),
            "faded_style": _number_plane._style_wire(faded),
        }
        assert _create_polar_grid_plan is not None
        plan = _create_polar_grid_plan(
            json.dumps(request, separators=(",", ":"), allow_nan=False)
        )
        wire = json.loads(str(plan.geometryJson()))

        self._radial_lines = _line_group(wire["radial_lines"])
        self._circles = _circle_group(wire["circles"])
        self._faded_radial_lines = _line_group(wire["faded_radial_lines"])
        self._faded_circles = _circle_group(wire["faded_circles"])
        self.background_lines = _compat.VGroup(*self._radial_lines, *self._circles)
        self.faded_lines = _compat.VGroup(
            *self._faded_radial_lines, *self._faded_circles
        )

        self.remove(self.x_axis, self.y_axis)
        self.add(self.faded_lines, self.background_lines, self.x_axis, self.y_axis)

        self.radius_max = resolved_radius_max
        self.size = resolved_size
        self.radius_step = resolved_radius_step
        self.azimuth_step = resolved_azimuth_step
        self.azimuth_units = azimuth_units
        self.azimuth_compact_fraction = bool(azimuth_compact_fraction)
        self.azimuth_offset = resolved_azimuth_offset
        self.azimuth_direction = azimuth_direction
        self.azimuth_label_buff = resolved_label_buff
        self.azimuth_label_font_size = resolved_label_font_size
        self.radius_config = radius_axis_config
        self.background_line_style = background
        self.faded_line_style = faded
        self.faded_line_ratio = ratio
        self.make_smooth_after_applying_functions = bool(
            make_smooth_after_applying_functions
        )
        self._polar_plane_grid_plan = plan

    def copy(self) -> PolarPlane:
        clone = object.__new__(type(self))
        clone.x_range = list(self.x_range)
        clone.y_range = list(self.y_range)
        clone.x_length = self.x_length
        clone.y_length = self.y_length
        clone.axis_config = _axes._plain_copy(self.axis_config)
        clone.x_axis_config = _axes._plain_copy(self.x_axis_config)
        clone.y_axis_config = _axes._plain_copy(self.y_axis_config)
        clone._base_request = _axes._plain_copy(self._base_request)
        clone._axes_plan = self._axes_plan
        clone._query_plan = self._query_plan
        clone.radius_max = self.radius_max
        clone.size = self.size
        clone.radius_step = self.radius_step
        clone.azimuth_step = self.azimuth_step
        clone.azimuth_units = self.azimuth_units
        clone.azimuth_compact_fraction = self.azimuth_compact_fraction
        clone.azimuth_offset = self.azimuth_offset
        clone.azimuth_direction = self.azimuth_direction
        clone.azimuth_label_buff = self.azimuth_label_buff
        clone.azimuth_label_font_size = self.azimuth_label_font_size
        clone.radius_config = _axes._plain_copy(self.radius_config)
        clone.background_line_style = dict(self.background_line_style)
        clone.faded_line_style = dict(self.faded_line_style)
        clone.faded_line_ratio = self.faded_line_ratio
        clone.make_smooth_after_applying_functions = self.make_smooth_after_applying_functions
        clone._polar_plane_grid_plan = self._polar_plane_grid_plan

        clone.x_axis = self.x_axis.copy()
        clone.y_axis = self.y_axis.copy()
        clone.axes = _compat.VGroup(clone.x_axis, clone.y_axis)
        clone._radial_lines = self._radial_lines.copy()
        clone._circles = self._circles.copy()
        clone._faded_radial_lines = self._faded_radial_lines.copy()
        clone._faded_circles = self._faded_circles.copy()
        clone.background_lines = _compat.VGroup(*clone._radial_lines, *clone._circles)
        clone.faded_lines = _compat.VGroup(
            *clone._faded_radial_lines, *clone._faded_circles
        )
        _compat.VGroup.__init__(
            clone,
            clone.faded_lines,
            clone.background_lines,
            clone.x_axis,
            clone.y_axis,
        )
        return clone

    def get_coordinate_labels(self, *args: object, **kwargs: Any):
        del args, kwargs
        raise NotImplementedError(
            "PolarPlane coordinate labels require retained number/MathTex labels"
        )

    def add_coordinates(self, *args: object, **kwargs: Any):
        del args, kwargs
        raise NotImplementedError(
            "PolarPlane.add_coordinates requires retained number/MathTex labels"
        )

    def get_vector(self, coords: object, **kwargs: Any):
        del coords, kwargs
        raise NotImplementedError(
            "PolarPlane.get_vector requires the retained Arrow compatibility surface"
        )

    def prepare_for_nonlinear_transform(self, num_inserted_curves: int = 50) -> PolarPlane:
        del num_inserted_curves
        raise NotImplementedError(
            "PolarPlane.prepare_for_nonlinear_transform requires retained pointwise nonlinear deformation"
        )


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _number_plane.install()
    setattr(_base, "PolarPlane", PolarPlane)
    setattr(_compat, "PolarPlane", PolarPlane)
    if "PolarPlane" not in _base.__all__:
        _base.__all__.append("PolarPlane")
    _INSTALLED = True
