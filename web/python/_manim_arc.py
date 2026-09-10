"""Thin Manim Arc adapters backed by shared Rust retained geometry."""

from __future__ import annotations

from typing import Any

from _noon_errors import engine_call

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared
from _manim_shared_geometry import _sector_component_count


def _require_geometry_host(name: str) -> None:
    if _shared._create_geometry_handle is None:
        raise RuntimeError(f"{name} requires the shared Rust authoring host")


def _finish_candidate(
    owner: _base.Mobject,
    candidate: object,
    name: str,
    options: dict[str, Any],
) -> None:
    color = options.pop("color", None)
    _shared._apply_shared_constructor_options(candidate, options)
    if color is not None:
        parsed = _shared._compat._as_color("color", color)
        _shared._apply_constructor_color(candidate, parsed)
    _shared._attach_geometry_options(owner, candidate, name)


class Arc(_compat.VMobject):
    """ManimCE-compatible Arc whose path construction is owned by shared Rust."""

    def __init__(
        self,
        radius: float | None = 1.0,
        start_angle: float = 0.0,
        angle: float = _base.TAU / 4.0,
        num_components: int = 9,
        arc_center: object = _base.ORIGIN,
        **kwargs: Any,
    ) -> None:
        _require_geometry_host("Arc")
        radius_value = 1.0 if radius is None else _shared._ir._finite_number("radius", radius)
        start_value = _shared._ir._finite_number("start_angle", start_angle)
        angle_value = _shared._ir._finite_number("angle", angle)
        component_count = _sector_component_count(num_components)
        center = _base._as_vec2(arc_center)
        candidate = engine_call(
            _shared._geometry_options.arc,
            radius_value,
            start_value,
            angle_value,
            component_count,
            center.x,
            center.y,
            operation="Arc",
        )
        _finish_candidate(self, candidate, "Arc", dict(kwargs))
        self.radius = radius_value
        self.start_angle = start_value
        self.angle = angle_value
        self.num_components = component_count
        self.arc_center = center


class ArcBetweenPoints(Arc):
    """Arc spanning two points; endpoint/radius geometry remains Rust-owned."""

    def __init__(
        self,
        start: object,
        end: object,
        angle: float = _base.TAU / 4.0,
        radius: float | None = None,
        **kwargs: Any,
    ) -> None:
        _require_geometry_host("ArcBetweenPoints")
        start_point = _base._as_vec2(start)
        end_point = _base._as_vec2(end)
        angle_value = _shared._ir._finite_number("angle", angle)
        radius_value = (
            None if radius is None else _shared._ir._finite_number("radius", radius)
        )
        options = dict(kwargs)
        component_count = _sector_component_count(options.pop("num_components", 9))
        candidate = engine_call(
            _shared._geometry_options.arcBetweenPoints,
            start_point.x,
            start_point.y,
            end_point.x,
            end_point.y,
            angle_value,
            radius_value,
            component_count,
            operation="ArcBetweenPoints",
        )
        metadata = engine_call(
            _shared._geometry_options.arcBetweenPointsMetadata,
            start_point.x,
            start_point.y,
            end_point.x,
            end_point.y,
            angle_value,
            radius_value,
            operation="ArcBetweenPoints.metadata",
        )
        _finish_candidate(self, candidate, "ArcBetweenPoints", options)
        self.radius = float(metadata.radius)
        self.start_angle = 0.0
        self.angle = float(metadata.angle)
        self.num_components = component_count
        self.arc_center = _base.ORIGIN
