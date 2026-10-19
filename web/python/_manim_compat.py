"""ManimCE-compatible public authoring foundation for the browser Python frontend.

Python owns public class shape and argument coercion. Shared Rust operations own
semantic geometry, mutation, layout and execution.
"""

from __future__ import annotations

import copy
import math
from collections.abc import Hashable, Iterable, Mapping
from typing import TYPE_CHECKING, Any, Callable, Iterator, Self

import noon as _base
from _manim_iterables import resize_array as _resize_array

if TYPE_CHECKING:
    from manim.typing import Point3DLike

from noon import Mobject, Scene
_ir = _base._ir


class _Direction3(tuple):
    """Immutable three-component arithmetic for the Manim depth directions."""

    __slots__ = ()

    def __new__(cls, x, y, z):
        return tuple.__new__(cls, (
            _ir._finite_number("direction component", float(value)) for value in (x, y, z)
        ))

    def __getnewargs__(self):
        return tuple(self)

    @staticmethod
    def _components(value):
        if isinstance(value, _base.Vec2):
            return (*value, 0.0)
        components = tuple(value)
        if len(components) != 3:
            raise TypeError("direction arithmetic requires three components")
        return tuple(float(component) for component in components)

    def __add__(self, other):
        return _Direction3(*(left + right for left, right in zip(self, self._components(other))))

    def __sub__(self, other):
        return _Direction3(*(left - right for left, right in zip(self, self._components(other))))

    def __neg__(self):
        return self * -1

    def __mul__(self, scalar):
        factor = float(scalar)
        return _Direction3(*(component * factor for component in self))

    def __rmul__(self, scalar):
        return self * scalar

    def __truediv__(self, scalar):
        return self * (1 / float(scalar))


OUT = _Direction3(0.0, 0.0, 1.0)
IN = _Direction3(0.0, 0.0, -1.0)


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


def _manim_rgba_rows(color: _base.Color | None, opacity: float):
    # SemanticStyle is the source of truth and currently represents solid paint.
    # Manim's array facade uses black when a color is omitted and keeps opacity.
    rgb = (0.0, 0.0, 0.0) if color is None else (
        float(color.red), float(color.green), float(color.blue)
    )
    rows = [[*rgb, float(opacity)]]
    try:
        import numpy as np
    except ImportError:
        return rows
    return np.asarray(rows, dtype=np.float64)


def _manim_colors_from_rgbas(rgbas):
    rows = rgbas.tolist() if hasattr(rgbas, "tolist") else rgbas
    colors = []
    for row in rows:
        rgba = [float(component) for component in row]
        colors.append(
            None if not any(rgba) else _base.Color(*rgba[:3])
        )
    return colors


def _manim_opacities_from_rgbas(rgbas):
    rows = rgbas.tolist() if hasattr(rgbas, "tolist") else rgbas
    opacities = [float(row[3]) for row in rows]
    try:
        import numpy as np
    except ImportError:
        return opacities
    return np.asarray(opacities, dtype=np.float64)


def _manim_point3(point):
    values = (float(point.x), float(point.y), 0.0)
    try:
        import numpy as np
    except ImportError:
        return list(values)
    return np.asarray(values, dtype=np.float64)


