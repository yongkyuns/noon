"""Thin Manim geometry adapters backed by shared Rust semantics.

This module patches only operations whose full observable geometry/layout contract is
already owned by Rust. Class identity and inheritance remain unchanged where an
established compatibility class already exists.
"""

from __future__ import annotations

import operator
from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_geometry as _geometry
import _manim_semantic_handles as _shared

_ORIGINAL_DOT_INIT = _geometry.Dot.__init__
_ORIGINAL_ELLIPSE_INIT = _geometry.Ellipse.__init__
_ORIGINAL_TRIANGLE_INIT = _geometry.Triangle.__init__
_INSTALLED = False


def _apply_candidate_color(candidate: object, color: object) -> None:
    if color is not None:
        parsed = _shared._phase_b._as_color("color", color)
        _shared._apply_constructor_color(candidate, parsed)


def _coordinate_mask(dim: int) -> tuple[float, float, float]:
    if isinstance(dim, bool) or not isinstance(dim, int):
        raise TypeError("dim must be an integer")
    if dim == 0:
        return (1.0, 0.0, 0.0)
    if dim == 1:
        return (0.0, 1.0, 0.0)
    raise NotImplementedError("Noon's shared 2D coordinate placement supports only x/y")


def _set_coord(
    self: _base.Mobject,
    value: float,
    dim: int,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Set a directional coordinate through shared critical-point placement."""

    coordinate = _shared._ir._finite_number("value", value)
    mask = _coordinate_mask(dim)
    point = _base.Vec2(coordinate if dim == 0 else 0.0, coordinate if dim == 1 else 0.0)
    return _shared._move_to(
        self,
        point,
        aligned_edge=direction,
        coor_mask=mask,
    )


def _set_x(
    self: _base.Mobject,
    x: float,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Set a directional x coordinate through shared ``set_coord`` semantics."""

    return _set_coord(self, x, 0, direction)


def _set_y(
    self: _base.Mobject,
    y: float,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Set a directional y coordinate through shared ``set_coord`` semantics."""

    return _set_coord(self, y, 1, direction)


def _match_coord(
    self: _base.Mobject,
    mobject: _base.Mobject,
    dim: int,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Match a directional coordinate through shared critical-point placement."""

    return _shared._move_to(
        self,
        mobject,
        aligned_edge=direction,
        coor_mask=_coordinate_mask(dim),
    )


def _match_x(
    self: _base.Mobject,
    mobject: _base.Mobject,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Match a directional x coordinate through shared ``match_coord`` semantics."""

    return _match_coord(self, mobject, 0, direction)


def _match_y(
    self: _base.Mobject,
    mobject: _base.Mobject,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Match a directional y coordinate through shared ``match_coord`` semantics."""

    return _match_coord(self, mobject, 1, direction)


def _rotate_about_origin(
    self: _base.Mobject,
    angle: float,
    axis: object = _compat.OUT,
    **kwargs: Any,
) -> _base.Mobject:
    """Rotate through the shared Rust transform path around Manim's origin."""

    return _shared._rotate(
        self,
        angle,
        axis,
        about_point=_base.ORIGIN,
        **kwargs,
    )


def _dot_init(
    self: _geometry.Dot,
    point: object = _base.ORIGIN,
    radius: float = _geometry.DEFAULT_DOT_RADIUS,
    stroke_width: float = 0.0,
    fill_opacity: float = 1.0,
    color: _base.Color = _base.WHITE,
    **kwargs: Any,
) -> None:
    if _shared._create_geometry_handle is None:
        _ORIGINAL_DOT_INIT(
            self,
            point=point,
            radius=radius,
            stroke_width=stroke_width,
            fill_opacity=fill_opacity,
            color=color,
            **kwargs,
        )
        return

    point_value = _compat._as_vec2(point)
    radius_value = _shared._ir._positive_number("radius", radius)
    options = dict(kwargs)
    options["stroke_width"] = stroke_width
    options["fill_opacity"] = fill_opacity
    candidate = _shared._geometry_options.dot(
        point_value.x, point_value.y, radius_value
    )
    _shared._apply_shared_constructor_options(candidate, options)
    _apply_candidate_color(candidate, color)
    _shared._attach_geometry_options(self, candidate, "Dot")
    self.radius = radius_value


def _triangle_init(self: _geometry.Triangle, **kwargs: Any) -> None:
    if _shared._create_geometry_handle is None:
        _ORIGINAL_TRIANGLE_INIT(self, **kwargs)
        return

    options = dict(kwargs)
    color = options.pop("color", None)
    candidate = _shared._geometry_options.triangle()
    _shared._apply_shared_constructor_options(candidate, options)
    _apply_candidate_color(candidate, color)
    _shared._attach_geometry_options(self, candidate, "Triangle")


def _ellipse_init(
    self: _geometry.Ellipse,
    width: float = 2.0,
    height: float = 1.0,
    **kwargs: Any,
) -> None:
    if _shared._create_geometry_handle is None:
        _ORIGINAL_ELLIPSE_INIT(self, width=width, height=height, **kwargs)
        return

    width_value = _shared._ir._positive_number("width", width)
    height_value = _shared._ir._positive_number("height", height)
    options = dict(kwargs)
    color = options.pop("color", None)
    scale = options.pop("scale", None)
    candidate = _shared._geometry_options.ellipse(width_value, height_value)
    _shared._apply_shared_constructor_options(candidate, options)
    if scale is not None:
        scale_value = _shared._ir._vec2("scale", scale)
        candidate.scaleBy(scale_value["x"], scale_value["y"])
    _apply_candidate_color(candidate, color)
    _shared._attach_geometry_options(self, candidate, "Ellipse")


class Elbow(_compat.VMobject):
    """Manim-compatible Elbow backed by shared Rust geometry."""

    def __init__(self, width: float = 0.2, angle: float = 0.0, **kwargs: Any) -> None:
        if _shared._create_geometry_handle is None:
            raise RuntimeError("Elbow requires the shared browser geometry bridge")

        width_value = _shared._ir._finite_number("width", width)
        angle_value = _shared._ir._finite_number("angle", angle)
        options = dict(kwargs)
        color = options.pop("color", None)
        candidate = _shared._geometry_options.elbow(width_value, angle_value)
        _shared._apply_shared_constructor_options(candidate, options)
        _apply_candidate_color(candidate, color)
        _shared._attach_geometry_options(self, candidate, "Elbow")


class RoundedRectangle(_compat.Rectangle):
    """Scalar-radius Manim RoundedRectangle backed by shared Rust geometry."""

    def __init__(self, corner_radius: float = 0.5, **kwargs: Any) -> None:
        if _shared._create_geometry_handle is None:
            raise RuntimeError("RoundedRectangle requires the shared browser geometry bridge")
        if isinstance(corner_radius, (list, tuple)):
            raise NotImplementedError(
                "per-corner RoundedRectangle radii are not exposed by the browser bridge yet"
            )

        options = dict(kwargs)
        width = _shared._ir._positive_number("width", options.pop("width", 4.0))
        height = _shared._ir._positive_number("height", options.pop("height", 2.0))
        radius = _shared._ir._finite_number("corner_radius", corner_radius)
        color = options.pop("color", None)
        candidate = _shared._geometry_options.roundedRectangle(width, height, radius)
        _shared._apply_shared_constructor_options(candidate, options)
        _apply_candidate_color(candidate, color)
        _shared._attach_geometry_options(self, candidate, "RoundedRectangle")
        self.width_value = width
        self.height_value = height
        self.corner_radius = radius


def _shape_matcher_buff(buff: object) -> tuple[float, float]:
    if isinstance(buff, (int, float)) and not isinstance(buff, bool):
        value = _shared._ir._finite_number("buff", buff)
        return value, value
    value = _compat._as_vec2(buff)
    return (
        _shared._ir._finite_number("buff.x", value.x),
        _shared._ir._finite_number("buff.y", value.y),
    )


def _shape_matcher_target(target: object):
    if not isinstance(target, _base.Mobject):
        raise TypeError("shape matcher target must be a Mobject")
    if isinstance(target, _compat.Group):
        shared = _shared._shared_family_layout_session(target)
        if shared is None:
            raise NotImplementedError(
                "shape matcher Group/VGroup targets require shared semantic family bounds"
            )
        return shared[0]
    handle = _shared._handle_for(target)
    if handle is None:
        raise NotImplementedError(
            "shape matcher target requires current shared semantic geometry"
        )
    return handle


def _shape_matcher_options(target: object, method: str, *args: float):
    if _shared._create_geometry_handle is None:
        raise RuntimeError("shape matchers require the shared browser geometry bridge")
    source = _shape_matcher_target(target)
    constructor = getattr(source, method, None)
    if constructor is None:
        raise NotImplementedError(
            "shape matcher target bridge does not expose shared matcher construction"
        )
    return constructor(*args)


class SurroundingRectangle(RoundedRectangle):
    """Manim SurroundingRectangle backed entirely by shared layout/matcher bounds."""

    def __init__(
        self,
        mobject: _base.Mobject,
        color: _base.Color = _geometry.PURE_YELLOW,
        buff: object = _base.SMALL_BUFF,
        corner_radius: float = 0.0,
        **kwargs: Any,
    ) -> None:
        buff_x, buff_y = _shape_matcher_buff(buff)
        radius = _shared._ir._finite_number("corner_radius", corner_radius)
        candidate = _shape_matcher_options(
            mobject,
            "beginSurroundingRectangle",
            buff_x,
            buff_y,
            radius,
        )
        options = dict(kwargs)
        _shared._apply_shared_constructor_options(candidate, options)
        _apply_candidate_color(candidate, color)
        _shared._attach_geometry_options(self, candidate, "SurroundingRectangle")
        self.buff = buff
        self.corner_radius = radius


class BackgroundRectangle(SurroundingRectangle):
    """Manim BackgroundRectangle using the same shared family-bounds matcher path."""

    def __init__(
        self,
        mobject: _base.Mobject,
        color: _base.Color = _base.BLACK,
        stroke_width: float = 0.0,
        stroke_opacity: float = 0.0,
        fill_opacity: float = 0.75,
        buff: object = 0.0,
        **kwargs: Any,
    ) -> None:
        options = dict(kwargs)
        corner_radius = _shared._ir._finite_number(
            "corner_radius", options.pop("corner_radius", 0.0)
        )
        buff_x, buff_y = _shape_matcher_buff(buff)
        fill_value = _shared._phase_b._opacity("fill_opacity", fill_opacity)
        candidate = _shape_matcher_options(
            mobject,
            "beginBackgroundRectangle",
            buff_x,
            buff_y,
            corner_radius,
            fill_value,
        )
        options["stroke_width"] = stroke_width
        options["stroke_opacity"] = stroke_opacity
        options["fill_opacity"] = fill_value
        _shared._apply_shared_constructor_options(candidate, options)
        _apply_candidate_color(candidate, color)
        _shared._attach_geometry_options(self, candidate, "BackgroundRectangle")
        self.buff = buff
        self.corner_radius = corner_radius


class Underline(_compat.Line):
    """Manim Underline backed by the shared Rust line-matcher semantics."""

    def __init__(
        self,
        mobject: _base.Mobject,
        buff: float = _base.SMALL_BUFF,
        **kwargs: Any,
    ) -> None:
        if _shared._create_geometry_handle is None:
            raise RuntimeError("Underline requires the shared browser shape-matcher bridge")
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("Underline target must be a Mobject")
        target_handle = _shared._handle_for(mobject)
        if target_handle is None:
            raise NotImplementedError(
                "Underline currently requires a target with shared semantic geometry"
            )

        buff_value = _shared._ir._finite_number("buff", buff)
        options = dict(kwargs)
        color = options.pop("color", None)
        context = _shared._live_constructor_context("Underline")
        candidate = (
            target_handle.beginUnderline(buff_value)
            if context is None
            else context.beginUnderline(target_handle, buff_value)
        )
        _shared._apply_shared_constructor_options(candidate, options)
        _apply_candidate_color(candidate, color)
        _shared._attach_geometry_options(self, candidate, "Underline")
        self.buff = buff_value


def _sector_component_count(value: object) -> int:
    if isinstance(value, bool):
        raise TypeError("num_components must be an integer")
    try:
        result = operator.index(value)
    except TypeError as error:
        raise TypeError("num_components must be an integer") from error
    if result < 2:
        raise ValueError("num_components must be at least 2")
    if result > 0xFFFFFFFF:
        raise ValueError("num_components is too large")
    return int(result)


def _sector_options(
    kwargs: dict[str, Any],
) -> tuple[dict[str, Any], int, _base.Vec2]:
    options = dict(kwargs)
    component_count = _sector_component_count(options.pop("num_components", 9))
    center = _compat._as_vec2(options.pop("arc_center", _base.ORIGIN))
    return options, component_count, center


def _apply_sector_style(
    candidate: object,
    kwargs: dict[str, Any],
    *,
    fill_opacity: float,
    stroke_width: float,
    color: object,
) -> None:
    options = dict(kwargs)
    options["fill_opacity"] = fill_opacity
    options["stroke_width"] = stroke_width
    _shared._apply_shared_constructor_options(candidate, options)
    _apply_candidate_color(candidate, color)


class AnnularSector(_compat.VMobject):
    """Manim-compatible annular sector backed by the shared Rust constructor."""

    def __init__(
        self,
        inner_radius: float = 1.0,
        outer_radius: float = 2.0,
        angle: float = _base.TAU / 4.0,
        start_angle: float = 0.0,
        fill_opacity: float = 1.0,
        stroke_width: float = 0.0,
        color: _base.Color = _base.WHITE,
        **kwargs: Any,
    ) -> None:
        if _shared._create_geometry_handle is None:
            raise RuntimeError("AnnularSector requires the shared browser geometry bridge")

        options, component_count, center = _sector_options(kwargs)
        inner = _shared._ir._finite_number("inner_radius", inner_radius)
        outer = _shared._ir._finite_number("outer_radius", outer_radius)
        angle_value = _shared._ir._finite_number("angle", angle)
        start_value = _shared._ir._finite_number("start_angle", start_angle)
        candidate = _shared._geometry_options.annularSector(
                inner,
                outer,
                angle_value,
                start_value,
                component_count,
                center.x,
                center.y,
        )
        _apply_sector_style(
            candidate,
            options,
            fill_opacity=fill_opacity,
            stroke_width=stroke_width,
            color=color,
        )
        _shared._attach_geometry_options(self, candidate, "AnnularSector")
        self.inner_radius = inner
        self.outer_radius = outer
        self.angle = angle_value
        self.start_angle = start_value
        self.num_components = component_count
        self.arc_center = center


class Sector(AnnularSector):
    """Manim-compatible circle sector backed by the shared Rust constructor."""

    def __init__(self, radius: float = 1.0, **kwargs: Any) -> None:
        if _shared._create_geometry_handle is None:
            raise RuntimeError("Sector requires the shared browser geometry bridge")

        options = dict(kwargs)
        fill_opacity = options.pop("fill_opacity", 1.0)
        stroke_width = options.pop("stroke_width", 0.0)
        color = options.pop("color", _base.WHITE)
        start_angle = options.pop("start_angle", 0.0)
        angle = options.pop("angle", _base.TAU / 4.0)
        options, component_count, center = _sector_options(options)
        radius_value = _shared._ir._finite_number("radius", radius)
        angle_value = _shared._ir._finite_number("angle", angle)
        start_value = _shared._ir._finite_number("start_angle", start_angle)
        candidate = _shared._geometry_options.sector(
                radius_value,
                angle_value,
                start_value,
                component_count,
                center.x,
                center.y,
        )
        _apply_sector_style(
            candidate,
            options,
            fill_opacity=fill_opacity,
            stroke_width=stroke_width,
            color=color,
        )
        _shared._attach_geometry_options(self, candidate, "Sector")
        self.inner_radius = 0.0
        self.outer_radius = radius_value
        self.angle = angle_value
        self.start_angle = start_value
        self.num_components = component_count
        self.arc_center = center


class Annulus(_compat.VMobject):
    """Manim-compatible annulus backed by the shared Rust constructor."""

    def __init__(
        self,
        inner_radius: float = 1.0,
        outer_radius: float = 2.0,
        fill_opacity: float = 1.0,
        stroke_width: float = 0.0,
        color: _base.Color = _base.WHITE,
        mark_paths_closed: bool = False,
        **kwargs: Any,
    ) -> None:
        if _shared._create_geometry_handle is None:
            raise RuntimeError("Annulus requires the shared browser geometry bridge")

        options, component_count, center = _sector_options(kwargs)
        inner = _shared._ir._finite_number("inner_radius", inner_radius)
        outer = _shared._ir._finite_number("outer_radius", outer_radius)
        candidate = _shared._geometry_options.annulus(
                inner,
                outer,
                component_count,
                center.x,
                center.y,
        )
        _apply_sector_style(
            candidate,
            options,
            fill_opacity=fill_opacity,
            stroke_width=stroke_width,
            color=color,
        )
        _shared._attach_geometry_options(self, candidate, "Annulus")
        self.inner_radius = inner
        self.outer_radius = outer
        self.mark_paths_closed = bool(mark_paths_closed)
        self.num_components = component_count
        self.arc_center = center


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True
    _base.Mobject.set_coord = _set_coord
    _base.Mobject.set_x = _set_x
    _base.Mobject.set_y = _set_y
    _base.Mobject.match_coord = _match_coord
    _base.Mobject.match_x = _match_x
    _base.Mobject.match_y = _match_y
    _base.Mobject.rotate_about_origin = _rotate_about_origin
    if _shared._create_geometry_handle is not None:
        _geometry.Dot.__init__ = _dot_init
        _geometry.Ellipse.__init__ = _ellipse_init
        _geometry.Triangle.__init__ = _triangle_init

    public = {
        "Elbow": Elbow,
        "RoundedRectangle": RoundedRectangle,
        "SurroundingRectangle": SurroundingRectangle,
        "BackgroundRectangle": BackgroundRectangle,
        "Underline": Underline,
        "AnnularSector": AnnularSector,
        "Sector": Sector,
        "Annulus": Annulus,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)


install()