class VMobject(Mobject):
    """Manim-compatible vector-mobject authoring type over Noon semantic geometry."""

    n_points_per_cubic_curve = 4
    tolerance_for_point_equality = 1e-6

    @property
    def cap_style(self):
        from _manim_semantic_handles import _get_cap_style
        return _get_cap_style(self)

    @cap_style.setter
    def cap_style(self, value):
        from _manim_semantic_handles import _set_cap_style
        _set_cap_style(self, value)

    @property
    def n_points_per_curve(self) -> int:
        """The current cubic stride, matching Manim's mutable instance value."""
        return self.n_points_per_cubic_curve

    def get_group_class(self):
        return VGroup

    @staticmethod
    def get_mobject_type_class():
        return VMobject

    def get_points_defining_boundary(self):
        from _manim_path_queries import boundary_points
        return boundary_points(self)

    def get_point_mobject(self, center=None):
        point = VectorizedPoint(self.get_center() if center is None else center)
        point.match_style(self)
        return point

    def __init__(self, *, color=_base.WHITE, **kwargs):
        # These Python-facing VMobject query settings are part of Manim's
        # constructor contract. Keep them on the wrapper because the shared
        # Rust path remains the authority for geometry and query results.
        tolerance = kwargs.pop(
            "tolerance_for_point_equality", self.tolerance_for_point_equality
        )
        stride = kwargs.pop(
            "n_points_per_cubic_curve", self.n_points_per_cubic_curve
        )
        self.tolerance_for_point_equality = tolerance
        self.n_points_per_cubic_curve = stride
        try:
            import numpy as np
        except ImportError:
            import operator

            sample_count = operator.index(stride)
            if sample_count < 0:
                raise ValueError(
                    f"Number of samples, {sample_count}, must be non-negative."
                )
            if sample_count == 0:
                self._bezier_t_values = []
            elif sample_count == 1:
                self._bezier_t_values = [0.0]
            else:
                self._bezier_t_values = [
                    index / (sample_count - 1) for index in range(sample_count)
                ]
        else:
            self._bezier_t_values = np.linspace(0, 1, stride)
        from _manim_semantic_handles import _empty_path_init
        _empty_path_init(self, color=color, kind="VMobject", **kwargs)

    def consider_points_equals(self, p0, p1):
        from _manim_path_queries import consider_points_equals
        return consider_points_equals(self, p0, p1)

    def consider_points_equals_2d(self, p0, p1):
        from _manim_path_queries import consider_points_equals_2d
        return consider_points_equals_2d(self, p0, p1)

    def set_points_as_corners(self, points):
        from _manim_path_editing import set_points_as_corners
        return set_points_as_corners(self, points)

    def set_points(self, points):
        from _manim_path_editing import set_points
        return set_points(self, points)

    def append_points(self, points):
        from _manim_path_editing import append_points
        return append_points(self, points)

    def resize_points(self, new_length, resize_func=_resize_array):
        from _manim_path_editing import resize_points
        return resize_points(self, new_length, resize_func=resize_func)

    def clear_points(self):
        from _manim_path_editing import clear_points
        return clear_points(self)

    def set_points_smoothly(self, points):
        from _manim_path_editing import set_points_smoothly
        return set_points_smoothly(self, points)

    def make_smooth(self):
        return self.change_anchor_mode("smooth")

    def make_jagged(self):
        return self.change_anchor_mode("jagged")

    def change_anchor_mode(self, mode):
        from _manim_path_editing import change_anchor_mode
        return change_anchor_mode(self, mode)

    def start_new_path(self, point):
        from _manim_path_editing import start_new_path
        return start_new_path(self, point)

    def add_line_to(self, point):
        from _manim_path_editing import add_line_to
        return add_line_to(self, point)

    def add_quadratic_bezier_curve_to(self, control, anchor):
        from _manim_path_editing import add_quadratic_bezier_curve_to
        return add_quadratic_bezier_curve_to(self, control, anchor)

    def add_cubic_bezier_curve_to(self, control1, control2, anchor):
        from _manim_path_editing import add_cubic_bezier_curve_to
        return add_cubic_bezier_curve_to(self, control1, control2, anchor)

    def add_smooth_curve_to(self, *points):
        from _manim_path_editing import add_smooth_curve_to
        return add_smooth_curve_to(self, *points)

    def scale_handle_to_anchor_distances(self, factor):
        from _manim_path_editing import scale_handle_to_anchor_distances
        return scale_handle_to_anchor_distances(self, factor)

    def add_cubic_bezier_curve(self, anchor1, handle1, handle2, anchor2):
        from _manim_path_editing import edit_points
        return edit_points(
            self,
            "addCubicBezierCurve",
            "liveAddCubicBezierCurve",
            (anchor1, handle1, handle2, anchor2),
        )

    def add_cubic_bezier_curves(self, curves):
        from _manim_path_editing import add_cubic_bezier_curves
        return add_cubic_bezier_curves(self, curves)

    def add_subpath(self, points):
        from _manim_path_editing import add_subpath
        return add_subpath(self, points)

    def set_anchors_and_handles(self, anchors1, handles1, handles2, anchors2):
        from _manim_path_editing import set_anchors_and_handles
        return set_anchors_and_handles(self, anchors1, handles1, handles2, anchors2)

    def append_vectorized_mobject(self, source):
        from _manim_path_editing import append_vectorized_mobject
        return append_vectorized_mobject(self, source)

    def add_points_as_corners(self, points):
        from _manim_path_editing import add_points_as_corners
        return add_points_as_corners(self, points)

    def insert_n_curves(self, n):
        from _manim_path_editing import insert_n_curves
        return insert_n_curves(self, n)

    def get_subcurve(self, a, b):
        from _manim_semantic_handles import _clone_mobject
        return _clone_mobject(self, subcurve=(float(a), float(b)))

    def get_subpaths(self):
        from _manim_path_queries import subpaths
        return subpaths(self)

    def get_subpath_split_indices_from_points(self, points, n_dims=3):
        from _manim_path_queries import subpath_split_indices
        return subpath_split_indices(self, points, n_dims)

    def get_subpaths_from_points(self, points):
        from _manim_path_queries import subpaths_from_points
        return subpaths_from_points(self, points)

    def gen_subpaths_from_points_2d(self, points):
        from _manim_path_queries import gen_subpaths_from_points_2d
        return gen_subpaths_from_points_2d(self, points)

    def gen_cubic_bezier_tuples_from_points(self, points):
        from _manim_path_queries import gen_cubic_bezier_tuples_from_points
        return gen_cubic_bezier_tuples_from_points(self, points)

    def get_cubic_bezier_tuples_from_points(self, points):
        from _manim_path_queries import cubic_bezier_tuples_from_points
        return cubic_bezier_tuples_from_points(self, points)

    def get_cubic_bezier_tuples(self):
        from _manim_path_queries import cubic_bezier_tuples
        return cubic_bezier_tuples(self)

    def is_closed(self):
        from _manim_path_queries import is_closed
        return is_closed(self)

    def get_num_curves(self):
        from _manim_path_queries import curve_count
        return curve_count(self)

    def get_nth_curve_points(self, n):
        from _manim_path_queries import curve_points
        return curve_points(self, n)

    def get_nth_curve_length_pieces(self, n, sample_points=None):
        from _manim_path_queries import curve_length_pieces
        return curve_length_pieces(self, n, sample_points)

    def get_nth_curve_length(self, n, sample_points=None):
        from _manim_path_queries import curve_length
        return curve_length(self, n, sample_points)

    def get_nth_curve_function(self, n):
        from _manim_path_queries import curve_function
        return curve_function(self, n)

    def get_nth_curve_function_with_length(self, n, sample_points=None):
        from _manim_path_queries import curve_function_with_length
        return curve_function_with_length(self, n, sample_points)

    def get_curve_functions(self):
        from _manim_path_queries import curve_functions
        yield from curve_functions(self)

    def get_curve_functions_with_lengths(self, **kwargs):
        from _manim_path_queries import curve_functions_with_lengths
        yield from curve_functions_with_lengths(self, **kwargs)

    def get_direction(self):
        from _manim_path_queries import path_direction
        return path_direction(self)

    def force_direction(self, target_direction):
        if target_direction not in ("CW", "CCW"):
            raise ValueError('Invalid input for force_direction. Use "CW" or "CCW"')
        if self.get_direction() != target_direction:
            self.reverse_direction()
        return self

    def get_start_anchors(self):
        from _manim_path_queries import path_points
        return path_points(self, "startAnchors")

    def get_end_anchors(self):
        from _manim_path_queries import path_points
        return path_points(self, "endAnchors")

    def get_anchors(self):
        from _manim_path_queries import path_points
        return path_points(self, "anchors")

    def get_anchors_and_handles(self):
        from _manim_path_queries import anchors_and_handles
        return anchors_and_handles(self)

    def reverse_direction(self):
        from _manim_path_editing import edit_points
        return edit_points(self, "reverseDirection", "liveReverseDirection", ())

    def align_points(self, vmobject):
        from _manim_path_editing import align_points
        return align_points(self, vmobject)

    def align_rgbas(self, vmobject):
        from _manim_path_editing import align_rgbas
        return align_rgbas(self, vmobject)

    def pointwise_become_partial(self, vmobject, a, b):
        from _manim_path_editing import pointwise_become_partial
        return pointwise_become_partial(self, vmobject, a, b)

    def close_path(self):
        from _manim_path_editing import close_path
        return close_path(self)

    def set_color_by_gradient(self, *colors):
        from _manim_semantic_handles import _set_color_by_gradient
        return _set_color_by_gradient(self, *colors)

    set_submobject_colors_by_gradient = set_color_by_gradient

    def get_fill_color(self):
        return self.get_fill_colors()[0]

    def get_fill_rgbas(self):
        from _manim_semantic_handles import _paint_color
        return _manim_rgba_rows(_paint_color(self, "fill"), self.get_fill_opacity())

    def get_fill_colors(self):
        return _manim_colors_from_rgbas(self.get_fill_rgbas())

    def get_fill_opacities(self):
        return _manim_opacities_from_rgbas(self.get_fill_rgbas())

    def get_stroke_color(self, background=False):
        return self.get_stroke_colors(background)[0]

    def get_stroke_rgbas(self, background=False):
        if background:
            from _manim_semantic_handles import (
                _get_background_stroke_color,
                _get_background_stroke_opacity,
            )
            return _manim_rgba_rows(
                _get_background_stroke_color(self),
                _get_background_stroke_opacity(self),
            )
        from _manim_semantic_handles import _paint_color
        return _manim_rgba_rows(_paint_color(self, "stroke"), self.get_stroke_opacity())

    def get_stroke_colors(self, background=False):
        return _manim_colors_from_rgbas(self.get_stroke_rgbas(background))

    def get_stroke_opacities(self, background=False):
        return _manim_opacities_from_rgbas(self.get_stroke_rgbas(background))

    def get_stroke_width(self, background=False):
        if background:
            from _manim_semantic_handles import _get_background_stroke_width
            return _get_background_stroke_width(self)
        from _manim_semantic_handles import _get_stroke_width
        return _get_stroke_width(self, background)

    def get_sheen_factor(self):
        return 0.0

    def get_sheen_direction(self):
        from _manim_semantic_handles import _get_sheen_direction
        return _get_sheen_direction(self)

    def get_gradient_start_and_end_points(self):
        from _manim_semantic_handles import _get_gradient_start_and_end_points
        return _get_gradient_start_and_end_points(self)

    def get_background_image(self):
        return None

    def color_using_background_image(self, background_image):
        from _manim_semantic_handles import _color_using_background_image
        return _color_using_background_image(self, background_image)

    def match_background_image(self, vmobject):
        from _manim_semantic_handles import _match_background_image
        return _match_background_image(self, vmobject)

    def get_style(self, simple=False):
        style = {
            "stroke_opacity": self.get_stroke_opacity(),
            "stroke_width": self.get_stroke_width(),
        }
        if simple:
            style["fill_color"] = self.get_fill_color()
            style["fill_opacity"] = self.get_fill_opacity()
            style["stroke_color"] = self.get_stroke_color()
            return style
        style.update({
            "fill_color": self.get_fill_colors(),
            "fill_opacity": self.get_fill_opacities(),
            "stroke_color": self.get_stroke_colors(),
            "background_stroke_color": self.get_stroke_colors(background=True),
            "background_stroke_width": self.get_stroke_width(background=True),
            "background_stroke_opacity": self.get_stroke_opacity(background=True),
            "sheen_factor": self.get_sheen_factor(),
            "sheen_direction": self.get_sheen_direction(),
            "background_image": self.get_background_image(),
        })
        return style

    def set_style(
        self,
        fill_color=None,
        fill_opacity=None,
        stroke_color=None,
        stroke_width=None,
        stroke_opacity=None,
        background_stroke_color=None,
        background_stroke_width=None,
        background_stroke_opacity=None,
        sheen_factor=None,
        sheen_direction=None,
        background_image=None,
        family=True,
    ):
        from _manim_semantic_handles import _set_style
        return _set_style(self, fill_color, fill_opacity, stroke_color,
                          stroke_width, stroke_opacity, background_stroke_color,
                          background_stroke_width, background_stroke_opacity,
                          sheen_factor, sheen_direction, background_image, family)

    def interpolate_color(self, mobject1, mobject2, alpha):
        from _manim_semantic_handles import _interpolate_manim_style
        return _interpolate_manim_style(self, mobject1, mobject2, alpha)

    def set_background_stroke(self, **kwargs):
        kwargs["background"] = True
        return self.set_stroke(**kwargs)

    def set_cap_style(self, cap_style):
        from _manim_semantic_handles import _set_cap_style
        return _set_cap_style(self, cap_style)

    def set_sheen_direction(self, direction, family=True):
        from _manim_semantic_handles import _set_sheen_direction
        return _set_sheen_direction(self, direction, family)

    def rotate_sheen_direction(self, angle, axis=_base.OUT, family=True):
        from _manim_semantic_handles import _rotate_sheen_direction
        return _rotate_sheen_direction(self, angle, axis, family)

    def set_sheen(self, factor, direction=None, family=True):
        from _manim_semantic_handles import _set_sheen
        return _set_sheen(self, factor, direction, family)

    def match_style(self, vmobject, family=True):
        from _manim_semantic_handles import _match_style
        return _match_style(self, vmobject, family)

    def copy(self) -> VMobject:
        from _manim_semantic_handles import _copy_mobject
        return _copy_mobject(self)

    def set_color(self, color: object, family: bool = True) -> VMobject:
        from _manim_updaters import _canonical_vmobject_set_color
        return _canonical_vmobject_set_color(self, color, family=family)

    def set_fill(self, color: object = None, opacity: float | None = None, family: bool = True) -> VMobject:
        from _manim_updaters import _canonical_vmobject_set_fill
        return _canonical_vmobject_set_fill(self, color=color, opacity=opacity, family=family)

    def set_stroke(
        self,
        color: object = None,
        width: float | None = None,
        opacity: float | None = None,
        background: bool = False,
        family: bool = True,
    ) -> VMobject:
        if background:
            from _manim_semantic_handles import _set_background_stroke
            return _set_background_stroke(self, color, width, opacity, family)
        from _manim_updaters import _canonical_vmobject_set_stroke
        return _canonical_vmobject_set_stroke(self, color=color, width=width, opacity=opacity, family=family)

    def set_opacity(self, opacity: float, family: bool = True) -> VMobject:
        from _manim_updaters import _canonical_vmobject_set_opacity
        return _canonical_vmobject_set_opacity(self, opacity, family=family)

    def get_fill_opacity(self) -> float:
        from _manim_semantic_handles import _get_fill_opacity
        return _get_fill_opacity(self)

    def get_stroke_opacity(self, background=False) -> float:
        if background:
            return float(self.get_stroke_opacities(background=True)[0])
        from _manim_semantic_handles import _get_stroke_opacity
        return _get_stroke_opacity(self)


    def point_from_proportion(self, alpha: float) -> _base.Vec2:
        from _manim_path_queries import point_from_proportion
        return point_from_proportion(self, alpha)

    def proportion_from_point(self, point: object) -> float:
        from _manim_path_queries import _reject_staged_custom_stride_query
        _reject_staged_custom_stride_query(self)
        self.throw_error_if_no_points()
        from _manim_path_queries import proportion_from_point
        return proportion_from_point(self, point)

    def get_last_point(self) -> _base.Vec2:
        from _manim_path_queries import last_point
        return last_point(self)

    def has_new_path_started(self) -> bool:
        from _manim_path_queries import has_new_path_started
        return has_new_path_started(self)

    def get_arc_length(self, sample_points_per_curve: int | None = None) -> float:
        from _manim_path_queries import arc_length
        return arc_length(self, sample_points_per_curve)


class VectorizedPoint(VMobject):
    """Invisible one-point VMobject with Manim's artificial layout extent."""

    def __init__(
        self,
        location=_base.ORIGIN,
        color=_base.BLACK,
        fill_opacity=0,
        stroke_width=0,
        artificial_width=0.01,
        artificial_height=0.01,
        **kwargs,
    ) -> None:
        self.artificial_width = artificial_width
        self.artificial_height = artificial_height
        super().__init__(
            color=color,
            fill_opacity=fill_opacity,
            stroke_width=stroke_width,
            **kwargs,
        )
        self.set_points([location])

    @VMobject.width.getter
    def width(self) -> float:
        return self.artificial_width

    @VMobject.height.getter
    def height(self) -> float:
        return self.artificial_height

    def get_location(self):
        return self.get_start()

    def set_location(self, new_loc):
        self.set_points([new_loc])
        return self


class Circle(VMobject):
    def __init__(
        self,
        radius: float = 1.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        from _manim_semantic_handles import _circle_init
        _circle_init(self, radius, color=color, **kwargs)


class Rectangle(VMobject):
    def __init__(
        self,
        width: float = 4.0,
        height: float = 2.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        from _manim_semantic_handles import _rectangle_init
        _rectangle_init(self, width, height, color=color, **kwargs)


class ScreenRectangle(Rectangle):
    """Manim screen call shape over shared rectangle geometry and bounds."""

    def __init__(
        self, aspect_ratio: float = 16.0 / 9.0, height: float = 4, **kwargs: Any
    ) -> None:
        super().__init__(width=aspect_ratio * height, height=height, **kwargs)

    @property
    def aspect_ratio(self) -> float:
        return self.width / self.height

    @aspect_ratio.setter
    def aspect_ratio(self, value: float) -> None:
        self.stretch_to_fit_width(value * self.height)


class Square(Rectangle):
    def __init__(
        self,
        side_length: float = 2.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        from _manim_semantic_handles import _square_init
        _square_init(self, side_length, color=color, **kwargs)


class Line(VMobject):
    def __init__(
        self,
        start: object = None,
        end: object = None,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        from _manim_semantic_handles import _line_init
        _line_init(self, start, end, color=color, **kwargs)

    def get_start(self) -> _base.Vec2:
        from _manim_geometry import _line_get_start
        return _line_get_start(self)

    def get_end(self) -> _base.Vec2:
        from _manim_geometry import _line_get_end
        return _line_get_end(self)


class Path(VMobject):
    def __init__(
        self,
        path: _base.VectorPath,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        from _manim_semantic_handles import _path_init
        _path_init(self, path, color=color, **kwargs)


def _leaf_mobjects(value: object) -> list[Mobject]:
    if _is_shared_family(value):
        leaves: list[Mobject] = []
        for member in value.submobjects:
            leaves.extend(_leaf_mobjects(member))
        return leaves
    if isinstance(value, Mobject):
        return [value]
    raise TypeError("expected a Mobject or Group")


def _is_shared_family(value: object) -> bool:
    from _manim_semantic_handles import _is_shared_family as is_shared_family
    return is_shared_family(value)


def _rotation_angle_2d(angle: float, axis: object = OUT) -> float:
    try:
        if len(axis) != 3:  # type: ignore[arg-type]
            raise TypeError
        x = float(axis[0])  # type: ignore[index]
        y = float(axis[1])  # type: ignore[index]
        z = float(axis[2])  # type: ignore[index]
    except (TypeError, ValueError, IndexError) as error:
        raise TypeError("rotation axis must be a three-component vector") from error
    if not all(math.isfinite(value) for value in (x, y, z)):
        raise ValueError("rotation axis must be finite")
    if not math.isclose(x, 0.0, abs_tol=1e-12) or not math.isclose(y, 0.0, abs_tol=1e-12):
        raise NotImplementedError("2D authoring supports rotation about the z axis only")
    if math.isclose(z, 0.0, abs_tol=1e-12):
        raise ValueError("rotation axis must be non-zero")
    value = float(angle)
    if not math.isfinite(value):
        raise ValueError("rotation angle must be finite")
    return -value if z < 0.0 else value


def _set_group_metadata(group, name, value):
    # Non-vector Group inherits Manim's deprecated arbitrary-attribute setters.
    # These attributes have no paint or runtime meaning.
    import warnings
    warnings.warn(
        "This method is not guaranteed to stay around. Please prefer "
        "setting the attribute normally or with Mobject.set().",
        DeprecationWarning,
        stacklevel=3,
    )
    setattr(group, name, value)
    return group


class Group(Mobject):
    """Python identities and ergonomics over a shared Rust semantic family."""

    def set_color_by_gradient(self, *colors):
        from _manim_semantic_handles import _set_color_by_gradient
        return _set_color_by_gradient(self, *colors)

    set_submobject_colors_by_gradient = set_color_by_gradient

    def set_style(
        self,
        fill_color=None,
        fill_opacity=None,
        stroke_color=None,
        stroke_width=None,
        stroke_opacity=None,
        background_stroke_color=None,
        background_stroke_width=None,
        background_stroke_opacity=None,
        sheen_factor=None,
        sheen_direction=None,
        background_image=None,
        family=True,
    ):
        from _manim_semantic_handles import _set_style
        return _set_style(self, fill_color, fill_opacity, stroke_color,
                          stroke_width, stroke_opacity, background_stroke_color,
                          background_stroke_width, background_stroke_opacity,
                          sheen_factor, sheen_direction, background_image, family)

    def match_style(self, vmobject, family=True):
        from _manim_semantic_handles import _match_style
        return _match_style(self, vmobject, family)

    def __init__(self, *mobjects: object, z_index: float = 0, color=_base.WHITE) -> None:
        from _manim_semantic_handles import _group_init
        _group_init(self, *mobjects, z_index=z_index, color=color)

    @property
    def submobjects(self) -> list[object]:
        """Ordered shared membership snapshot; edit membership with add/remove."""
        from _manim_semantic_handles import _group_members
        return _group_members(self)

    @property
    def id(self) -> int:
        if not _is_shared_family(self) and getattr(self, "_semantic_handle", None) is not None:
            return _base.Mobject.id.fget(self)
        raise AttributeError("Group has no single runtime object id in Noon")

    @property
    def geometry(self) -> dict[str, Any]:
        if not _is_shared_family(self) and getattr(self, "_semantic_handle", None) is not None:
            return _base.Mobject.geometry.fget(self)
        raise AttributeError("Group has no single runtime geometry in Noon")

    @property
    def transform(self) -> dict[str, Any]:
        if not _is_shared_family(self) and getattr(self, "_semantic_handle", None) is not None:
            return _base.Mobject.transform.fget(self)
        raise AttributeError("Group has no single runtime transform in Noon")

    @property
    def style(self) -> dict[str, Any]:
        if not _is_shared_family(self) and getattr(self, "_semantic_handle", None) is not None:
            return _base.Mobject.style.fget(self)
        raise AttributeError("Group has no single runtime style in Noon")

    def __iter__(self) -> Iterator[object]:
        return iter(self.submobjects)

    def __len__(self) -> int:
        return int(self._semantic_family_handle.memberCount)

    def __getitem__(self, index: int | slice) -> object:
        if isinstance(index, slice):
            group_class = VGroup if isinstance(self, VGroup) else Group
            return group_class(*self.submobjects[index])
        return self.submobjects[index]

    def insert(self, index: int, mobject: Mobject) -> Self:
        from _manim_semantic_handles import _group_insert
        return _group_insert(self, index, mobject)

    def add(self, *mobjects: object) -> Group:
        from _manim_semantic_handles import _group_add
        return _group_add(self, *mobjects)

    def add_to_back(self, *mobjects: object) -> Group:
        from _manim_semantic_handles import _group_add_to_back
        return _group_add_to_back(self, *mobjects)

    def add_n_more_submobjects(self, n: int) -> Self | None:
        from _manim_semantic_handles import _group_add_n_more_submobjects
        return _group_add_n_more_submobjects(self, n)

    def invert(self, recursive: bool = False) -> Self:
        from _manim_semantic_handles import _group_invert
        return _group_invert(self, recursive)

    def sort(
        self,
        point_to_num_func: Callable[[Point3DLike], float] = lambda point: point[0],
        submob_func: Callable[[Mobject], Any] | None = None,
    ) -> Self:
        from _manim_semantic_handles import _group_sort
        return _group_sort(self, point_to_num_func, submob_func)

    def shuffle(self, recursive: bool = False) -> Self:
        from _manim_semantic_handles import _group_shuffle
        return _group_shuffle(self, recursive)

    def remove(self, *mobjects: object) -> Group:
        from _manim_semantic_handles import _group_remove
        return _group_remove(self, *mobjects)

    def copy(self) -> Group:
        from _manim_semantic_handles import _group_copy
        return _group_copy(self)

    def get_center(self) -> _base.Vec2:
        return _base._semantic_operations()._get_center(self)

    def shift(self, direction: object) -> Group:
        return _base._semantic_operations()._group_shift(self, direction)

    def move_to(
        self,
        point_or_mobject: object,
        aligned_edge: object = _base.ORIGIN,
        coor_mask: object = (1.0, 1.0, 1.0),
    ) -> Group:
        return _base._semantic_operations()._group_move_to(self, point_or_mobject, aligned_edge, coor_mask)

    def center(self) -> Group:
        return self.move_to(_base.ORIGIN)

    def set_x(self, x: float, direction: object = _base.ORIGIN) -> Group:
        return self.set_coord(x, 0, direction)

    def set_y(self, y: float, direction: object = _base.ORIGIN) -> Group:
        return self.set_coord(y, 1, direction)

    def scale(self, factor: float | tuple[float, float], *, about_point=None, about_edge=None) -> Group:
        from _manim_semantic_handles import _group_scale
        return _group_scale(self, factor, about_point=about_point, about_edge=about_edge)

    def flip(self, axis=_base.UP, *, about_point=None, about_edge=None):
        return _base._semantic_operations()._flip(self, axis, about_point=about_point, about_edge=about_edge)

    def rotate_about_origin(self, angle, axis=OUT, **kwargs):
        return self.rotate(angle, axis=axis, about_point=_base.ORIGIN, **kwargs)

    def rotate(
        self,
        angle: float,
        axis: object = OUT,
        *,
        about_point: object | None = None,
        about_edge: object | None = None,
        **kwargs: Any,
    ) -> Group:
        from _manim_semantic_handles import _group_rotate
        return _group_rotate(self, angle, axis, about_point=about_point, about_edge=about_edge, **kwargs)

    def set_color(self, color: object, family: bool = True) -> Group:
        if not _is_shared_family(self) and getattr(self, "_semantic_handle", None) is not None:
            return _base.Mobject.set_color(self, color)
        from _manim_semantic_handles import _group_set_color
        return _group_set_color(self, color, family=family)

    def set_fill(self, value: Any) -> Group:
        return _set_group_metadata(self, "fill", value)

    def set_stroke(self, value: Any) -> Group:
        return _set_group_metadata(self, "stroke", value)

    def set_opacity(self, value: Any) -> Group:
        return _set_group_metadata(self, "opacity", value)

    def fade(self, darkness: float = 0.5, family: bool = True) -> Group:
        return _base.Mobject.fade(self, darkness, family)

    def next_to(
        self,
        mobject_or_point: object,
        direction: object = _base.RIGHT,
        buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        aligned_edge: object = _base.ORIGIN,
        submobject_to_align: object | None = None,
        index_of_submobject_to_align: int | None = None,
        coor_mask: object = (1.0, 1.0, 1.0),
    ) -> _base.Mobject | Group:
        return _base._semantic_operations()._next_to(self, mobject_or_point, direction, buff, aligned_edge, submobject_to_align, index_of_submobject_to_align, coor_mask)

    def align_to(self, mobject_or_point: object, direction: object = _base.ORIGIN) -> Group:
        if not _is_shared_family(self):
            return _base.Mobject.align_to(self, mobject_or_point, direction)
        return _base._semantic_operations()._group_align_to(self, mobject_or_point, direction)

    def to_edge(
        self,
        edge: object = None,
        buff: float = _base.DEFAULT_MOBJECT_TO_EDGE_BUFFER,
    ) -> Group:
        return self._align_on_frame(_base._as_vec2(_base.LEFT if edge is None else edge), float(buff))

    def to_corner(
        self,
        corner: object = None,
        buff: float = _base.DEFAULT_MOBJECT_TO_EDGE_BUFFER,
    ) -> Group:
        return self._align_on_frame(_base._as_vec2(_base.DL if corner is None else corner), float(buff))

    def _align_on_frame(self, direction: _base.Vec2, buff: float) -> Group:
        return _base._semantic_operations()._group_align_on_frame(self, direction, buff)


    def arrange(
        self,
        direction: object = _base.RIGHT,
        buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        center: bool = True,
        **kwargs: Any,
    ) -> Group:
        return _base._semantic_operations()._group_arrange(self, direction, buff, center, **kwargs)

    def arrange_submobjects(self, *args: Any, **kwargs: Any) -> Self:
        return self.arrange(*args, **kwargs)

    def arrange_in_grid(
        self, rows: int | None = None, cols: int | None = None,
        buff: float | tuple[float, float] = _base.MED_SMALL_BUFF,
        cell_alignment: object = _base.ORIGIN, row_alignments: str | None = None,
        col_alignments: str | None = None, row_heights=None, col_widths=None,
        flow_order: str = "rd",
    ) -> Group:
        from _manim_semantic_handles import _group_arrange_in_grid
        return _group_arrange_in_grid(self, rows, cols, buff, cell_alignment,
            row_alignments, col_alignments, row_heights, col_widths, flow_order)

    @property
    def animate(self):
        from _manim_animate import _AlignedGroupAnimationBuilder
        return _AlignedGroupAnimationBuilder(self)

    def _copy_for_animate_target(self) -> Group:
        if not _is_shared_family(self) and getattr(self, "_semantic_handle", None) is not None:
            return _base.Mobject._copy_for_animate_target(self)
        return _base._semantic_operations()._group_copy(self)

    def __deepcopy__(self, memo):
        return deepcopy_semantic_wrapper(self, memo)

    def get_color(self) -> _base.Color:
        if not _is_shared_family(self) and getattr(self, "_semantic_handle", None) is not None:
            return _base.Mobject.get_color(self)
        from _manim_geometry import _group_get_color
        return _group_get_color(self)


class VGroup(Group):
    match_style = VMobject.match_style
    interpolate = VMobject.interpolate
    interpolate_color = VMobject.interpolate_color
    # The family wrapper reuses vector-class geometry queries without duplicating them.
    # Paint getters stay on VMobject until the family root has authoritative own-style state.
    cap_style = VMobject.cap_style
    get_group_class = VMobject.get_group_class
    get_mobject_type_class = staticmethod(VMobject.get_mobject_type_class)
    get_points_defining_boundary = VMobject.get_points_defining_boundary
    get_gradient_start_and_end_points = VMobject.get_gradient_start_and_end_points
    get_background_image = VMobject.get_background_image
    get_sheen_factor = VMobject.get_sheen_factor
    get_sheen_direction = VMobject.get_sheen_direction
    set_background_stroke = VMobject.set_background_stroke
    set_cap_style = VMobject.set_cap_style
    color_using_background_image = VMobject.color_using_background_image
    match_background_image = VMobject.match_background_image
    set_sheen_direction = VMobject.set_sheen_direction
    rotate_sheen_direction = VMobject.rotate_sheen_direction
    set_sheen = VMobject.set_sheen
    get_style = VMobject.get_style
    set_style = VMobject.set_style
    get_fill_color = VMobject.get_fill_color
    get_fill_rgbas = VMobject.get_fill_rgbas
    get_fill_colors = VMobject.get_fill_colors
    get_fill_opacities = VMobject.get_fill_opacities
    get_stroke_color = VMobject.get_stroke_color
    get_stroke_rgbas = VMobject.get_stroke_rgbas
    get_stroke_colors = VMobject.get_stroke_colors
    get_stroke_opacities = VMobject.get_stroke_opacities
    get_stroke_width = VMobject.get_stroke_width

    def get_point_mobject(self, center=None):
        if center is None:
            center = self.get_center()
        point = VectorizedPoint(center)
        point.match_style(self, family=False)
        return point

    def get_fill_opacity(self):
        from _manim_semantic_handles import _get_fill_opacity
        return _get_fill_opacity(self)

    def get_stroke_opacity(self, background=False):
        if background:
            return float(self.get_stroke_opacities(background=True)[0])
        from _manim_semantic_handles import _get_stroke_opacity
        return _get_stroke_opacity(self)

    def set_fill(
        self,
        color: object = None,
        opacity: float | None = None,
        family: bool = True,
    ) -> VGroup:
        if not _is_shared_family(self) and getattr(self, "_semantic_handle", None) is not None:
            return _base.Mobject.set_fill(self, color, opacity)
        from _manim_semantic_handles import _group_set_fill
        return _group_set_fill(self, color, opacity, family)

    def set_stroke(self, color: object = None, width: float | None = None,
                   opacity: float | None = None, background: bool = False,
                   family: bool = True) -> VGroup:
        if background:
            from _manim_semantic_handles import _set_background_stroke
            return _set_background_stroke(self, color, width, opacity, family)
        if not _is_shared_family(self) and getattr(self, "_semantic_handle", None) is not None:
            return _base.Mobject.set_stroke(self, color, width, opacity)
        from _manim_semantic_handles import _group_set_stroke
        return _group_set_stroke(self, color, width, opacity, family)

    def set_opacity(self, opacity: float, family: bool = True) -> VGroup:
        if not _is_shared_family(self) and getattr(self, "_semantic_handle", None) is not None:
            return _base.Mobject.set_opacity(self, opacity)
        from _manim_semantic_handles import _group_set_opacity
        return _group_set_opacity(self, opacity, family)

    def make_smooth(self):
        return self.change_anchor_mode("smooth")

    def make_jagged(self):
        return self.change_anchor_mode("jagged")

    def change_anchor_mode(self, mode):
        from _manim_path_editing import change_anchor_mode
        return change_anchor_mode(self, mode)


class CurvesAsSubmobjects(VGroup):
    """Expose each shared-Rust cubic path segment as a styled VMobject child."""

    def __init__(self, vmobject: VMobject, **kwargs: Any) -> None:
        super().__init__(**kwargs)
        for curve in vmobject.get_cubic_bezier_tuples():
            part = VMobject()
            part.set_points(curve)
            part.match_style(vmobject)
            self.add(part)

    def point_from_proportion(self, alpha: float) -> _base.Vec2:
        """Query this piecewise family by cumulative shared-Rust arc length."""
        if alpha < 0 or alpha > 1:
            raise ValueError(f"Alpha {alpha} not between 0 and 1.")

        if len(self.submobjects) == 0:
            raise Exception(
                "Cannot call CurvesAsSubmobjects. point_from_proportion for a "
                "CurvesAsSubmobject with no submobjects"
            )
        parts = [part for part in self.submobjects if part.get_num_points() > 0]
        if not parts:
            raise Exception(
                "Cannot call CurvesAsSubmobjects. point_from_proportion for a "
                "CurvesAsSubmobject whose submobjects have no points"
            )
        if alpha == 1:
            return parts[-1].point_from_proportion(1)

        lengths = [part.get_arc_length() for part in parts]
        target_length = alpha * sum(lengths)
        current_length = 0.0
        for part, length in zip(parts, lengths):
            if current_length + length >= target_length:
                residue = (target_length - current_length) / length
                return part.point_from_proportion(residue)
            current_length += length


class VDict(VGroup, VMobject):
    """A shared-family vector group with Manim's key-to-mobject access."""

    def __init__(
        self,
        mapping_or_iterable: Mapping[Hashable, VMobject]
        | Iterable[tuple[Hashable, VMobject]] = {},
        show_keys: bool = False,
        **kwargs: Any,
    ) -> None:
        if show_keys:
            raise NotImplementedError(
                "VDict show_keys requires adding text submobjects to geometry objects"
            )
        super().__init__(**kwargs)
        self.show_keys = show_keys
        self.submob_dict: dict[Hashable, VMobject] = {}
        self.add(mapping_or_iterable)

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}({self.submob_dict!r})"

    def add(
        self,
        mapping_or_iterable: Mapping[Hashable, VMobject]
        | Iterable[tuple[Hashable, VMobject]],
    ) -> Self:
        for key, value in dict(mapping_or_iterable).items():
            self.add_key_value_pair(key, value)
        return self

    def remove(self, key: Hashable) -> Self:
        if key not in self.submob_dict:
            raise KeyError(f"The given key '{key!s}' is not present in the VDict")
        super().remove(self.submob_dict[key])
        del self.submob_dict[key]
        return self

    def __getitem__(self, key: Hashable) -> VMobject:
        return self.submob_dict[key]

    def __setitem__(self, key: Hashable, value: VMobject) -> None:
        if key in self.submob_dict:
            self.remove(key)
        self.add([(key, value)])

    def __delitem__(self, key: Hashable) -> None:
        # Manim deliberately removes only the key here; its family member remains.
        del self.submob_dict[key]

    def __contains__(self, key: object) -> bool:
        return key in self.submob_dict

    def get_all_submobjects(self):
        return self.submob_dict.values()

    def add_key_value_pair(self, key: Hashable, value: VMobject) -> Self:
        if not isinstance(value, (VMobject, VGroup)):
            raise TypeError(
                f"VDict values must be VMobjects, got {type(value).__name__}"
            )
        if self.show_keys:
            raise NotImplementedError(
                "VDict show_keys requires adding text submobjects to geometry objects"
            )
        self.submob_dict[key] = value
        super().add(value)
        return self


def _mobject_generate_target(self: Mobject, use_deepcopy: bool = False) -> Mobject:
    """Capture a detached target through the shared Rust target editor."""
    del use_deepcopy
    previous = getattr(self, "target", None)
    # A target must not recursively clone the previously generated target chain.
    # Preserve the wrapper's previous target if shared capture is rejected.
    self.target = None
    try:
        target = self._copy_for_animate_target()
    except BaseException:
        self.target = previous
        raise
    self.target = target
    return target


def _mobject_save_state(self: Mobject) -> Mobject:
    if hasattr(self, "saved_state"):
        self.saved_state = None
    self.saved_state = self.copy()
    return self


def _mobject_restore(self: Mobject) -> Mobject:
    if not hasattr(self, "saved_state") or self.saved_state is None:
        raise Exception("Trying to restore without having saved")
    return self.become(self.saved_state)


class MoveToTarget:
    """Manim target-editor request over the ordinary shared Transform path."""

    def __new__(cls, mobject: object, **kwargs: Any):
        if not isinstance(mobject, Mobject):
            raise TypeError("MoveToTarget target must be a Mobject")
        if not hasattr(mobject, "target"):
            raise ValueError("MoveToTarget called on mobject without attribute 'target'")
        target = mobject.target
        if not isinstance(target, Mobject):
            raise TypeError("MoveToTarget target state must be a Mobject")
        return _base.Transform(mobject, target, **kwargs)


class Restore:
    """Animate to the saved shared-semantic snapshot with ordinary Transform."""

    def __new__(cls, mobject: object, **kwargs: Any):
        if not isinstance(mobject, Mobject):
            raise TypeError("Restore target must be a Mobject")
        target = getattr(mobject, "saved_state", None)
        if not isinstance(target, Mobject):
            raise Exception("Trying to restore without having saved")
        return _base.Transform(mobject, target, **kwargs)


_FAMILY_COPY_METADATA = object()


def deepcopy_semantic_wrapper(self, memo):
    """Participate in a metadata preparation pass without copying engine handles."""
    allocate = memo.get(_FAMILY_COPY_METADATA)
    if allocate is not None:
        return allocate(self)
    clone = self.copy()
    memo[id(self)] = clone
    return clone


def prepare_family_wrapper_copy(source: Group, excluded_fields):
    """Prepare wrapper metadata, including saved-state references, before commit.

    No constructors or semantic operations run here. Rust copies all referenced
    nodes in one transaction after the fallible host metadata pass has completed.
    """
    pairs = []
    family_members = []
    memo = {}

    def allocate(value):
        existing = memo.get(id(value))
        if existing is not None:
            return existing
        clone = object.__new__(type(value))
        memo[id(value)] = clone
        pairs.append((value, clone))
        if _is_shared_family(value):
            family_members.append((clone, [allocate(member) for member in value.submobjects]))
        return clone

    memo[_FAMILY_COPY_METADATA] = allocate
    root = allocate(source)
    # Deepcopy can discover saved states or wrappers nested in arbitrary metadata
    # containers. Their hooks enqueue metadata work instead of touching Rust.
    index = 0
    while index < len(pairs):
        original, clone = pairs[index]
        index += 1
        excluded = excluded_fields(original)
        copy_wrapper_attributes(original, clone, memo, excluded | {"submobjects"})
    return root, pairs, family_members


def copy_wrapper_attributes(source, target, memo=None, excluded=()):
    """Copy host-language attributes using an optional family identity memo."""
    for name, value in source.__dict__.items():
        if name not in excluded:
            setattr(target, name, copy.deepcopy(value, memo))
